//! Data model: shortcuts, actions, conditions and settings.

use crate::hotkey::Hotkey;
use crate::key::Key;
use crate::steps::{MacroStep, Step};
use serde::{Deserialize, Serialize};

/// Identifier of a shortcut, unique within a configuration.
pub type ShortcutId = u32;

fn yes() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A shortcut: a combination bound to an action, with activation conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shortcut {
    pub id: ShortcutId,
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Group the shortcut belongs to (empty: none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub favorite: bool,
    pub keys: Hotkey,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Conditions::is_default")]
    pub conditions: Conditions,
}

impl Shortcut {
    pub fn new(id: ShortcutId, keys: Hotkey, action: Action) -> Self {
        Shortcut {
            id,
            name: String::new(),
            enabled: true,
            group: String::new(),
            favorite: false,
            keys,
            action,
            conditions: Conditions::default(),
        }
    }
}

/// What happens when a shortcut is triggered.
///
/// The UI presents three kinds (Open, Text, Macro); internally a *Text* is a
/// macro made of a single "type text" step, so that it can later grow into a
/// full macro without changing the stored model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Open(OpenAction),
    Macro(Macro),
}

/// Kind of action as presented to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionKind {
    Open,
    Text,
    Macro,
}

impl Action {
    /// A *Text* action: a one-step macro typing `text`.
    pub fn text(text: impl Into<String>, mode: TextMode) -> Self {
        Action::Macro(Macro { steps: vec![MacroStep::new(Step::TypeText { text: text.into(), mode })] })
    }

    /// A macro made of the given steps (all enabled).
    pub fn steps(steps: impl IntoIterator<Item = Step>) -> Self {
        Action::Macro(Macro { steps: steps.into_iter().map(MacroStep::new).collect() })
    }

    /// An *Open* action with default options.
    pub fn open(target: impl Into<String>) -> Self {
        Action::Open(OpenAction { target: target.into(), ..OpenAction::default() })
    }

    pub fn kind(&self) -> ActionKind {
        match self {
            Action::Open(_) => ActionKind::Open,
            Action::Macro(m) if m.as_text().is_some() => ActionKind::Text,
            Action::Macro(_) => ActionKind::Macro,
        }
    }

    /// The text and mode when this action is a simple *Text*.
    pub fn as_text(&self) -> Option<(&str, TextMode)> {
        match self {
            Action::Macro(m) => m.as_text(),
            Action::Open(_) => None,
        }
    }
}

/// Window state requested when opening a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    #[default]
    Normal,
    Minimized,
    Maximized,
}

impl WindowState {
    pub const ALL: [WindowState; 3] = [WindowState::Normal, WindowState::Minimized, WindowState::Maximized];

    fn is_normal(&self) -> bool {
        *self == WindowState::Normal
    }
}

/// Parameters of the *Open* action (program, document, folder, URL, protocol…).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OpenAction {
    pub target: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub working_dir: String,
    #[serde(default, skip_serializing_if = "WindowState::is_normal")]
    pub window: WindowState,
    #[serde(default, skip_serializing_if = "is_false")]
    pub run_as_admin: bool,
}

/// How a text is sent to the active window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextMode {
    /// Simulated typing, character by character (default).
    #[default]
    Typing,
    /// Through the clipboard, which is restored afterwards.
    Paste,
}

impl TextMode {
    pub const ALL: [TextMode; 2] = [TextMode::Typing, TextMode::Paste];

    pub(crate) fn is_typing(&self) -> bool {
        *self == TextMode::Typing
    }
}

/// An ordered list of steps (F-MAC-01).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Macro {
    pub steps: Vec<MacroStep>,
}

impl Macro {
    /// The text and mode when the macro is a single enabled "type text" step,
    /// i.e. what the user sees as a *Text* action.
    pub fn as_text(&self) -> Option<(&str, TextMode)> {
        match self.steps.as_slice() {
            [MacroStep { enabled: true, step: Step::TypeText { text, mode } }] => Some((text.as_str(), *mode)),
            _ => None,
        }
    }
}

/// Where a shortcut is active, depending on the foreground program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramMode {
    #[default]
    Everywhere,
    /// Only when one of the listed programs is in the foreground.
    OnlyIn,
    /// Everywhere except when one of the listed programs is in the foreground.
    Except,
}

impl ProgramMode {
    pub const ALL: [ProgramMode; 3] = [ProgramMode::Everywhere, ProgramMode::OnlyIn, ProgramMode::Except];

    /// Priority used to pick the most specific shortcut (higher wins).
    pub fn rank(self) -> u8 {
        match self {
            ProgramMode::OnlyIn => 2,
            ProgramMode::Except => 1,
            ProgramMode::Everywhere => 0,
        }
    }
}

/// Requirement on a lock key (Caps Lock, Num Lock, Scroll Lock).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockReq {
    #[default]
    Any,
    On,
    Off,
}

impl LockReq {
    pub const ALL: [LockReq; 3] = [LockReq::Any, LockReq::On, LockReq::Off];

    pub fn accepts(self, on: bool) -> bool {
        match self {
            LockReq::Any => true,
            LockReq::On => on,
            LockReq::Off => !on,
        }
    }

    pub fn compatible(self, other: LockReq) -> bool {
        [true, false].iter().any(|&on| self.accepts(on) && other.accepts(on))
    }

    fn is_any(&self) -> bool {
        *self == LockReq::Any
    }
}

/// Activation conditions of a shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Conditions {
    #[serde(default)]
    pub program_mode: ProgramMode,
    /// Executable names, e.g. `chrome.exe` (compared case-insensitively).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<String>,
    #[serde(default, skip_serializing_if = "LockReq::is_any")]
    pub caps_lock: LockReq,
    #[serde(default, skip_serializing_if = "LockReq::is_any")]
    pub num_lock: LockReq,
    #[serde(default, skip_serializing_if = "LockReq::is_any")]
    pub scroll_lock: LockReq,
}

impl Conditions {
    pub fn is_default(&self) -> bool {
        *self == Conditions::default()
    }

    /// Number of lock keys with a requirement (used for specificity).
    pub fn lock_count(&self) -> u8 {
        [self.caps_lock, self.num_lock, self.scroll_lock].iter().filter(|l| !l.is_any()).count() as u8
    }

    /// Whether the program condition accepts the given foreground program
    /// (a lowercase executable name, or `None` when unknown).
    pub fn accepts_program(&self, program: Option<&str>) -> bool {
        let listed = |p: &str| self.programs.iter().any(|q| q.eq_ignore_ascii_case(p));
        match self.program_mode {
            ProgramMode::Everywhere => true,
            ProgramMode::OnlyIn => program.is_some_and(listed),
            ProgramMode::Except => !program.is_some_and(listed),
        }
    }

    /// Whether the lock-key conditions accept the given lock states.
    pub fn accepts_locks(&self, locks: LockState) -> bool {
        self.caps_lock.accepts(locks.caps) && self.num_lock.accepts(locks.num) && self.scroll_lock.accepts(locks.scroll)
    }
}

/// Current state of the lock keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LockState {
    pub caps: bool,
    pub num: bool,
    pub scroll: bool,
}

/// Normalises a program entered by the user: keeps the file name, lowercases
/// it and appends `.exe` when no extension is given. Returns `None` if empty.
pub fn normalize_program(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_matches('"').trim();
    let name = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).trim();
    if name.is_empty() {
        return None;
    }
    let mut name = name.to_lowercase();
    if !name.contains('.') {
        name.push_str(".exe");
    }
    Some(name)
}

/// Theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub const ALL: [ThemePref; 3] = [ThemePref::System, ThemePref::Light, ThemePref::Dark];
}

fn auto() -> String {
    "auto".to_string()
}

fn default_stop_key() -> Hotkey {
    Hotkey::new(Key::Escape)
}

fn is_default_stop_key(key: &Hotkey) -> bool {
    *key == default_stop_key()
}

/// Application settings stored in the configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// `auto` or a language code such as `fr` or `en`.
    #[serde(default = "auto")]
    pub language: String,
    #[serde(default)]
    pub theme: ThemePref,
    /// Delay between simulated keystrokes, for slow applications (0 = as fast as possible).
    #[serde(default)]
    pub typing_delay_ms: u32,
    /// Mode proposed for new texts.
    #[serde(default, skip_serializing_if = "TextMode::is_typing")]
    pub default_text_mode: TextMode,
    /// Key that interrupts a running macro (F-MAC-03).
    #[serde(default = "default_stop_key", skip_serializing_if = "is_default_stop_key")]
    pub stop_key: Hotkey,
    /// Global combination opening the main window (§ 8.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_window_keys: Option<Hotkey>,
    /// Combination showing the cheat sheet (§ 8.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cheat_sheet_keys: Option<Hotkey>,
    /// Whether informational and error notifications are shown (the
    /// "notification" macro step is always shown).
    #[serde(default = "yes")]
    pub notifications: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: auto(),
            theme: ThemePref::System,
            typing_delay_ms: 0,
            default_text_mode: TextMode::Typing,
            stop_key: default_stop_key(),
            open_window_keys: None,
            cheat_sheet_keys: None,
            notifications: true,
        }
    }
}

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Config {
    /// Format version of the file, for future migrations.
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    /// User groups, in display order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(default, rename = "shortcut", skip_serializing_if = "Vec::is_empty")]
    pub shortcuts: Vec<Shortcut>,
}

impl Config {
    pub fn new() -> Self {
        Config {
            version: crate::config::FORMAT_VERSION,
            settings: Settings::default(),
            groups: Vec::new(),
            shortcuts: Vec::new(),
        }
    }

    /// Adds a group if it does not exist yet (names are compared
    /// case-insensitively); returns the stored name.
    pub fn ensure_group(&mut self, name: &str) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        if let Some(existing) = self.groups.iter().find(|g| g.to_lowercase() == name.to_lowercase()) {
            return Some(existing.clone());
        }
        self.groups.push(name.to_string());
        Some(name.to_string())
    }

    /// Renames a group and moves its shortcuts. Returns false if the new name
    /// is empty or already used by another group.
    pub fn rename_group(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty() || self.groups.iter().any(|g| g != old && g.to_lowercase() == new.to_lowercase()) {
            return false;
        }
        for g in &mut self.groups {
            if g == old {
                *g = new.to_string();
            }
        }
        for s in &mut self.shortcuts {
            if s.group == old {
                s.group = new.to_string();
            }
        }
        true
    }

    /// Deletes a group; its shortcuts are kept, without group.
    pub fn delete_group(&mut self, name: &str) {
        self.groups.retain(|g| g != name);
        for s in &mut self.shortcuts {
            if s.group == name {
                s.group.clear();
            }
        }
    }

    pub fn next_id(&self) -> ShortcutId {
        self.shortcuts.iter().map(|s| s.id).max().unwrap_or(0) + 1
    }

    pub fn get(&self, id: ShortcutId) -> Option<&Shortcut> {
        self.shortcuts.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: ShortcutId) -> Option<&mut Shortcut> {
        self.shortcuts.iter_mut().find(|s| s.id == id)
    }

    /// Inserts or replaces (by id) a shortcut.
    pub fn upsert(&mut self, shortcut: Shortcut) {
        match self.get_mut(shortcut.id) {
            Some(existing) => *existing = shortcut,
            None => self.shortcuts.push(shortcut),
        }
    }

    pub fn remove(&mut self, id: ShortcutId) -> Option<Shortcut> {
        let index = self.shortcuts.iter().position(|s| s.id == id)?;
        Some(self.shortcuts.remove(index))
    }

    /// Repairs a configuration loaded from disk: unique non-zero ids and
    /// normalised program names.
    pub fn sanitize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.next_id().max(1);
        for shortcut in &mut self.shortcuts {
            if shortcut.id == 0 || !seen.insert(shortcut.id) {
                shortcut.id = next;
                seen.insert(next);
                next += 1;
            }
            let programs = std::mem::take(&mut shortcut.conditions.programs);
            for program in programs.iter().filter_map(|p| normalize_program(p)) {
                if !shortcut.conditions.programs.contains(&program) {
                    shortcut.conditions.programs.push(program);
                }
            }
            shortcut.group = shortcut.group.trim().to_string();
        }
        // Unique, non-empty groups; groups used by shortcuts are listed.
        let groups = std::mem::take(&mut self.groups);
        for group in groups {
            self.ensure_group(&group);
        }
        let used: Vec<String> = self.shortcuts.iter().map(|s| s.group.clone()).collect();
        for group in used {
            if let Some(stored) = self.ensure_group(&group) {
                for s in self.shortcuts.iter_mut().filter(|s| s.group.to_lowercase() == stored.to_lowercase()) {
                    s.group = stored.clone();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_a_one_step_macro() {
        let action = Action::text("hello", TextMode::Typing);
        assert_eq!(action.kind(), ActionKind::Text);
        assert_eq!(action.as_text(), Some(("hello", TextMode::Typing)));
        let two_steps = Action::Macro(Macro {
            steps: vec![
                MacroStep::new(Step::TypeText { text: "a".into(), mode: TextMode::Typing }),
                MacroStep::new(Step::TypeText { text: "b".into(), mode: TextMode::Typing }),
            ],
        });
        assert_eq!(two_steps.kind(), ActionKind::Macro);
        assert_eq!(Action::open("notepad.exe").kind(), ActionKind::Open);
    }

    #[test]
    fn program_conditions() {
        let mut c = Conditions::default();
        assert!(c.accepts_program(None));
        c.program_mode = ProgramMode::OnlyIn;
        c.programs = vec!["excel.exe".into()];
        assert!(c.accepts_program(Some("excel.exe")));
        assert!(c.accepts_program(Some("EXCEL.EXE")));
        assert!(!c.accepts_program(Some("notepad.exe")));
        assert!(!c.accepts_program(None));
        c.program_mode = ProgramMode::Except;
        assert!(!c.accepts_program(Some("excel.exe")));
        assert!(c.accepts_program(Some("notepad.exe")));
        assert!(c.accepts_program(None));
    }

    #[test]
    fn lock_conditions() {
        let c = Conditions { caps_lock: LockReq::On, num_lock: LockReq::Off, ..Conditions::default() };
        assert!(c.accepts_locks(LockState { caps: true, num: false, scroll: true }));
        assert!(!c.accepts_locks(LockState { caps: false, num: false, scroll: true }));
        assert!(!c.accepts_locks(LockState { caps: true, num: true, scroll: false }));
        assert_eq!(c.lock_count(), 2);
        assert!(LockReq::Any.compatible(LockReq::On));
        assert!(!LockReq::On.compatible(LockReq::Off));
    }

    #[test]
    fn program_normalisation() {
        assert_eq!(normalize_program(" Chrome "), Some("chrome.exe".into()));
        assert_eq!(normalize_program(r"C:\Program Files\App\EXCEL.EXE"), Some("excel.exe".into()));
        assert_eq!(normalize_program("\"notepad.exe\""), Some("notepad.exe".into()));
        assert_eq!(normalize_program("   "), None);
    }

    #[test]
    fn sanitize_fixes_duplicate_ids_and_programs() {
        let hk = "Ctrl+A".parse().unwrap();
        let mut config = Config::new();
        let mut a = Shortcut::new(3, hk, Action::open("a"));
        a.conditions.programs = vec!["Word".into(), "word.exe".into()];
        config.shortcuts = vec![a, Shortcut::new(3, hk, Action::open("b")), Shortcut::new(0, hk, Action::open("c"))];
        config.sanitize();
        let ids: Vec<_> = config.shortcuts.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![3, 4, 5]);
        assert_eq!(config.shortcuts[0].conditions.programs, vec!["word.exe".to_string()]);
    }

    #[test]
    fn upsert_and_remove() {
        let hk = "Ctrl+A".parse().unwrap();
        let mut config = Config::new();
        config.upsert(Shortcut::new(1, hk, Action::open("a")));
        config.upsert(Shortcut::new(1, hk, Action::open("b")));
        assert_eq!(config.shortcuts.len(), 1);
        assert_eq!(config.next_id(), 2);
        assert!(config.remove(1).is_some());
        assert!(config.remove(1).is_none());
    }
}
