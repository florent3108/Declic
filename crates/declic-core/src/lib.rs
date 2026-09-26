//! Platform-independent core of Declic.
//!
//! This crate contains everything that can be reasoned about without an
//! operating system: the data model, the configuration file format, the
//! resolution of a key press to a shortcut, conflict detection, variable
//! expansion, translations and search. It is fully unit-tested.

pub mod config;
pub mod conflict;
pub mod engine;
pub mod hotkey;
pub mod i18n;
pub mod key;
pub mod model;
pub mod naming;
pub mod reorder;
pub mod search;
pub mod stats;
pub mod steps;
pub mod transfer;
pub mod typing;
pub mod vars;

pub use engine::Engine;
pub use hotkey::{Hotkey, ModReq, ModState, Modifier, Side};
pub use key::{Key, KeyCategory};
pub use model::*;
pub use steps::*;

#[cfg(test)]
mod scenarios {
    //! Acceptance scenarios of the specification (Annexe B) that can be
    //! checked without an operating system.

    use crate::conflict::find_conflicts;
    use crate::engine::Engine;
    use crate::hotkey::{ModState, Modifier, Side};
    use crate::key::Key;
    use crate::model::*;
    use crate::typing::{TypeUnit, plan};
    use crate::vars::{DateNames, LocalDateTime, VarProvider, expand};

    fn press(engine: &Engine, key: Key, mods: &[Modifier], program: &str) -> Option<ShortcutId> {
        let state = mods.iter().fold(ModState::EMPTY, |s, &m| s.with(m, Side::Left, true));
        engine.resolve(key, state, LockState::default(), &mut || Some(program.to_string())).map(|s| s.id)
    }

    fn config() -> Config {
        let mut config = Config::new();
        // 1. E-mail address.
        let mut email = Shortcut::new(1, "Ctrl+Alt+M".parse().unwrap(), Action::text("prenom.nom@exemple.fr", TextMode::Typing));
        email.name = "Adresse e-mail".into();
        // 2. Launch Notepad (the "activate if already open" part is a V1 macro step).
        let notepad = Shortcut::new(2, "Win+N".parse().unwrap(), Action::open("notepad.exe"));
        // 3. Excel-only shortcut.
        let mut excel = Shortcut::new(3, "Ctrl+Shift+S".parse().unwrap(), Action::text("x", TextMode::Typing));
        excel.conditions.program_mode = ProgramMode::OnlyIn;
        excel.conditions.programs = vec!["excel.exe".into()];
        // 5. Numeric keypad.
        let projects = Shortcut::new(5, "Ctrl+Num1".parse().unwrap(), Action::open(r"%USERPROFILE%\Projets"));
        // 6. Dated signature.
        let signature = Shortcut::new(6, "Ctrl+Alt+S".parse().unwrap(), Action::text("Marie Dupont\nService achats\n%DATE:dd/MM/yyyy%", TextMode::Typing));
        config.shortcuts = vec![email, notepad, excel, projects, signature];
        config
    }

    struct Env;
    impl VarProvider for Env {
        fn env_var(&self, name: &str) -> Option<String> {
            name.eq_ignore_ascii_case("USERPROFILE").then(|| r"C:\Users\Marie".to_string())
        }
        fn clipboard_text(&self) -> Option<String> {
            None
        }
        fn now(&self) -> LocalDateTime {
            LocalDateTime { year: 2026, month: 9, day: 25, hour: 9, minute: 0, second: 0, weekday: 4 }
        }
    }

    #[test]
    fn scenario_1_email_in_any_application() {
        let engine = Engine::new(&config().shortcuts);
        for app in ["winword.exe", "chrome.exe", "notepad.exe"] {
            assert_eq!(press(&engine, Key::M, &[Modifier::Ctrl, Modifier::Alt], app), Some(1));
        }
        let action = &config().shortcuts[0].action;
        let (text, _) = action.as_text().unwrap();
        let typed: String = plan(text)
            .into_iter()
            .map(|u| match u {
                TypeUnit::Unit(c) => char::from_u32(c as u32).unwrap(),
                _ => panic!("no parasitic key"),
            })
            .collect();
        assert_eq!(typed, "prenom.nom@exemple.fr");
    }

    #[test]
    fn scenario_2_win_n_launches_notepad() {
        let engine = Engine::new(&config().shortcuts);
        assert_eq!(press(&engine, Key::N, &[Modifier::Win], "explorer.exe"), Some(2));
    }

    #[test]
    fn scenario_3_excel_only() {
        let engine = Engine::new(&config().shortcuts);
        assert_eq!(press(&engine, Key::S, &[Modifier::Ctrl, Modifier::Shift], "excel.exe"), Some(3));
        assert_eq!(press(&engine, Key::S, &[Modifier::Ctrl, Modifier::Shift], "notepad.exe"), None);
    }

    #[test]
    fn scenario_5_numpad_one() {
        let engine = Engine::new(&config().shortcuts);
        assert_eq!(press(&engine, Key::Numpad1, &[Modifier::Ctrl], "explorer.exe"), Some(5));
        assert_eq!(press(&engine, Key::D1, &[Modifier::Ctrl], "explorer.exe"), None);
        let Action::Open(open) = &config().shortcuts[3].action else { panic!() };
        assert_eq!(expand(&open.target, &Env, &DateNames::english()), r"C:\Users\Marie\Projets");
    }

    #[test]
    fn scenario_6_dated_signature_has_three_lines() {
        let config = config();
        let (text, _) = config.shortcuts[4].action.as_text().unwrap();
        let expanded = expand(text, &Env, &DateNames::english());
        assert_eq!(expanded.lines().collect::<Vec<_>>(), vec!["Marie Dupont", "Service achats", "25/09/2026"]);
        assert_eq!(plan(&expanded).iter().filter(|u| **u == TypeUnit::Enter).count(), 2);
    }

    #[test]
    fn scenario_8_second_ctrl_alt_m_is_flagged() {
        let config = config();
        let draft = Shortcut::new(config.next_id(), "Ctrl+Alt+M".parse().unwrap(), Action::text("autre", TextMode::Typing));
        assert_eq!(find_conflicts(&draft, &config.shortcuts), vec![1]);
    }
}
