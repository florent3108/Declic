//! Macro steps (§ 5.3).
//!
//! A macro is an ordered list of [`MacroStep`]s, each wrapping a [`Step`] and
//! an "enabled" flag (a disabled step is kept but skipped).

use crate::hotkey::{Hotkey, ModReq, Modifier, Side};
use crate::key::Key;
use crate::model::{OpenAction, TextMode};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn one() -> u32 {
    1
}

fn is_one(n: &u32) -> bool {
    *n == 1
}

fn default_timeout() -> u32 {
    5000
}

/// Default time to wait for a window, in milliseconds.
pub const DEFAULT_WINDOW_TIMEOUT_MS: u32 = 5000;

/// A key pressed by a macro: a modifier with its side, or any other key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeldKey {
    Modifier(Modifier, Side),
    Key(Key),
}

/// A list of keys, written like `Ctrl+Shift+A` (a modifier without side
/// means its left key; `RightCtrl` designates the right one).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct KeyList(pub Vec<HeldKey>);

impl KeyList {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Keys of a combination: its modifiers (left side unless specified) then its key.
    pub fn from_hotkey(hotkey: &Hotkey) -> KeyList {
        let mut keys = Vec::new();
        for m in Modifier::ALL {
            match hotkey.req(m) {
                ModReq::Off => {}
                ModReq::Right => keys.push(HeldKey::Modifier(m, Side::Right)),
                ModReq::Any | ModReq::Left => keys.push(HeldKey::Modifier(m, Side::Left)),
            }
        }
        keys.push(HeldKey::Key(hotkey.key));
        KeyList(keys)
    }
}

impl fmt::Display for KeyList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|k| match k {
                HeldKey::Modifier(m, Side::Left) => m.id().to_string(),
                HeldKey::Modifier(m, Side::Right) => format!("Right{}", m.id()),
                HeldKey::Key(key) => key.id().into_owned(),
            })
            .collect();
        f.write_str(&parts.join("+"))
    }
}

impl FromStr for KeyList {
    type Err = crate::hotkey::HotkeyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || crate::hotkey::HotkeyParseError(s.to_string());
        if s.trim().is_empty() {
            return Ok(KeyList::default());
        }
        let mut keys = Vec::new();
        for token in s.split('+').map(str::trim) {
            if token.is_empty() {
                return Err(err());
            }
            let key = match crate::hotkey::parse_modifier_token(token) {
                Some((m, ModReq::Right)) => HeldKey::Modifier(m, Side::Right),
                Some((m, _)) => HeldKey::Modifier(m, Side::Left),
                None => HeldKey::Key(Key::from_id(token).ok_or_else(err)?),
            };
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        Ok(KeyList(keys))
    }
}

impl Serialize for KeyList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeyList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// What to do when a window cannot be found in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IfMissing {
    /// Stop the macro (default, safer).
    #[default]
    Stop,
    /// Go on with the next step.
    Continue,
}

impl IfMissing {
    fn is_default(&self) -> bool {
        *self == IfMissing::Stop
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub const ALL: [MouseButton; 3] = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickKind {
    #[default]
    Single,
    Double,
    /// Press the button without releasing it.
    Down,
    /// Release a pressed button.
    Up,
}

impl ClickKind {
    pub const ALL: [ClickKind; 4] = [ClickKind::Single, ClickKind::Double, ClickKind::Down, ClickKind::Up];
}

/// Reference point of a mouse move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveOrigin {
    /// Absolute position on the (virtual) screen.
    #[default]
    Screen,
    /// Relative to the top-left corner of the active window.
    Window,
    /// Relative to the current pointer position.
    Cursor,
}

impl MoveOrigin {
    pub const ALL: [MoveOrigin; 3] = [MoveOrigin::Screen, MoveOrigin::Window, MoveOrigin::Cursor];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelDirection {
    Up,
    #[default]
    Down,
    Left,
    Right,
}

impl WheelDirection {
    pub const ALL: [WheelDirection; 4] =
        [WheelDirection::Up, WheelDirection::Down, WheelDirection::Left, WheelDirection::Right];
}

/// One step of a macro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    /// Type (or paste) a text; variables are expanded.
    TypeText {
        text: String,
        #[serde(default, skip_serializing_if = "TextMode::is_typing")]
        mode: TextMode,
    },
    /// Press a key combination, possibly several times.
    PressKeys {
        keys: Hotkey,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        repeat: u32,
    },
    /// Keep keys pressed during the following steps.
    HoldKeys { keys: KeyList },
    /// Release held keys (all of them when the list is empty).
    ReleaseKeys {
        #[serde(default, skip_serializing_if = "KeyList::is_empty")]
        keys: KeyList,
    },
    /// Pause, in milliseconds.
    Wait { ms: u32 },
    /// Bring a window to the foreground, found by title (with `*`/`?`
    /// wildcards) and/or program, waiting for it at most `timeout_ms`.
    ActivateWindow {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        program: String,
        #[serde(default = "default_timeout")]
        timeout_ms: u32,
        #[serde(default, skip_serializing_if = "IfMissing::is_default")]
        if_missing: IfMissing,
    },
    /// Like `ActivateWindow`, but opens `launch` first when the window does
    /// not exist, then waits for it to appear.
    ActivateOrLaunch {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        program: String,
        #[serde(default = "default_timeout")]
        timeout_ms: u32,
        #[serde(default, skip_serializing_if = "IfMissing::is_default")]
        if_missing: IfMissing,
        launch: OpenAction,
    },
    /// Same parameters as the *Open* action.
    Open(OpenAction),
    /// Put a text in the clipboard; variables are expanded.
    CopyText { text: String },
    /// Mouse click at the current pointer position.
    Click {
        #[serde(default)]
        button: MouseButton,
        #[serde(default)]
        kind: ClickKind,
    },
    /// Move the mouse pointer.
    MoveMouse {
        x: i32,
        y: i32,
        #[serde(default)]
        origin: MoveOrigin,
    },
    /// Turn the mouse wheel.
    Wheel {
        #[serde(default)]
        direction: WheelDirection,
        #[serde(default = "one")]
        notches: u32,
    },
    /// Show a short notification; variables are expanded.
    Notify { text: String },
}

/// Kind of a step, without its parameters (for menus and labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepKind {
    TypeText,
    PressKeys,
    HoldKeys,
    ReleaseKeys,
    Wait,
    ActivateWindow,
    ActivateOrLaunch,
    Open,
    CopyText,
    Click,
    MoveMouse,
    Wheel,
    Notify,
}

impl StepKind {
    pub const ALL: [StepKind; 13] = [
        StepKind::TypeText,
        StepKind::PressKeys,
        StepKind::HoldKeys,
        StepKind::ReleaseKeys,
        StepKind::Wait,
        StepKind::ActivateWindow,
        StepKind::ActivateOrLaunch,
        StepKind::Open,
        StepKind::CopyText,
        StepKind::Click,
        StepKind::MoveMouse,
        StepKind::Wheel,
        StepKind::Notify,
    ];

    /// Stable identifier (also used as translation key suffix).
    pub fn id(self) -> &'static str {
        match self {
            StepKind::TypeText => "type_text",
            StepKind::PressKeys => "press_keys",
            StepKind::HoldKeys => "hold_keys",
            StepKind::ReleaseKeys => "release_keys",
            StepKind::Wait => "wait",
            StepKind::ActivateWindow => "activate_window",
            StepKind::ActivateOrLaunch => "activate_or_launch",
            StepKind::Open => "open",
            StepKind::CopyText => "copy_text",
            StepKind::Click => "click",
            StepKind::MoveMouse => "move_mouse",
            StepKind::Wheel => "wheel",
            StepKind::Notify => "notify",
        }
    }

    /// A new step of this kind with default parameters.
    pub fn default_step(self) -> Step {
        match self {
            StepKind::TypeText => Step::TypeText { text: String::new(), mode: TextMode::Typing },
            StepKind::PressKeys => Step::PressKeys { keys: Hotkey::new(Key::Enter), repeat: 1 },
            StepKind::HoldKeys => Step::HoldKeys { keys: KeyList(vec![HeldKey::Modifier(Modifier::Shift, Side::Left)]) },
            StepKind::ReleaseKeys => Step::ReleaseKeys { keys: KeyList::default() },
            StepKind::Wait => Step::Wait { ms: 200 },
            StepKind::ActivateWindow => Step::ActivateWindow {
                title: String::new(),
                program: String::new(),
                timeout_ms: DEFAULT_WINDOW_TIMEOUT_MS,
                if_missing: IfMissing::Stop,
            },
            StepKind::ActivateOrLaunch => Step::ActivateOrLaunch {
                title: String::new(),
                program: String::new(),
                timeout_ms: DEFAULT_WINDOW_TIMEOUT_MS,
                if_missing: IfMissing::Stop,
                launch: OpenAction::default(),
            },
            StepKind::Open => Step::Open(OpenAction::default()),
            StepKind::CopyText => Step::CopyText { text: String::new() },
            StepKind::Click => Step::Click { button: MouseButton::Left, kind: ClickKind::Single },
            StepKind::MoveMouse => Step::MoveMouse { x: 0, y: 0, origin: MoveOrigin::Screen },
            StepKind::Wheel => Step::Wheel { direction: WheelDirection::Down, notches: 3 },
            StepKind::Notify => Step::Notify { text: String::new() },
        }
    }
}

impl Step {
    pub fn kind(&self) -> StepKind {
        match self {
            Step::TypeText { .. } => StepKind::TypeText,
            Step::PressKeys { .. } => StepKind::PressKeys,
            Step::HoldKeys { .. } => StepKind::HoldKeys,
            Step::ReleaseKeys { .. } => StepKind::ReleaseKeys,
            Step::Wait { .. } => StepKind::Wait,
            Step::ActivateWindow { .. } => StepKind::ActivateWindow,
            Step::ActivateOrLaunch { .. } => StepKind::ActivateOrLaunch,
            Step::Open(_) => StepKind::Open,
            Step::CopyText { .. } => StepKind::CopyText,
            Step::Click { .. } => StepKind::Click,
            Step::MoveMouse { .. } => StepKind::MoveMouse,
            Step::Wheel { .. } => StepKind::Wheel,
            Step::Notify { .. } => StepKind::Notify,
        }
    }

    /// Texts of the step, for search.
    pub fn texts(&self) -> Vec<&str> {
        match self {
            Step::TypeText { text, .. } | Step::CopyText { text } | Step::Notify { text } => vec![text],
            Step::ActivateWindow { title, program, .. } => vec![title, program],
            Step::ActivateOrLaunch { title, program, launch, .. } => vec![title, program, &launch.target],
            Step::Open(open) => vec![&open.target, &open.arguments],
            _ => Vec::new(),
        }
    }
}

/// A step of a macro, which can be temporarily disabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroStep {
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub step: Step,
}

impl MacroStep {
    pub fn new(step: Step) -> Self {
        MacroStep { enabled: true, step }
    }
}

/// Why a step is invalid (checked before saving, F-CNF-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepProblem {
    EmptyText,
    EmptyKeys,
    NoWindowCriteria,
    EmptyTarget,
    ZeroCount,
}

/// Validates the parameters of a step.
pub fn step_problems(step: &Step) -> Vec<StepProblem> {
    let mut problems = Vec::new();
    match step {
        Step::TypeText { text, .. } | Step::CopyText { text } | Step::Notify { text } => {
            if text.is_empty() {
                problems.push(StepProblem::EmptyText);
            }
        }
        Step::PressKeys { repeat, .. } => {
            if *repeat == 0 {
                problems.push(StepProblem::ZeroCount);
            }
        }
        Step::HoldKeys { keys } => {
            if keys.is_empty() {
                problems.push(StepProblem::EmptyKeys);
            }
        }
        Step::ActivateWindow { title, program, .. } => {
            if title.trim().is_empty() && program.trim().is_empty() {
                problems.push(StepProblem::NoWindowCriteria);
            }
        }
        Step::ActivateOrLaunch { title, program, launch, .. } => {
            if title.trim().is_empty() && program.trim().is_empty() {
                problems.push(StepProblem::NoWindowCriteria);
            }
            if launch.target.trim().is_empty() {
                problems.push(StepProblem::EmptyTarget);
            }
        }
        Step::Open(open) => {
            if open.target.trim().is_empty() {
                problems.push(StepProblem::EmptyTarget);
            }
        }
        Step::Wheel { notches, .. } => {
            if *notches == 0 {
                problems.push(StepProblem::ZeroCount);
            }
        }
        Step::ReleaseKeys { .. } | Step::Wait { .. } | Step::Click { .. } | Step::MoveMouse { .. } => {}
    }
    problems
}

/// Case-insensitive wildcard matching: `*` matches any sequence (possibly
/// empty), `?` any single character; other characters match themselves. The
/// whole text must match.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Whether a window matches the criteria of an activation step: the title
/// pattern (if any) and the program (if any) must both match.
pub fn window_matches(title_pattern: &str, program: &str, window_title: &str, window_program: Option<&str>) -> bool {
    let title_ok = title_pattern.trim().is_empty() || wildcard_match(title_pattern.trim(), window_title);
    let program_ok = program.trim().is_empty()
        || window_program.is_some_and(|p| {
            crate::model::normalize_program(program).is_some_and(|wanted| wanted.eq_ignore_ascii_case(p))
        });
    title_ok && program_ok && !(title_pattern.trim().is_empty() && program.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard_match("*Bloc-notes", "Sans titre - Bloc-notes"));
        assert!(wildcard_match("sans titre*", "Sans titre - Bloc-notes"));
        assert!(wildcard_match("*titre*", "Sans titre - Bloc-notes"));
        assert!(wildcard_match("Rapport ?.docx - Word", "Rapport 3.docx - Word"));
        assert!(!wildcard_match("Bloc-notes", "Sans titre - Bloc-notes"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("", ""));
        assert!(!wildcard_match("", "x"));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("a*b*c", "aXXbYY"));
        assert!(wildcard_match("ÉTÉ*", "été 2026"));
    }

    #[test]
    fn window_criteria() {
        assert!(window_matches("*Bloc-notes", "", "Doc - Bloc-notes", Some("notepad.exe")));
        assert!(window_matches("", "notepad", "Doc - Bloc-notes", Some("notepad.exe")));
        assert!(window_matches("*Bloc-notes", "Notepad.exe", "Doc - Bloc-notes", Some("notepad.exe")));
        assert!(!window_matches("*Bloc-notes", "winword.exe", "Doc - Bloc-notes", Some("notepad.exe")));
        assert!(!window_matches("", "", "Doc", Some("notepad.exe")));
        assert!(!window_matches("", "notepad.exe", "Doc", None));
    }

    #[test]
    fn key_lists() {
        let list: KeyList = "Ctrl+RightShift+A".parse().unwrap();
        assert_eq!(
            list.0,
            vec![HeldKey::Modifier(Modifier::Ctrl, Side::Left), HeldKey::Modifier(Modifier::Shift, Side::Right), HeldKey::Key(Key::A)]
        );
        assert_eq!(list.to_string(), "Ctrl+RightShift+A");
        assert_eq!("".parse::<KeyList>().unwrap(), KeyList::default());
        assert!("Ctrl+Nope".parse::<KeyList>().is_err());
        let hk: Hotkey = "RightCtrl+Alt+V".parse().unwrap();
        assert_eq!(KeyList::from_hotkey(&hk).to_string(), "RightCtrl+Alt+V");
    }

    #[test]
    fn every_kind_has_a_default_step_of_that_kind() {
        for kind in StepKind::ALL {
            assert_eq!(kind.default_step().kind(), kind);
        }
    }

    #[test]
    fn step_validation() {
        assert_eq!(step_problems(&Step::TypeText { text: String::new(), mode: TextMode::Typing }), vec![StepProblem::EmptyText]);
        assert_eq!(step_problems(&StepKind::ActivateWindow.default_step()), vec![StepProblem::NoWindowCriteria]);
        assert_eq!(
            step_problems(&StepKind::ActivateOrLaunch.default_step()),
            vec![StepProblem::NoWindowCriteria, StepProblem::EmptyTarget]
        );
        assert!(step_problems(&StepKind::Wait.default_step()).is_empty());
        assert!(step_problems(&StepKind::Click.default_step()).is_empty());
        assert_eq!(step_problems(&Step::Wheel { direction: WheelDirection::Up, notches: 0 }), vec![StepProblem::ZeroCount]);
    }
}
