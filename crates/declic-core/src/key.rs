//! Keyboard-layout-independent key identifiers.
//!
//! A [`Key`] names a *key*, not the character it produces: `Key::A` is the key
//! that produces "A" in the user's current layout, `Key::Oem1` is the first
//! "punctuation" key whose printed symbol depends on the layout, and the
//! numeric keypad keys are distinct from the top-row digits.
//!
//! Identifiers (see [`Key::id`]) are stable ASCII strings used in the
//! configuration file. How a key is *displayed* is decided by the UI, which may
//! ask the platform layer for the character printed on the key.

use std::borrow::Cow;
use std::fmt;

/// Broad family of a key, used to group keys in pickers and for risk checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCategory {
    Letter,
    Digit,
    Function,
    Numpad,
    Navigation,
    Editing,
    System,
    Punctuation,
    Media,
    Browser,
    Launch,
    Other,
}

macro_rules! define_keys {
    ($( $variant:ident => $id:literal, $cat:ident; )*) => {
        /// A key on the keyboard, independent from the active layout.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Key {
            $( $variant, )*
            /// Any other key, identified by a platform key code.
            Other(u16),
        }

        impl Key {
            /// Every named key, in a sensible display order.
            pub const ALL: &'static [Key] = &[ $( Key::$variant, )* ];

            /// Stable identifier used in configuration files.
            pub fn id(self) -> Cow<'static, str> {
                match self {
                    $( Key::$variant => Cow::Borrowed($id), )*
                    Key::Other(code) => Cow::Owned(format!("Code{code}")),
                }
            }

            /// Family of the key.
            pub fn category(self) -> KeyCategory {
                match self {
                    $( Key::$variant => KeyCategory::$cat, )*
                    Key::Other(_) => KeyCategory::Other,
                }
            }

            fn named_from_id(id: &str) -> Option<Key> {
                $( if id.eq_ignore_ascii_case($id) { return Some(Key::$variant); } )*
                None
            }
        }
    };
}

define_keys! {
    A => "A", Letter; B => "B", Letter; C => "C", Letter; D => "D", Letter;
    E => "E", Letter; F => "F", Letter; G => "G", Letter; H => "H", Letter;
    I => "I", Letter; J => "J", Letter; K => "K", Letter; L => "L", Letter;
    M => "M", Letter; N => "N", Letter; O => "O", Letter; P => "P", Letter;
    Q => "Q", Letter; R => "R", Letter; S => "S", Letter; T => "T", Letter;
    U => "U", Letter; V => "V", Letter; W => "W", Letter; X => "X", Letter;
    Y => "Y", Letter; Z => "Z", Letter;

    D0 => "0", Digit; D1 => "1", Digit; D2 => "2", Digit; D3 => "3", Digit;
    D4 => "4", Digit; D5 => "5", Digit; D6 => "6", Digit; D7 => "7", Digit;
    D8 => "8", Digit; D9 => "9", Digit;

    F1 => "F1", Function; F2 => "F2", Function; F3 => "F3", Function;
    F4 => "F4", Function; F5 => "F5", Function; F6 => "F6", Function;
    F7 => "F7", Function; F8 => "F8", Function; F9 => "F9", Function;
    F10 => "F10", Function; F11 => "F11", Function; F12 => "F12", Function;
    F13 => "F13", Function; F14 => "F14", Function; F15 => "F15", Function;
    F16 => "F16", Function; F17 => "F17", Function; F18 => "F18", Function;
    F19 => "F19", Function; F20 => "F20", Function; F21 => "F21", Function;
    F22 => "F22", Function; F23 => "F23", Function; F24 => "F24", Function;

    Numpad0 => "Num0", Numpad; Numpad1 => "Num1", Numpad; Numpad2 => "Num2", Numpad;
    Numpad3 => "Num3", Numpad; Numpad4 => "Num4", Numpad; Numpad5 => "Num5", Numpad;
    Numpad6 => "Num6", Numpad; Numpad7 => "Num7", Numpad; Numpad8 => "Num8", Numpad;
    Numpad9 => "Num9", Numpad;
    NumpadAdd => "NumAdd", Numpad; NumpadSubtract => "NumSubtract", Numpad;
    NumpadMultiply => "NumMultiply", Numpad; NumpadDivide => "NumDivide", Numpad;
    NumpadDecimal => "NumDecimal", Numpad; NumpadEnter => "NumEnter", Numpad;

    Left => "Left", Navigation; Right => "Right", Navigation;
    Up => "Up", Navigation; Down => "Down", Navigation;
    Home => "Home", Navigation; End => "End", Navigation;
    PageUp => "PageUp", Navigation; PageDown => "PageDown", Navigation;

    Escape => "Escape", Editing; Tab => "Tab", Editing; Enter => "Enter", Editing;
    Space => "Space", Editing; Backspace => "Backspace", Editing;
    Insert => "Insert", Editing; Delete => "Delete", Editing;

    PrintScreen => "PrintScreen", System; Pause => "Pause", System;
    ScrollLock => "ScrollLock", System; CapsLock => "CapsLock", System;
    NumLock => "NumLock", System; Menu => "Menu", System; Sleep => "Sleep", System;

    Oem1 => "Oem1", Punctuation; OemPlus => "OemPlus", Punctuation;
    OemComma => "OemComma", Punctuation; OemMinus => "OemMinus", Punctuation;
    OemPeriod => "OemPeriod", Punctuation; Oem2 => "Oem2", Punctuation;
    Oem3 => "Oem3", Punctuation; Oem4 => "Oem4", Punctuation;
    Oem5 => "Oem5", Punctuation; Oem6 => "Oem6", Punctuation;
    Oem7 => "Oem7", Punctuation; Oem8 => "Oem8", Punctuation;
    Oem102 => "Oem102", Punctuation;

    MediaPlayPause => "MediaPlayPause", Media; MediaStop => "MediaStop", Media;
    MediaNext => "MediaNext", Media; MediaPrevious => "MediaPrevious", Media;
    VolumeMute => "VolumeMute", Media; VolumeDown => "VolumeDown", Media;
    VolumeUp => "VolumeUp", Media;

    BrowserBack => "BrowserBack", Browser; BrowserForward => "BrowserForward", Browser;
    BrowserRefresh => "BrowserRefresh", Browser; BrowserStop => "BrowserStop", Browser;
    BrowserSearch => "BrowserSearch", Browser; BrowserFavorites => "BrowserFavorites", Browser;
    BrowserHome => "BrowserHome", Browser;

    LaunchMail => "LaunchMail", Launch; LaunchMedia => "LaunchMedia", Launch;
    LaunchApp1 => "LaunchApp1", Launch; LaunchApp2 => "LaunchApp2", Launch;
}

impl Key {
    /// Parses a key identifier (case-insensitive). Accepts `CodeNNN` for
    /// [`Key::Other`] and a few friendly aliases.
    pub fn from_id(id: &str) -> Option<Key> {
        let id = id.trim();
        if id.is_empty() {
            return None;
        }
        if let Some(key) = Key::named_from_id(id) {
            return Some(key);
        }
        if let Some(rest) = strip_prefix_ci(id, "Code") {
            return rest.parse::<u16>().ok().map(Key::Other);
        }
        let alias = match id.to_lowercase().as_str() {
            "esc" | "echap" | "échap" => Key::Escape,
            "return" | "entree" | "entrée" => Key::Enter,
            "del" | "suppr" => Key::Delete,
            "ins" | "inser" => Key::Insert,
            "pgup" | "pageprec" => Key::PageUp,
            "pgdn" | "pagesuiv" => Key::PageDown,
            "espace" => Key::Space,
            "apps" | "application" | "contextmenu" => Key::Menu,
            "prtsc" | "printscr" | "impr" => Key::PrintScreen,
            "break" => Key::Pause,
            "calculator" | "calculatrice" => Key::LaunchApp2,
            "mail" => Key::LaunchMail,
            _ => return None,
        };
        Some(alias)
    }

    /// Whether the key normally produces a character when typed without
    /// modifiers (letters, digits, punctuation, space).
    pub fn is_character(self) -> bool {
        matches!(
            self.category(),
            KeyCategory::Letter | KeyCategory::Digit | KeyCategory::Punctuation
        ) || self == Key::Space
    }

    /// Whether the key is one of the lock keys.
    pub fn is_lock(self) -> bool {
        matches!(self, Key::CapsLock | Key::NumLock | Key::ScrollLock)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id())
    }
}

pub(crate) fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix)
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_for_every_named_key() {
        for &key in Key::ALL {
            assert_eq!(Key::from_id(&key.id()), Some(key), "{key:?}");
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<String> = Key::ALL.iter().map(|k| k.id().to_lowercase()).collect();
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(before, ids.len());
    }

    #[test]
    fn parsing_is_case_insensitive_and_supports_aliases() {
        assert_eq!(Key::from_id("m"), Some(Key::M));
        assert_eq!(Key::from_id("num1"), Some(Key::Numpad1));
        assert_eq!(Key::from_id("Esc"), Some(Key::Escape));
        assert_eq!(Key::from_id("Code232"), Some(Key::Other(232)));
        assert_eq!(Key::Other(232).id(), "Code232");
        assert_eq!(Key::from_id(""), None);
        assert_eq!(Key::from_id("NotAKey"), None);
    }

    #[test]
    fn numpad_digits_are_distinct_from_top_row() {
        assert_ne!(Key::from_id("1"), Key::from_id("Num1"));
        assert_eq!(Key::D1.category(), KeyCategory::Digit);
        assert_eq!(Key::Numpad1.category(), KeyCategory::Numpad);
    }

    #[test]
    fn character_keys() {
        assert!(Key::A.is_character());
        assert!(Key::Space.is_character());
        assert!(Key::Oem1.is_character());
        assert!(!Key::F5.is_character());
        assert!(!Key::MediaPlayPause.is_character());
    }
}
