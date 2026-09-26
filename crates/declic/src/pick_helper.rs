//! `declic.exe --pick <hint>`: eyedropper (F-MAC-04, F-CND-02).
//!
//! A window-less helper started by the main window. It shows a small label
//! next to the pointer; the next left click is swallowed and reported on
//! standard output (position, window rectangle, program, title). A right
//! click or Esc cancels. Like `--capture`, it runs in its own process because
//! low-level hooks are not reliable in the process owning the foreground
//! window.

use declic_win::hook::{Decision, KeyboardHook};
use declic_win::keys::KeyInput;
use declic_win::mouse_hook::{MouseAction, MouseHook};
use declic_win::tip::Tip;
use std::io::{Read, Write};
use std::time::Duration;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_APP,
};

const MSG_MOVE: u32 = WM_APP + 1;
const MSG_PICK: u32 = WM_APP + 2;
const MSG_CANCEL: u32 = WM_APP + 3;
const SAFETY_TIMEOUT: Duration = Duration::from_secs(120);

fn post(thread: u32, msg: u32, x: i32, y: i32) {
    // SAFETY: posting a thread message with plain integer parameters.
    unsafe {
        let _ = PostThreadMessageW(thread, msg, WPARAM(x as u32 as usize), LPARAM(y as isize));
    }
}

/// A picked point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub x: i32,
    pub y: i32,
    pub rect: (i32, i32, i32, i32),
    pub program: String,
    pub title: String,
}

pub fn format_line(p: &Picked) -> String {
    format!(
        "pick {} {} {} {} {} {} {}\t{}",
        p.x,
        p.y,
        p.rect.0,
        p.rect.1,
        p.rect.2,
        p.rect.3,
        if p.program.is_empty() { "-" } else { &p.program },
        p.title.replace(['\r', '\n'], " ")
    )
}

pub fn parse_line(line: &str) -> Option<Picked> {
    let rest = line.strip_prefix("pick ")?;
    let (numbers, title) = rest.split_once('\t').unwrap_or((rest, ""));
    let parts: Vec<&str> = numbers.split_whitespace().collect();
    if parts.len() != 7 {
        return None;
    }
    let n = |i: usize| parts[i].parse::<i32>().ok();
    Some(Picked {
        x: n(0)?,
        y: n(1)?,
        rect: (n(2)?, n(3)?, n(4)?, n(5)?),
        program: if parts[6] == "-" { String::new() } else { parts[6].to_string() },
        title: title.to_string(),
    })
}

pub fn run(hint: &str) {
    declic_win::window::set_dpi_aware();
    // SAFETY: plain query.
    let main_thread = unsafe { GetCurrentThreadId() };
    let Some(tip) = Tip::new(hint) else {
        println!("cancel");
        return;
    };
    let mouse = MouseHook::start(move |event| match event.action {
        MouseAction::Move => {
            post(main_thread, MSG_MOVE, event.x, event.y);
            Decision::Pass
        }
        MouseAction::LeftDown => {
            post(main_thread, MSG_PICK, event.x, event.y);
            Decision::Swallow
        }
        MouseAction::LeftUp => Decision::Swallow,
        MouseAction::RightDown => {
            post(main_thread, MSG_CANCEL, 0, 0);
            Decision::Swallow
        }
        MouseAction::RightUp => Decision::Swallow,
        _ => Decision::Pass,
    });
    let keyboard = KeyboardHook::start(move |event| {
        if event.input == KeyInput::Key(declic_core::Key::Escape) {
            if event.down {
                post(main_thread, MSG_CANCEL, 0, 0);
            }
            return Decision::Swallow;
        }
        Decision::Pass
    });
    if mouse.is_err() || keyboard.is_err() {
        println!("cancel");
        return;
    }
    // Ends when the main window closes our input, or after a safety delay.
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
        post(main_thread, MSG_CANCEL, 0, 0);
    });
    std::thread::spawn(move || {
        std::thread::sleep(SAFETY_TIMEOUT);
        post(main_thread, MSG_CANCEL, 0, 0);
    });
    let (x, y) = declic_win::inject::cursor_pos();
    tip.show_at(x, y, &format!("{hint}\n{x}, {y}"));
    let mut msg = MSG::default();
    let mut result = "cancel".to_string();
    // SAFETY: standard message loop of this thread.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            match msg.message {
                MSG_MOVE => {
                    let (x, y) = (msg.wParam.0 as u32 as i32, msg.lParam.0 as i32);
                    tip.show_at(x, y, &format!("{hint}\n{x}, {y}"));
                }
                MSG_PICK => {
                    let (x, y) = (msg.wParam.0 as u32 as i32, msg.lParam.0 as i32);
                    let window = declic_win::winfind::window_at(x, y);
                    let picked = Picked {
                        x,
                        y,
                        rect: window.as_ref().map(|w| w.rect).unwrap_or((0, 0, 0, 0)),
                        program: window.as_ref().and_then(|w| w.program.clone()).unwrap_or_default(),
                        title: window.map(|w| w.title).unwrap_or_default(),
                    };
                    result = format_line(&picked);
                    break;
                }
                MSG_CANCEL => break,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
    drop(mouse);
    drop(keyboard);
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{result}");
    let _ = out.flush();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_round_trip() {
        let p = Picked { x: -5, y: 10, rect: (0, 0, 800, 600), program: "notepad.exe".into(), title: "Sans titre - Bloc-notes".into() };
        assert_eq!(parse_line(&format_line(&p)), Some(p.clone()));
        let empty = Picked { program: String::new(), title: String::new(), ..p };
        assert_eq!(parse_line(&format_line(&empty)), Some(empty));
        assert_eq!(parse_line("cancel"), None);
    }
}
