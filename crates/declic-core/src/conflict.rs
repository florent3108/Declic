//! Conflict detection, risky combinations and validation (F-CNF-01..04).

use crate::engine::Specificity;
use crate::hotkey::{Hotkey, Modifier};
use crate::key::{Key, KeyCategory};
use crate::model::{Action, Conditions, ProgramMode, Shortcut, ShortcutId};

/// Whether there is a context (foreground program and lock states) in which
/// both sets of conditions are satisfied.
pub fn conditions_overlap(a: &Conditions, b: &Conditions) -> bool {
    let locks = a.caps_lock.compatible(b.caps_lock)
        && a.num_lock.compatible(b.num_lock)
        && a.scroll_lock.compatible(b.scroll_lock);
    locks && programs_overlap(a, b)
}

fn contains(list: &[String], program: &str) -> bool {
    list.iter().any(|p| p.eq_ignore_ascii_case(program))
}

fn programs_overlap(a: &Conditions, b: &Conditions) -> bool {
    use ProgramMode::*;
    match (a.program_mode, b.program_mode) {
        (Everywhere, Everywhere) | (Everywhere, Except) | (Except, Everywhere) | (Except, Except) => true,
        (Everywhere, OnlyIn) => !b.programs.is_empty(),
        (OnlyIn, Everywhere) => !a.programs.is_empty(),
        (OnlyIn, OnlyIn) => a.programs.iter().any(|p| contains(&b.programs, p)),
        (OnlyIn, Except) => a.programs.iter().any(|p| !contains(&b.programs, p)),
        (Except, OnlyIn) => b.programs.iter().any(|p| !contains(&a.programs, p)),
    }
}

/// Whether `a` and `b` are ambiguous: the same key press can trigger both in
/// some context, and the specificity rule cannot choose between them.
pub fn is_conflict(a: &Shortcut, b: &Shortcut) -> bool {
    a.id != b.id
        && a.keys.overlaps(&b.keys)
        && conditions_overlap(&a.conditions, &b.conditions)
        && Specificity::of(a) == Specificity::of(b)
}

/// Returns the ids of the shortcuts that conflict with `draft` (F-CNF-01).
/// Disabled shortcuts are included, since enabling them later would create a
/// silent conflict.
pub fn find_conflicts(draft: &Shortcut, others: &[Shortcut]) -> Vec<ShortcutId> {
    others.iter().filter(|o| is_conflict(draft, o)).map(|o| o.id).collect()
}

/// Ids of all shortcuts involved in at least one conflict (for list badges).
pub fn conflicting_ids(shortcuts: &[Shortcut]) -> Vec<ShortcutId> {
    shortcuts
        .iter()
        .filter(|a| shortcuts.iter().any(|b| is_conflict(a, b)))
        .map(|a| a.id)
        .collect()
}

/// Why a combination cannot be used at all (F-COMB-06, F-CNF-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unavailable {
    /// Ctrl+Alt+Del is handled by Windows itself and never reaches applications.
    SecureAttention,
    /// Win+L locks the session before any application can see it.
    LockWorkstation,
}

/// Checks whether a combination is reserved by the operating system.
pub fn unavailable(keys: &Hotkey) -> Option<Unavailable> {
    let ctrl_alt = keys.is_exactly(&[Modifier::Ctrl, Modifier::Alt])
        || keys.is_exactly(&[Modifier::Ctrl, Modifier::Alt, Modifier::Shift]);
    if ctrl_alt && matches!(keys.key, Key::Delete | Key::NumpadDecimal) {
        return Some(Unavailable::SecureAttention);
    }
    if keys.key == Key::L && keys.win != crate::hotkey::ModReq::Off {
        return Some(Unavailable::LockWorkstation);
    }
    None
}

/// Non-blocking warnings about risky combinations (F-CNF-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Warning {
    /// A key that normally types or edits text, used without modifier.
    NoModifier,
    /// Only Shift with a character key: it would replace typing capitals/symbols.
    ShiftOnly,
    /// A very common application shortcut (Ctrl+C, Alt+F4…) active everywhere.
    CommonShortcut,
    /// Ctrl+Alt with a key that produces characters with AltGr on many layouts.
    AltGrCharacter,
}

/// Returns the warnings that apply to a shortcut.
pub fn warnings(shortcut: &Shortcut) -> Vec<Warning> {
    let keys = &shortcut.keys;
    let everywhere = shortcut.conditions.program_mode != ProgramMode::OnlyIn;
    let mut out = Vec::new();
    let typing_key = matches!(
        keys.key.category(),
        KeyCategory::Letter
            | KeyCategory::Digit
            | KeyCategory::Punctuation
            | KeyCategory::Editing
            | KeyCategory::Navigation
            | KeyCategory::Numpad
    );
    if !keys.has_modifiers() && typing_key && everywhere {
        out.push(Warning::NoModifier);
    }
    if keys.is_exactly(&[Modifier::Shift]) && (keys.key.is_character() || keys.key.category() == KeyCategory::Numpad) && everywhere {
        out.push(Warning::ShiftOnly);
    }
    let common = (keys.is_exactly(&[Modifier::Ctrl])
        && matches!(
            keys.key,
            Key::A | Key::C | Key::F | Key::N | Key::O | Key::P | Key::S | Key::T | Key::V | Key::W | Key::X | Key::Y | Key::Z | Key::Tab
        ))
        || (keys.is_exactly(&[Modifier::Alt]) && matches!(keys.key, Key::F4 | Key::Tab))
        || (keys.is_exactly(&[Modifier::Ctrl, Modifier::Shift]) && keys.key == Key::Escape);
    if common && everywhere {
        out.push(Warning::CommonShortcut);
    }
    if keys.is_exactly(&[Modifier::Ctrl, Modifier::Alt])
        && (matches!(keys.key.category(), KeyCategory::Digit | KeyCategory::Punctuation) || keys.key == Key::E)
    {
        out.push(Warning::AltGrCharacter);
    }
    out
}

/// A field that failed validation before saving (F-CNF-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationError {
    /// The target of an *Open* action is empty.
    MissingTarget,
    /// The text of a *Text* action is empty.
    MissingText,
    /// A program condition other than "everywhere" has no program.
    MissingPrograms,
    /// The macro has no step.
    EmptyMacro,
    /// A step of the macro has an invalid parameter.
    Step { index: usize, problem: crate::steps::StepProblem },
    /// The combination is reserved by the system.
    Unavailable(Unavailable),
}

/// Validates the fields of a shortcut. The combination itself is always
/// present in a [`Shortcut`]; editors check for a missing one beforehand.
pub fn validate(shortcut: &Shortcut) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Some(reason) = unavailable(&shortcut.keys) {
        errors.push(ValidationError::Unavailable(reason));
    }
    match &shortcut.action {
        Action::Open(open) if open.target.trim().is_empty() => errors.push(ValidationError::MissingTarget),
        Action::Open(_) => {}
        Action::Macro(m) => match m.as_text() {
            Some(("", _)) => errors.push(ValidationError::MissingText),
            Some(_) => {}
            None if m.steps.iter().all(|s| !s.enabled) => errors.push(ValidationError::EmptyMacro),
            None => {
                for (index, s) in m.steps.iter().enumerate().filter(|(_, s)| s.enabled) {
                    for problem in crate::steps::step_problems(&s.step) {
                        errors.push(ValidationError::Step { index, problem });
                    }
                }
            }
        },
    }
    if shortcut.conditions.program_mode != ProgramMode::Everywhere && shortcut.conditions.programs.is_empty() {
        errors.push(ValidationError::MissingPrograms);
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn text_shortcut(id: u32, keys: &str) -> Shortcut {
        Shortcut::new(id, keys.parse().unwrap(), Action::text("x", TextMode::Typing))
    }

    fn with_programs(mut s: Shortcut, mode: ProgramMode, programs: &[&str]) -> Shortcut {
        s.conditions.program_mode = mode;
        s.conditions.programs = programs.iter().map(|p| p.to_string()).collect();
        s
    }

    #[test]
    fn scenario_8_duplicate_combination_is_reported() {
        let existing = vec![text_shortcut(1, "Ctrl+Alt+M"), text_shortcut(2, "Ctrl+Alt+N")];
        let draft = text_shortcut(3, "Ctrl+Alt+M");
        assert_eq!(find_conflicts(&draft, &existing), vec![1]);
        assert_eq!(conflicting_ids(&[existing[0].clone(), existing[1].clone(), draft]), vec![1, 3]);
    }

    #[test]
    fn a_shortcut_does_not_conflict_with_itself() {
        let s = text_shortcut(1, "Ctrl+Alt+M");
        assert!(find_conflicts(&s, std::slice::from_ref(&s)).is_empty());
    }

    #[test]
    fn disjoint_or_resolvable_conditions_are_not_conflicts() {
        // F-CND-03 example: A in Word, B elsewhere.
        let word = with_programs(text_shortcut(1, "Ctrl+Shift+S"), ProgramMode::OnlyIn, &["winword.exe"]);
        let elsewhere = text_shortcut(2, "Ctrl+Shift+S");
        assert!(!is_conflict(&word, &elsewhere));
        let not_word = with_programs(text_shortcut(3, "Ctrl+Shift+S"), ProgramMode::Except, &["winword.exe"]);
        assert!(!is_conflict(&word, &not_word));
        let excel = with_programs(text_shortcut(4, "Ctrl+Shift+S"), ProgramMode::OnlyIn, &["excel.exe"]);
        assert!(!is_conflict(&word, &excel));
    }

    #[test]
    fn overlapping_conditions_of_same_specificity_conflict() {
        let a = with_programs(text_shortcut(1, "Ctrl+Shift+S"), ProgramMode::OnlyIn, &["winword.exe", "excel.exe"]);
        let b = with_programs(text_shortcut(2, "Ctrl+Shift+S"), ProgramMode::OnlyIn, &["EXCEL.EXE"]);
        assert!(is_conflict(&a, &b));
        let c = with_programs(text_shortcut(3, "Ctrl+Shift+S"), ProgramMode::Except, &["a.exe"]);
        let d = with_programs(text_shortcut(4, "Ctrl+Shift+S"), ProgramMode::Except, &["b.exe"]);
        assert!(is_conflict(&c, &d));
    }

    #[test]
    fn lock_conditions_can_separate_shortcuts() {
        let mut on = text_shortcut(1, "Ctrl+J");
        on.conditions.caps_lock = LockReq::On;
        let mut off = text_shortcut(2, "Ctrl+J");
        off.conditions.caps_lock = LockReq::Off;
        assert!(!is_conflict(&on, &off));
        let mut on2 = text_shortcut(3, "Ctrl+J");
        on2.conditions.caps_lock = LockReq::On;
        assert!(is_conflict(&on, &on2));
    }

    #[test]
    fn sided_modifiers_can_separate_shortcuts() {
        assert!(!is_conflict(&text_shortcut(1, "LeftCtrl+P"), &text_shortcut(2, "RightCtrl+P")));
        assert!(is_conflict(&text_shortcut(1, "RightCtrl+P"), &text_shortcut(2, "RightCtrl+P")));
    }

    #[test]
    fn condition_overlap_matrix() {
        let base = text_shortcut(1, "Ctrl+A");
        let only_a = with_programs(base.clone(), ProgramMode::OnlyIn, &["a.exe"]).conditions;
        let except_a = with_programs(base.clone(), ProgramMode::Except, &["a.exe"]).conditions;
        let except_ab = with_programs(base.clone(), ProgramMode::Except, &["a.exe", "b.exe"]).conditions;
        let only_ab = with_programs(base.clone(), ProgramMode::OnlyIn, &["a.exe", "b.exe"]).conditions;
        let everywhere = base.conditions.clone();
        assert!(!conditions_overlap(&only_a, &except_a));
        assert!(conditions_overlap(&only_ab, &except_a));
        assert!(!conditions_overlap(&only_ab, &except_ab));
        assert!(conditions_overlap(&everywhere, &only_a));
        assert!(conditions_overlap(&everywhere, &except_a));
    }

    #[test]
    fn risky_combinations() {
        assert_eq!(warnings(&text_shortcut(1, "A")), vec![Warning::NoModifier]);
        assert!(warnings(&text_shortcut(1, "F9")).is_empty());
        assert!(warnings(&text_shortcut(1, "MediaPlayPause")).is_empty());
        assert_eq!(warnings(&text_shortcut(1, "Shift+A")), vec![Warning::ShiftOnly]);
        assert_eq!(warnings(&text_shortcut(1, "Ctrl+C")), vec![Warning::CommonShortcut]);
        assert_eq!(warnings(&text_shortcut(1, "Alt+F4")), vec![Warning::CommonShortcut]);
        // Restricted to one application, Ctrl+C is a deliberate choice.
        let in_app = with_programs(text_shortcut(1, "Ctrl+C"), ProgramMode::OnlyIn, &["app.exe"]);
        assert!(warnings(&in_app).is_empty());
        assert_eq!(warnings(&text_shortcut(1, "Ctrl+Alt+2")), vec![Warning::AltGrCharacter]);
        // Scenario 1's combination raises no warning.
        assert!(warnings(&text_shortcut(1, "Ctrl+Alt+M")).is_empty());
        assert!(warnings(&text_shortcut(1, "Win+N")).is_empty());
    }

    #[test]
    fn reserved_combinations() {
        assert_eq!(unavailable(&"Ctrl+Alt+Delete".parse().unwrap()), Some(Unavailable::SecureAttention));
        assert_eq!(unavailable(&"Win+L".parse().unwrap()), Some(Unavailable::LockWorkstation));
        assert_eq!(unavailable(&"Win+E".parse().unwrap()), None);
        assert_eq!(unavailable(&"Ctrl+Delete".parse().unwrap()), None);
    }

    #[test]
    fn validation() {
        let ok = text_shortcut(1, "Ctrl+Alt+M");
        assert!(validate(&ok).is_empty());
        let empty_text = Shortcut::new(1, "Ctrl+Alt+M".parse().unwrap(), Action::text("", TextMode::Typing));
        assert_eq!(validate(&empty_text), vec![ValidationError::MissingText]);
        let empty_target = Shortcut::new(1, "Win+N".parse().unwrap(), Action::open("  "));
        assert_eq!(validate(&empty_target), vec![ValidationError::MissingTarget]);
        let no_programs = with_programs(ok.clone(), ProgramMode::OnlyIn, &[]);
        assert_eq!(validate(&no_programs), vec![ValidationError::MissingPrograms]);
        let reserved = Shortcut::new(1, "Win+L".parse().unwrap(), Action::open("x"));
        assert_eq!(validate(&reserved), vec![ValidationError::Unavailable(Unavailable::LockWorkstation)]);
    }
}
