//! Execution of actions and macro steps (§ 5), on the service's executor thread.

use declic_core::typing::plan;
use declic_core::vars::{DateNames, expand};
use declic_core::{Action, ClickKind, HeldKey, Hotkey, Key, Modifier, MouseButton, MoveOrigin, OpenAction, Step, TextMode};
use declic_win::inject;
use declic_win::launch::LaunchError;
use declic_win::system::SystemVars;
use declic_win::winfind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Something to tell the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    NotFound(String),
    LaunchFailed(String, u32),
    Elevated,
    InjectFailed,
    /// A window could not be found (description of the criteria).
    WindowNotFound(String),
    /// Text of a "notification" macro step (always shown).
    Message(String),
    /// The user interrupted the macro with the stop key.
    Stopped,
}

/// How an action ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Stopped,
    Failed,
}

/// Environment of an execution.
pub struct Env<'a> {
    pub names: DateNames,
    pub typing_delay: Duration,
    pub abort: &'a AtomicBool,
    pub notify: &'a dyn Fn(Notice),
}

impl Env<'_> {
    fn aborted(&self) -> bool {
        self.abort.load(Ordering::Relaxed)
    }

    fn expand(&self, text: &str) -> String {
        expand(text, &SystemVars, &self.names)
    }

    /// Sleeps, waking up early when the user interrupts the macro.
    fn sleep(&self, duration: Duration) -> bool {
        let mut left = duration;
        while !left.is_zero() {
            if self.aborted() {
                return false;
            }
            let slice = left.min(Duration::from_millis(20));
            std::thread::sleep(slice);
            left -= slice;
        }
        !self.aborted()
    }
}

/// Pause between two macro steps, so that applications can process the
/// previous input.
const STEP_GAP: Duration = Duration::from_millis(15);
/// Time left to the application to read the clipboard after "paste".
const PASTE_SETTLE: Duration = Duration::from_millis(500);

/// Runs an action.
pub fn run(action: &Action, env: &Env) -> Outcome {
    match action {
        Action::Open(open) => {
            inject::mask_menu_keys();
            open_target(open, env)
        }
        Action::Macro(m) => {
            let steps: Vec<&Step> = m.steps.iter().filter(|s| s.enabled).map(|s| &s.step).collect();
            run_steps(&steps, env)
        }
    }
}

fn open_target(open: &OpenAction, env: &Env) -> Outcome {
    let target = env.expand(&open.target);
    let arguments = env.expand(&open.arguments);
    let working_dir = env.expand(&open.working_dir);
    match declic_win::launch::open(&target, &arguments, &working_dir, open.window, open.run_as_admin) {
        Ok(()) => Outcome::Done,
        Err(LaunchError::NotFound) => {
            (env.notify)(Notice::NotFound(target));
            Outcome::Failed
        }
        Err(LaunchError::Cancelled) => Outcome::Failed,
        Err(LaunchError::Failed(code)) => {
            (env.notify)(Notice::LaunchFailed(target, code));
            Outcome::Failed
        }
    }
}

/// State of a running macro, cleaned up at the end whatever happens.
struct Running {
    held: Vec<HeldKey>,
    buttons: Vec<MouseButton>,
}

fn run_steps(steps: &[&Step], env: &Env) -> Outcome {
    // Keys still held by the user (e.g. Ctrl+Alt of Ctrl+Alt+M) must not
    // alter what the macro sends (F-TXT-03).
    let released = match inject::release_modifiers() {
        Ok(released) => released,
        Err(_) => {
            (env.notify)(Notice::InjectFailed);
            return Outcome::Failed;
        }
    };
    let mut state = Running { held: Vec::new(), buttons: Vec::new() };
    let mut outcome = Outcome::Done;
    for (i, step) in steps.iter().enumerate() {
        if env.aborted() {
            outcome = Outcome::Stopped;
            break;
        }
        if i > 0 && !env.sleep(STEP_GAP) {
            outcome = Outcome::Stopped;
            break;
        }
        match run_step(step, env, &mut state) {
            Outcome::Done => {}
            other => {
                outcome = other;
                break;
            }
        }
    }
    if !state.held.is_empty() {
        let _ = inject::release_keys(&state.held);
    }
    for button in state.buttons.drain(..) {
        inject::release_button(button);
    }
    inject::restore_modifiers(released);
    if outcome == Outcome::Stopped {
        (env.notify)(Notice::Stopped);
    }
    outcome
}

fn failed_input(env: &Env) -> Outcome {
    (env.notify)(Notice::InjectFailed);
    Outcome::Failed
}

fn check_elevated(env: &Env) -> bool {
    if declic_win::process::foreground_is_elevated_above_us() {
        (env.notify)(Notice::Elevated);
        return false;
    }
    true
}

fn describe_window(title: &str, program: &str) -> String {
    match (title.trim().is_empty(), program.trim().is_empty()) {
        (false, false) => format!("{title} ({program})"),
        (false, true) => title.to_string(),
        _ => program.to_string(),
    }
}

fn run_step(step: &Step, env: &Env, state: &mut Running) -> Outcome {
    match step {
        Step::TypeText { text, mode } => {
            if !check_elevated(env) {
                return Outcome::Failed;
            }
            let text = env.expand(text);
            match mode {
                TextMode::Typing => {
                    let units = plan(&text);
                    // Chunks keep the macro interruptible during long texts.
                    for chunk in units.chunks(256) {
                        if env.aborted() {
                            return Outcome::Stopped;
                        }
                        if inject::type_units(chunk, env.typing_delay).is_err() {
                            return failed_input(env);
                        }
                    }
                    Outcome::Done
                }
                TextMode::Paste => paste(&text, env),
            }
        }
        Step::PressKeys { keys, repeat } => {
            if !check_elevated(env) {
                return Outcome::Failed;
            }
            for _ in 0..(*repeat).max(1) {
                if env.aborted() {
                    return Outcome::Stopped;
                }
                if inject::press_hotkey(keys, 1, env.typing_delay).is_err() {
                    return failed_input(env);
                }
            }
            Outcome::Done
        }
        Step::HoldKeys { keys } => {
            let new: Vec<HeldKey> = keys.0.iter().copied().filter(|k| !state.held.contains(k)).collect();
            if inject::hold_keys(&new).is_err() {
                return failed_input(env);
            }
            state.held.extend(new);
            Outcome::Done
        }
        Step::ReleaseKeys { keys } => {
            let to_release: Vec<HeldKey> =
                if keys.is_empty() { std::mem::take(&mut state.held) } else { keys.0.clone() };
            state.held.retain(|k| !to_release.contains(k));
            if inject::release_keys(&to_release).is_err() {
                return failed_input(env);
            }
            Outcome::Done
        }
        Step::Wait { ms } => {
            if env.sleep(Duration::from_millis(*ms as u64)) { Outcome::Done } else { Outcome::Stopped }
        }
        Step::ActivateWindow { title, program, timeout_ms, if_missing } => {
            let title = env.expand(title);
            let stop = || env.aborted();
            match winfind::wait_for(&title, program, Duration::from_millis(*timeout_ms as u64), &stop) {
                Some(window) => {
                    winfind::activate(window.hwnd);
                    Outcome::Done
                }
                None if env.aborted() => Outcome::Stopped,
                None => window_missing(&title, program, *if_missing, env),
            }
        }
        Step::ActivateOrLaunch { title, program, timeout_ms, if_missing, launch } => {
            let title = env.expand(title);
            if let Some(window) = winfind::find(&title, program) {
                winfind::activate(window.hwnd);
                return Outcome::Done;
            }
            if open_target(launch, env) != Outcome::Done {
                return Outcome::Failed;
            }
            let stop = || env.aborted();
            match winfind::wait_for(&title, program, Duration::from_millis(*timeout_ms as u64), &stop) {
                Some(window) => {
                    winfind::activate(window.hwnd);
                    Outcome::Done
                }
                None if env.aborted() => Outcome::Stopped,
                None => window_missing(&title, program, *if_missing, env),
            }
        }
        Step::Open(open) => open_target(open, env),
        Step::CopyText { text } => {
            if declic_win::clipboard::set_text(&env.expand(text), false) { Outcome::Done } else { failed_input(env) }
        }
        Step::Click { button, kind } => {
            if inject::click(*button, *kind).is_err() {
                return failed_input(env);
            }
            match kind {
                ClickKind::Down => state.buttons.push(*button),
                ClickKind::Up => state.buttons.retain(|b| b != button),
                _ => {}
            }
            Outcome::Done
        }
        Step::MoveMouse { x, y, origin } => {
            let (bx, by) = match origin {
                MoveOrigin::Screen => (0, 0),
                MoveOrigin::Window => winfind::foreground().map(|w| (w.rect.0, w.rect.1)).unwrap_or((0, 0)),
                MoveOrigin::Cursor => inject::cursor_pos(),
            };
            if inject::move_cursor(bx + x, by + y).is_err() { failed_input(env) } else { Outcome::Done }
        }
        Step::Wheel { direction, notches } => {
            if inject::wheel(*direction, *notches).is_err() { failed_input(env) } else { Outcome::Done }
        }
        Step::Notify { text } => {
            (env.notify)(Notice::Message(env.expand(text)));
            Outcome::Done
        }
    }
}

fn window_missing(title: &str, program: &str, if_missing: declic_core::IfMissing, env: &Env) -> Outcome {
    (env.notify)(Notice::WindowNotFound(describe_window(title, program)));
    match if_missing {
        declic_core::IfMissing::Stop => Outcome::Failed,
        declic_core::IfMissing::Continue => Outcome::Done,
    }
}

/// Paste mode (F-TXT-04): the text goes through the clipboard, whose previous
/// content is restored afterwards. The temporary content is excluded from
/// the clipboard history.
fn paste(text: &str, env: &Env) -> Outcome {
    let backup = declic_win::clipboard::backup();
    if !declic_win::clipboard::set_text(text, true) {
        return failed_input(env);
    }
    let ctrl_v = Hotkey::new(Key::V).with(Modifier::Ctrl, declic_core::ModReq::Any);
    let pressed = inject::press_hotkey(&ctrl_v, 1, Duration::ZERO);
    std::thread::sleep(PASTE_SETTLE);
    declic_win::clipboard::restore(&backup);
    if pressed.is_err() { failed_input(env) } else { Outcome::Done }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_descriptions() {
        assert_eq!(describe_window("*Bloc-notes", ""), "*Bloc-notes");
        assert_eq!(describe_window("", "notepad.exe"), "notepad.exe");
        assert_eq!(describe_window("Doc", "notepad.exe"), "Doc (notepad.exe)");
    }

    #[test]
    fn interrupted_wait_stops_early() {
        let abort = AtomicBool::new(true);
        let env = Env { names: DateNames::default(), typing_delay: Duration::ZERO, abort: &abort, notify: &|_| {} };
        let start = std::time::Instant::now();
        let steps = [&Step::Wait { ms: 5000 }];
        // Aborted before the first step: nothing is sent, the macro stops.
        assert_eq!(run_steps(&steps, &env), Outcome::Stopped);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
