//! Detection of combinations already registered by Windows or another
//! application (F-CNF-02).
//!
//! Programs usually reserve global combinations with `RegisterHotKey`; trying
//! to register the same combination fails with "already registered". Declic
//! itself uses a keyboard hook, so it can still take over such a combination
//! (F-COMB-06), but the user is warned that it is also used elsewhere.

use crate::keys::to_vk;
use declic_core::{Hotkey, ModReq, Modifier};
use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};

/// Result of a probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Nobody else registered the combination.
    Free,
    /// Windows or another application registered it.
    UsedElsewhere,
}

const PROBE_ID: i32 = 0xB00C;

/// Checks whether a combination is registered by another program. Must be
/// called from a thread that may own hot keys (any thread; the probe is
/// released immediately).
pub fn probe(hotkey: &Hotkey) -> Availability {
    let (vk, _) = to_vk(hotkey.key);
    if vk == 0 {
        return Availability::Free;
    }
    let mut mods = MOD_NOREPEAT;
    for (m, flag) in [(Modifier::Ctrl, MOD_CONTROL), (Modifier::Alt, MOD_ALT), (Modifier::Shift, MOD_SHIFT), (Modifier::Win, MOD_WIN)] {
        if hotkey.req(m) != ModReq::Off {
            mods |= flag;
        }
    }
    // SAFETY: registers then immediately unregisters a thread hot key.
    unsafe {
        match RegisterHotKey(None, PROBE_ID, HOT_KEY_MODIFIERS(mods.0), vk as u32) {
            Ok(()) => {
                let _ = UnregisterHotKey(None, PROBE_ID);
                Availability::Free
            }
            Err(_) if GetLastError() == ERROR_HOTKEY_ALREADY_REGISTERED => Availability::UsedElsewhere,
            Err(_) => Availability::Free,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unusual_combination_is_free() {
        let hk: Hotkey = "Ctrl+Alt+Shift+F23".parse().unwrap();
        assert_eq!(probe(&hk), Availability::Free);
        // Probing twice must not leave the probe registered.
        assert_eq!(probe(&hk), Availability::Free);
    }

    /// Prints the result for a few well-known combinations (informative).
    #[test]
    #[ignore]
    fn print_common_combinations() {
        for text in ["Win+E", "Win+R", "Win+Shift+S", "Alt+Tab", "F12", "Ctrl+Alt+M", "Win+N", "Ctrl+Alt+Q", "Ctrl+Alt+Y", "Ctrl+Shift+F7", "Alt+F9", "Ctrl+Alt+Shift+F22", "Ctrl+Alt+K", "Ctrl+Alt+Num1"] {
            let hk: Hotkey = text.parse().unwrap();
            println!("{text}: {:?}", probe(&hk));
        }
    }
}