//! Mapping between Windows virtual-key codes and Declic's layout-independent keys.

use crate::wide::from_wide;
use declic_core::{Key, Modifier, Side};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyNameTextW, GetKeyboardLayout, MAPVK_VK_TO_CHAR, MAPVK_VK_TO_VSC, MapVirtualKeyExW,
};

/// What a raw keyboard event represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyInput {
    Modifier(Modifier, Side),
    Key(Key),
}

/// Virtual-key code sent by Declic to "mask" a released Win or Alt key, so
/// that releasing it does not open the Start menu or an application menu.
/// 0xE8 is unassigned.
pub const VK_MASK: u16 = 0xE8;

const LETTERS: [Key; 26] = [
    Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L, Key::M,
    Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
];
const DIGITS: [Key; 10] = [Key::D0, Key::D1, Key::D2, Key::D3, Key::D4, Key::D5, Key::D6, Key::D7, Key::D8, Key::D9];
const NUMPAD: [Key; 10] = [
    Key::Numpad0, Key::Numpad1, Key::Numpad2, Key::Numpad3, Key::Numpad4,
    Key::Numpad5, Key::Numpad6, Key::Numpad7, Key::Numpad8, Key::Numpad9,
];
const FKEYS: [Key; 24] = [
    Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12,
    Key::F13, Key::F14, Key::F15, Key::F16, Key::F17, Key::F18, Key::F19, Key::F20, Key::F21, Key::F22, Key::F23,
    Key::F24,
];

/// Interprets a keyboard event. `extended` is the "extended key" flag, which
/// distinguishes e.g. the arrow keys from the numeric keypad with Num Lock off.
/// Returns `None` for events that must be ignored (unassigned codes).
pub fn classify(vk: u16, scan: u32, extended: bool) -> Option<KeyInput> {
    use KeyInput::{Key as K, Modifier as M};
    let key = match vk {
        0x10 => return Some(M(Modifier::Shift, if scan == 0x36 { Side::Right } else { Side::Left })),
        0x11 => return Some(M(Modifier::Ctrl, if extended { Side::Right } else { Side::Left })),
        0x12 => return Some(M(Modifier::Alt, if extended { Side::Right } else { Side::Left })),
        0xA0 => return Some(M(Modifier::Shift, Side::Left)),
        0xA1 => return Some(M(Modifier::Shift, Side::Right)),
        0xA2 => return Some(M(Modifier::Ctrl, Side::Left)),
        0xA3 => return Some(M(Modifier::Ctrl, Side::Right)),
        0xA4 => return Some(M(Modifier::Alt, Side::Left)),
        0xA5 => return Some(M(Modifier::Alt, Side::Right)),
        0x5B => return Some(M(Modifier::Win, Side::Left)),
        0x5C => return Some(M(Modifier::Win, Side::Right)),
        0x00 | 0xFF | VK_MASK => return None,
        0x08 => Key::Backspace,
        0x09 => Key::Tab,
        0x0C => Key::Numpad5, // "Clear": keypad 5 with Num Lock off
        0x0D => if extended { Key::NumpadEnter } else { Key::Enter },
        0x13 => Key::Pause,
        0x14 => Key::CapsLock,
        0x1B => Key::Escape,
        0x20 => Key::Space,
        // Navigation keys without the extended flag come from the keypad (Num Lock off).
        0x21 => if extended { Key::PageUp } else { Key::Numpad9 },
        0x22 => if extended { Key::PageDown } else { Key::Numpad3 },
        0x23 => if extended { Key::End } else { Key::Numpad1 },
        0x24 => if extended { Key::Home } else { Key::Numpad7 },
        0x25 => if extended { Key::Left } else { Key::Numpad4 },
        0x26 => if extended { Key::Up } else { Key::Numpad8 },
        0x27 => if extended { Key::Right } else { Key::Numpad6 },
        0x28 => if extended { Key::Down } else { Key::Numpad2 },
        0x2C => Key::PrintScreen,
        0x2D => if extended { Key::Insert } else { Key::Numpad0 },
        0x2E => if extended { Key::Delete } else { Key::NumpadDecimal },
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x5D => Key::Menu,
        0x5F => Key::Sleep,
        0x60..=0x69 => NUMPAD[(vk - 0x60) as usize],
        0x6A => Key::NumpadMultiply,
        0x6B => Key::NumpadAdd,
        0x6D => Key::NumpadSubtract,
        0x6E => Key::NumpadDecimal,
        0x6F => Key::NumpadDivide,
        0x70..=0x87 => FKEYS[(vk - 0x70) as usize],
        0x90 => Key::NumLock,
        0x91 => Key::ScrollLock,
        0xA6 => Key::BrowserBack,
        0xA7 => Key::BrowserForward,
        0xA8 => Key::BrowserRefresh,
        0xA9 => Key::BrowserStop,
        0xAA => Key::BrowserSearch,
        0xAB => Key::BrowserFavorites,
        0xAC => Key::BrowserHome,
        0xAD => Key::VolumeMute,
        0xAE => Key::VolumeDown,
        0xAF => Key::VolumeUp,
        0xB0 => Key::MediaNext,
        0xB1 => Key::MediaPrevious,
        0xB2 => Key::MediaStop,
        0xB3 => Key::MediaPlayPause,
        0xB4 => Key::LaunchMail,
        0xB5 => Key::LaunchMedia,
        0xB6 => Key::LaunchApp1,
        0xB7 => Key::LaunchApp2,
        0xBA => Key::Oem1,
        0xBB => Key::OemPlus,
        0xBC => Key::OemComma,
        0xBD => Key::OemMinus,
        0xBE => Key::OemPeriod,
        0xBF => Key::Oem2,
        0xC0 => Key::Oem3,
        0xDB => Key::Oem4,
        0xDC => Key::Oem5,
        0xDD => Key::Oem6,
        0xDE => Key::Oem7,
        0xDF => Key::Oem8,
        0xE2 => Key::Oem102,
        other => Key::Other(other),
    };
    Some(K(key))
}

/// Virtual-key code and extended flag used to *send* a key.
pub fn to_vk(key: Key) -> (u16, bool) {
    let position = |list: &[Key]| list.iter().position(|&k| k == key).map(|i| i as u16);
    if let Some(i) = position(&LETTERS) {
        return (0x41 + i, false);
    }
    if let Some(i) = position(&DIGITS) {
        return (0x30 + i, false);
    }
    if let Some(i) = position(&NUMPAD) {
        return (0x60 + i, false);
    }
    if let Some(i) = position(&FKEYS) {
        return (0x70 + i, false);
    }
    match key {
        Key::Backspace => (0x08, false),
        Key::Tab => (0x09, false),
        Key::Enter => (0x0D, false),
        Key::NumpadEnter => (0x0D, true),
        Key::Pause => (0x13, false),
        Key::CapsLock => (0x14, false),
        Key::Escape => (0x1B, false),
        Key::Space => (0x20, false),
        Key::PageUp => (0x21, true),
        Key::PageDown => (0x22, true),
        Key::End => (0x23, true),
        Key::Home => (0x24, true),
        Key::Left => (0x25, true),
        Key::Up => (0x26, true),
        Key::Right => (0x27, true),
        Key::Down => (0x28, true),
        Key::PrintScreen => (0x2C, true),
        Key::Insert => (0x2D, true),
        Key::Delete => (0x2E, true),
        Key::Menu => (0x5D, true),
        Key::Sleep => (0x5F, false),
        Key::NumpadMultiply => (0x6A, false),
        Key::NumpadAdd => (0x6B, false),
        Key::NumpadSubtract => (0x6D, false),
        Key::NumpadDecimal => (0x6E, false),
        Key::NumpadDivide => (0x6F, true),
        Key::NumLock => (0x90, true),
        Key::ScrollLock => (0x91, false),
        Key::BrowserBack => (0xA6, true),
        Key::BrowserForward => (0xA7, true),
        Key::BrowserRefresh => (0xA8, true),
        Key::BrowserStop => (0xA9, true),
        Key::BrowserSearch => (0xAA, true),
        Key::BrowserFavorites => (0xAB, true),
        Key::BrowserHome => (0xAC, true),
        Key::VolumeMute => (0xAD, true),
        Key::VolumeDown => (0xAE, true),
        Key::VolumeUp => (0xAF, true),
        Key::MediaNext => (0xB0, true),
        Key::MediaPrevious => (0xB1, true),
        Key::MediaStop => (0xB2, true),
        Key::MediaPlayPause => (0xB3, true),
        Key::LaunchMail => (0xB4, true),
        Key::LaunchMedia => (0xB5, true),
        Key::LaunchApp1 => (0xB6, true),
        Key::LaunchApp2 => (0xB7, true),
        Key::Oem1 => (0xBA, false),
        Key::OemPlus => (0xBB, false),
        Key::OemComma => (0xBC, false),
        Key::OemMinus => (0xBD, false),
        Key::OemPeriod => (0xBE, false),
        Key::Oem2 => (0xBF, false),
        Key::Oem3 => (0xC0, false),
        Key::Oem4 => (0xDB, false),
        Key::Oem5 => (0xDC, false),
        Key::Oem6 => (0xDD, false),
        Key::Oem7 => (0xDE, false),
        Key::Oem8 => (0xDF, false),
        Key::Oem102 => (0xE2, false),
        Key::Other(code) => (code, false),
        // Letters, digits, numpad digits and function keys are handled above.
        _ => (0, false),
    }
}

/// Virtual-key code of a modifier key.
pub fn modifier_vk(m: Modifier, side: Side) -> u16 {
    match (m, side) {
        (Modifier::Shift, Side::Left) => 0xA0,
        (Modifier::Shift, Side::Right) => 0xA1,
        (Modifier::Ctrl, Side::Left) => 0xA2,
        (Modifier::Ctrl, Side::Right) => 0xA3,
        (Modifier::Alt, Side::Left) => 0xA4,
        (Modifier::Alt, Side::Right) => 0xA5,
        (Modifier::Win, Side::Left) => 0x5B,
        (Modifier::Win, Side::Right) => 0x5C,
    }
}

/// Character printed on a key in the current keyboard layout (letters,
/// digits and punctuation), e.g. `$` for `Oem1` on a French AZERTY layout.
pub fn layout_char(key: Key) -> Option<char> {
    if !key.is_character() || key == Key::Space {
        return None;
    }
    let (vk, _) = to_vk(key);
    // SAFETY: plain query functions without pointers.
    let code = unsafe { MapVirtualKeyExW(vk as u32, MAPVK_VK_TO_CHAR, Some(GetKeyboardLayout(0))) };
    let c = char::from_u32(code & 0x7FFF_FFFF)?; // high bit: dead key
    (!c.is_control() && c != '\0').then(|| c.to_uppercase().next().unwrap_or(c))
}

/// Name given by Windows to a key (in the language of the keyboard layout),
/// used for keys without a translated name.
pub fn system_key_name(key: Key) -> Option<String> {
    let (vk, extended) = to_vk(key);
    if vk == 0 {
        return None;
    }
    // SAFETY: the buffer outlives the call.
    unsafe {
        let scan = MapVirtualKeyExW(vk as u32, MAPVK_VK_TO_VSC, Some(GetKeyboardLayout(0)));
        if scan == 0 {
            return None;
        }
        let lparam = ((scan as i32) << 16) | if extended { 1 << 24 } else { 0 };
        let mut buf = [0u16; 64];
        let len = GetKeyNameTextW(lparam, &mut buf);
        (len > 0).then(|| from_wide(&buf[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_round_trip_through_virtual_keys() {
        for &key in Key::ALL {
            let (vk, extended) = to_vk(key);
            assert_ne!(vk, 0, "{key:?} has no virtual key");
            let back = classify(vk, 0, extended);
            assert_eq!(back, Some(KeyInput::Key(key)), "{key:?}");
        }
    }

    #[test]
    fn keypad_with_num_lock_off_is_still_the_keypad() {
        assert_eq!(classify(0x23, 0x4F, false), Some(KeyInput::Key(Key::Numpad1)));
        assert_eq!(classify(0x23, 0x4F, true), Some(KeyInput::Key(Key::End)));
        assert_eq!(classify(0x0D, 0x1C, true), Some(KeyInput::Key(Key::NumpadEnter)));
    }

    #[test]
    fn modifiers_are_recognised_with_their_side() {
        assert_eq!(classify(0xA3, 0x1D, true), Some(KeyInput::Modifier(Modifier::Ctrl, Side::Right)));
        assert_eq!(classify(0x5B, 0x5B, true), Some(KeyInput::Modifier(Modifier::Win, Side::Left)));
        assert_eq!(classify(0x10, 0x36, false), Some(KeyInput::Modifier(Modifier::Shift, Side::Right)));
        assert_eq!(classify(VK_MASK, 0, false), None);
    }
}
