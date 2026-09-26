//! Resolution of a key press to a shortcut (F-CND-03, F-CND-04).

use crate::hotkey::ModState;
use crate::key::Key;
use crate::model::{LockState, Shortcut};
use std::collections::HashMap;

/// How specific a shortcut is; the most specific matching shortcut wins.
///
/// Order of criteria: program condition (*only in* > *everywhere except* >
/// *everywhere*, as required by F-CND-03), then the number of lock-key
/// conditions, then the number of side-specific modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Specificity {
    pub program: u8,
    pub locks: u8,
    pub sides: u8,
}

impl Specificity {
    pub fn of(shortcut: &Shortcut) -> Self {
        Specificity {
            program: shortcut.conditions.program_mode.rank(),
            locks: shortcut.conditions.lock_count(),
            sides: shortcut.keys.sided_count(),
        }
    }
}

/// Fast lookup structure built from the enabled shortcuts.
#[derive(Debug, Default, Clone)]
pub struct Engine {
    shortcuts: Vec<Shortcut>,
    by_key: HashMap<Key, Vec<usize>>,
}

impl Engine {
    /// Builds the engine from a list of shortcuts; disabled ones are ignored.
    pub fn new(shortcuts: &[Shortcut]) -> Self {
        let mut engine = Engine::default();
        for shortcut in shortcuts.iter().filter(|s| s.enabled) {
            engine.by_key.entry(shortcut.keys.key).or_default().push(engine.shortcuts.len());
            engine.shortcuts.push(shortcut.clone());
        }
        // Within a key, keep candidates sorted from most to least specific,
        // then by id, so that resolution is deterministic.
        for indices in engine.by_key.values_mut() {
            indices.sort_by(|&a, &b| {
                let (sa, sb) = (&engine.shortcuts[a], &engine.shortcuts[b]);
                Specificity::of(sb).cmp(&Specificity::of(sa)).then(sa.id.cmp(&sb.id))
            });
        }
        engine
    }

    /// Whether any enabled shortcut uses `key` as its main key.
    pub fn uses_key(&self, key: Key) -> bool {
        self.by_key.contains_key(&key)
    }

    pub fn len(&self) -> usize {
        self.shortcuts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shortcuts.is_empty()
    }

    /// Finds the shortcut to trigger for `key` pressed with `mods`, or `None`
    /// if the key press must be passed through unchanged (F-CND-04).
    ///
    /// `foreground_program` is called at most once, and only when a candidate
    /// has a program condition; it returns the lowercase executable name of
    /// the foreground program, if known.
    pub fn resolve(
        &self,
        key: Key,
        mods: ModState,
        locks: LockState,
        foreground_program: &mut dyn FnMut() -> Option<String>,
    ) -> Option<&Shortcut> {
        let candidates = self.by_key.get(&key)?;
        let mut program: Option<Option<String>> = None;
        for &index in candidates {
            let shortcut = &self.shortcuts[index];
            if !shortcut.keys.matches(key, mods) || !shortcut.conditions.accepts_locks(locks) {
                continue;
            }
            let accepted = if shortcut.conditions.program_mode == crate::model::ProgramMode::Everywhere {
                true
            } else {
                let current = program.get_or_insert_with(&mut *foreground_program);
                shortcut.conditions.accepts_program(current.as_deref())
            };
            if accepted {
                return Some(shortcut);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{Hotkey, Modifier, Side};
    use crate::model::*;

    fn shortcut(id: u32, keys: &str, mode: ProgramMode, programs: &[&str]) -> Shortcut {
        let mut s = Shortcut::new(id, keys.parse().unwrap(), Action::open(format!("target{id}")));
        s.conditions.program_mode = mode;
        s.conditions.programs = programs.iter().map(|p| p.to_string()).collect();
        s
    }

    fn mods(list: &[Modifier]) -> ModState {
        list.iter().fold(ModState::EMPTY, |s, &m| s.with(m, Side::Left, true))
    }

    fn resolve_in(engine: &Engine, hk: &str, program: Option<&str>) -> Option<u32> {
        let hk: Hotkey = hk.parse().unwrap();
        let state = Modifier::ALL
            .iter()
            .filter(|&&m| hk.req(m) != crate::hotkey::ModReq::Off)
            .fold(ModState::EMPTY, |s, &m| s.with(m, Side::Left, true));
        engine
            .resolve(hk.key, state, LockState::default(), &mut || program.map(str::to_string))
            .map(|s| s.id)
    }

    #[test]
    fn scenario_3_application_specific_shortcut_passes_through_elsewhere() {
        // Ctrl+Shift+S only acts in Excel; elsewhere the key press is passed through.
        let engine = Engine::new(&[shortcut(1, "Ctrl+Shift+S", ProgramMode::OnlyIn, &["excel.exe"])]);
        assert_eq!(resolve_in(&engine, "Ctrl+Shift+S", Some("excel.exe")), Some(1));
        assert_eq!(resolve_in(&engine, "Ctrl+Shift+S", Some("notepad.exe")), None);
        assert_eq!(resolve_in(&engine, "Ctrl+Shift+S", None), None);
        // Other combinations are never intercepted.
        assert_eq!(resolve_in(&engine, "Ctrl+S", Some("excel.exe")), None);
    }

    #[test]
    fn most_specific_wins() {
        // F-CND-03: "only in" > "everywhere except" > "everywhere".
        let engine = Engine::new(&[
            shortcut(1, "Ctrl+Shift+S", ProgramMode::Everywhere, &[]),
            shortcut(2, "Ctrl+Shift+S", ProgramMode::Except, &["notepad.exe"]),
            shortcut(3, "Ctrl+Shift+S", ProgramMode::OnlyIn, &["winword.exe"]),
        ]);
        assert_eq!(resolve_in(&engine, "Ctrl+Shift+S", Some("winword.exe")), Some(3));
        assert_eq!(resolve_in(&engine, "Ctrl+Shift+S", Some("chrome.exe")), Some(2));
        assert_eq!(resolve_in(&engine, "Ctrl+Shift+S", Some("notepad.exe")), Some(1));
    }

    #[test]
    fn scenario_5_numpad_is_distinct_from_top_row() {
        let engine = Engine::new(&[shortcut(1, "Ctrl+Num1", ProgramMode::Everywhere, &[])]);
        assert_eq!(resolve_in(&engine, "Ctrl+Num1", None), Some(1));
        assert_eq!(resolve_in(&engine, "Ctrl+1", None), None);
        assert!(engine.uses_key(Key::Numpad1));
        assert!(!engine.uses_key(Key::D1));
    }

    #[test]
    fn disabled_shortcuts_are_ignored() {
        let mut s = shortcut(1, "Win+N", ProgramMode::Everywhere, &[]);
        s.enabled = false;
        let engine = Engine::new(&[s]);
        assert!(engine.is_empty());
        assert_eq!(resolve_in(&engine, "Win+N", None), None);
    }

    #[test]
    fn foreground_program_is_only_queried_when_needed() {
        let engine = Engine::new(&[
            shortcut(1, "Ctrl+Alt+M", ProgramMode::Everywhere, &[]),
            shortcut(2, "Ctrl+Alt+P", ProgramMode::OnlyIn, &["a.exe"]),
            shortcut(3, "Ctrl+Alt+P", ProgramMode::Except, &["a.exe"]),
        ]);
        let mut calls = 0;
        let m = mods(&[Modifier::Ctrl, Modifier::Alt]);
        let hit = engine.resolve(Key::M, m, LockState::default(), &mut || {
            calls += 1;
            None
        });
        assert_eq!(hit.map(|s| s.id), Some(1));
        assert_eq!(calls, 0);
        let hit = engine.resolve(Key::P, m, LockState::default(), &mut || {
            calls += 1;
            Some("b.exe".into())
        });
        assert_eq!(hit.map(|s| s.id), Some(3));
        assert_eq!(calls, 1, "queried once even with several candidates");
    }

    #[test]
    fn lock_conditions_and_their_specificity() {
        let mut caps_on = shortcut(1, "Ctrl+J", ProgramMode::Everywhere, &[]);
        caps_on.conditions.caps_lock = LockReq::On;
        let plain = shortcut(2, "Ctrl+J", ProgramMode::Everywhere, &[]);
        let engine = Engine::new(&[plain, caps_on]);
        let m = mods(&[Modifier::Ctrl]);
        let on = LockState { caps: true, ..LockState::default() };
        assert_eq!(engine.resolve(Key::J, m, on, &mut || None).map(|s| s.id), Some(1));
        assert_eq!(engine.resolve(Key::J, m, LockState::default(), &mut || None).map(|s| s.id), Some(2));
    }

    #[test]
    fn side_specific_combination_is_preferred() {
        let engine = Engine::new(&[
            shortcut(1, "Ctrl+P", ProgramMode::Everywhere, &[]),
            shortcut(2, "RightCtrl+P", ProgramMode::Everywhere, &[]),
        ]);
        let right = ModState::EMPTY.with(Modifier::Ctrl, Side::Right, true);
        let left = ModState::EMPTY.with(Modifier::Ctrl, Side::Left, true);
        assert_eq!(engine.resolve(Key::P, right, LockState::default(), &mut || None).map(|s| s.id), Some(2));
        assert_eq!(engine.resolve(Key::P, left, LockState::default(), &mut || None).map(|s| s.id), Some(1));
    }

    #[test]
    fn ties_are_resolved_by_id() {
        let engine = Engine::new(&[
            shortcut(9, "Ctrl+Alt+M", ProgramMode::Everywhere, &[]),
            shortcut(4, "Ctrl+Alt+M", ProgramMode::Everywhere, &[]),
        ]);
        assert_eq!(resolve_in(&engine, "Ctrl+Alt+M", None), Some(4));
    }
}
