//! Simulated keyboard input: typing Unicode text and handling held modifiers.

use crate::keys::{VK_MASK, modifier_vk, to_vk};
use declic_core::typing::TypeUnit;
use declic_core::{ClickKind, HeldKey, Hotkey, KeyList, ModState, Modifier, MouseButton, Side, WheelDirection};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSE_EVENT_FLAGS, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

/// Marker placed in `dwExtraInfo` of every event injected by Declic, so that
/// its own keyboard hook can recognise (and ignore) them.
pub const INJECTION_TAG: usize = 0x4443_4C4B;

/// Modifiers whose *logical* state was released by Declic while the user may
/// still physically hold them.
static RELEASED: AtomicU8 = AtomicU8::new(0);
/// Modifiers physically held, as tracked by the keyboard hook of this process.
static PHYSICAL: AtomicU8 = AtomicU8::new(0);

pub(crate) fn released_by_injection() -> ModState {
    ModState::from_bits(RELEASED.load(Ordering::Relaxed))
}

pub(crate) fn set_physical_modifiers(mods: ModState) {
    PHYSICAL.store(mods.bits(), Ordering::Relaxed);
    // Once physically released, a modifier no longer needs tracking.
    RELEASED.fetch_and(mods.bits(), Ordering::Relaxed);
}

/// Modifiers physically held according to the keyboard hook.
pub fn physical_modifiers() -> ModState {
    ModState::from_bits(PHYSICAL.load(Ordering::Relaxed))
}

/// Error raised when Windows refuses the simulated input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectError;

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the simulated keystrokes were rejected by Windows")
    }
}

impl std::error::Error for InjectError {}

fn is_extended_vk(vk: u16) -> bool {
    matches!(vk, 0xA3 | 0xA5 | 0x5B | 0x5C | 0x21..=0x28 | 0x2D | 0x2E | 0x6F | 0x90 | 0x5D)
}

fn key_input(vk: u16, up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended_vk(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    // SAFETY: plain query function.
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: INJECTION_TAG },
        },
    }
}

fn unicode_input(unit: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: unit, dwFlags: flags, time: 0, dwExtraInfo: INJECTION_TAG },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<(), InjectError> {
    if inputs.is_empty() {
        return Ok(());
    }
    // SAFETY: the slice is valid for the duration of the call.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() { Ok(()) } else { Err(InjectError) }
}

fn logically_down(vk: u16) -> bool {
    // SAFETY: plain query function.
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn logical_modifiers() -> ModState {
    let mut state = ModState::EMPTY;
    for m in Modifier::ALL {
        for side in [Side::Left, Side::Right] {
            if logically_down(modifier_vk(m, side)) {
                state = state.with(m, side, true);
            }
        }
    }
    state
}

fn mask_inputs() -> [INPUT; 2] {
    [key_input(VK_MASK, false), key_input(VK_MASK, true)]
}

/// When Win or Alt is held, sends a neutral keystroke so that releasing them
/// later does not open the Start menu or activate a menu bar (the main key of
/// the shortcut was hidden from the application).
pub fn mask_menu_keys() {
    let held = logical_modifiers();
    if held.any(Modifier::Win) || held.any(Modifier::Alt) {
        let _ = send(&mask_inputs());
    }
}

/// Releases (logically) every modifier currently held, so that the text
/// typed next is not altered by them (F-TXT-03). Returns the released set.
pub fn release_modifiers() -> Result<ModState, InjectError> {
    let held = logical_modifiers();
    if held.is_empty() {
        return Ok(held);
    }
    let mut inputs = Vec::with_capacity(10);
    if held.any(Modifier::Win) || held.any(Modifier::Alt) {
        inputs.extend(mask_inputs());
    }
    for m in Modifier::ALL {
        for side in [Side::Left, Side::Right] {
            if held.is_down(m, side) {
                inputs.push(key_input(modifier_vk(m, side), true));
            }
        }
    }
    RELEASED.fetch_or(held.bits(), Ordering::Relaxed);
    send(&inputs)?;
    Ok(held)
}

/// Presses again Ctrl and Shift if they were released by
/// [`release_modifiers`] and are still physically held. Alt and Win are not
/// restored: pressing them again would open a menu when they are released.
pub fn restore_modifiers(released: ModState) {
    let physical = physical_modifiers();
    let mut inputs = Vec::new();
    for m in [Modifier::Ctrl, Modifier::Shift] {
        for side in [Side::Left, Side::Right] {
            if released.is_down(m, side) && physical.is_down(m, side) {
                inputs.push(key_input(modifier_vk(m, side), false));
                RELEASED.fetch_and(!ModState::EMPTY.with(m, side, true).bits(), Ordering::Relaxed);
            }
        }
    }
    let _ = send(&inputs);
}

fn unit_inputs(unit: TypeUnit, out: &mut Vec<INPUT>) {
    match unit {
        TypeUnit::Unit(u) => {
            out.push(unicode_input(u, false));
            out.push(unicode_input(u, true));
        }
        TypeUnit::Enter => {
            out.push(key_input(0x0D, false));
            out.push(key_input(0x0D, true));
        }
        TypeUnit::Tab => {
            out.push(key_input(0x09, false));
            out.push(key_input(0x09, true));
        }
    }
}

/// Types the planned units. With a zero delay the text is sent in large
/// batches (fast, and the user's own keystrokes cannot interleave); otherwise
/// one keystroke at a time with the given pause, for slow applications.
pub fn type_units(units: &[TypeUnit], delay: Duration) -> Result<(), InjectError> {
    const BATCH: usize = 2048;
    if delay.is_zero() {
        for chunk in units.chunks(BATCH) {
            let mut inputs = Vec::with_capacity(chunk.len() * 2);
            for &unit in chunk {
                unit_inputs(unit, &mut inputs);
            }
            send(&inputs)?;
        }
    } else {
        let mut inputs = Vec::with_capacity(2);
        for &unit in units {
            inputs.clear();
            unit_inputs(unit, &mut inputs);
            send(&inputs)?;
            std::thread::sleep(delay);
        }
    }
    Ok(())
}

fn key_event(vk: u16, extended: bool, up: bool) -> INPUT {
    let mut input = key_input(vk, up);
    if extended {
        // SAFETY: the union holds the keyboard input built just above.
        unsafe { input.Anonymous.ki.dwFlags |= KEYEVENTF_EXTENDEDKEY };
    }
    input
}

fn held_key_vk(key: HeldKey) -> (u16, bool) {
    match key {
        HeldKey::Modifier(m, side) => (modifier_vk(m, side), side == Side::Right || m == Modifier::Win),
        HeldKey::Key(k) => to_vk(k),
    }
}

/// Presses a combination `repeat` times (modifiers, then the key; the
/// modifiers are released after each press).
pub fn press_hotkey(hotkey: &Hotkey, repeat: u32, delay: Duration) -> Result<(), InjectError> {
    let keys = KeyList::from_hotkey(hotkey);
    let mut once = Vec::new();
    for &k in &keys.0 {
        let (vk, ext) = held_key_vk(k);
        once.push(key_event(vk, ext, false));
    }
    for &k in keys.0.iter().rev() {
        let (vk, ext) = held_key_vk(k);
        once.push(key_event(vk, ext, true));
    }
    let needs_mask = keys.0.iter().any(|k| matches!(k, HeldKey::Modifier(Modifier::Alt | Modifier::Win, _)))
        && keys.0.len() == 1;
    for _ in 0..repeat.clamp(1, 10_000) {
        if needs_mask {
            // A lone Alt/Win press would open a menu or the Start menu.
            let mut inputs = vec![once[0]];
            inputs.extend(mask_inputs());
            inputs.push(once[1]);
            send(&inputs)?;
        } else {
            send(&once)?;
        }
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
    Ok(())
}

/// Presses keys without releasing them.
pub fn hold_keys(keys: &[HeldKey]) -> Result<(), InjectError> {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&k| {
            let (vk, ext) = held_key_vk(k);
            key_event(vk, ext, false)
        })
        .collect();
    send(&inputs)
}

/// Releases keys (in reverse order). When Alt or Win is among them, a
/// neutral key is sent first so that releasing them opens no menu.
pub fn release_keys(keys: &[HeldKey]) -> Result<(), InjectError> {
    let mut inputs = Vec::new();
    if keys.iter().any(|k| matches!(k, HeldKey::Modifier(Modifier::Alt | Modifier::Win, _))) {
        inputs.extend(mask_inputs());
    }
    for &k in keys.iter().rev() {
        let (vk, ext) = held_key_vk(k);
        inputs.push(key_event(vk, ext, true));
    }
    send(&inputs)
}

/// Runs `f` while a (tagged) Alt key is held, then releases Alt after a
/// neutral key: Windows allows a foreground change after an Alt press.
pub fn alt_nudge(f: impl FnOnce()) {
    let _ = send(&[key_input(0xA4, false)]);
    f();
    let mut inputs = mask_inputs().to_vec();
    inputs.push(key_input(0xA4, true));
    let _ = send(&inputs);
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: INJECTION_TAG },
        },
    }
}

/// Current pointer position (physical pixels of the virtual screen).
pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    // SAFETY: plain query.
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

/// Moves the pointer to a position of the virtual screen.
pub fn move_cursor(x: i32, y: i32) -> Result<(), InjectError> {
    // SAFETY: plain call.
    unsafe { SetCursorPos(x, y) }.map_err(|_| InjectError)
}

/// Clicks at the current pointer position.
pub fn click(button: MouseButton, kind: ClickKind) -> Result<(), InjectError> {
    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let inputs: Vec<INPUT> = match kind {
        ClickKind::Single => vec![mouse_input(down, 0), mouse_input(up, 0)],
        ClickKind::Double => vec![mouse_input(down, 0), mouse_input(up, 0), mouse_input(down, 0), mouse_input(up, 0)],
        ClickKind::Down => vec![mouse_input(down, 0)],
        ClickKind::Up => vec![mouse_input(up, 0)],
    };
    send(&inputs)
}

/// Releases a mouse button (used when a macro is interrupted).
pub fn release_button(button: MouseButton) {
    let _ = click(button, ClickKind::Up);
}

/// Turns the wheel by a number of notches.
pub fn wheel(direction: WheelDirection, notches: u32) -> Result<(), InjectError> {
    const NOTCH: i32 = 120;
    let n = notches.min(1000) as i32;
    let input = match direction {
        WheelDirection::Up => mouse_input(MOUSEEVENTF_WHEEL, NOTCH * n),
        WheelDirection::Down => mouse_input(MOUSEEVENTF_WHEEL, -NOTCH * n),
        WheelDirection::Right => mouse_input(MOUSEEVENTF_HWHEEL, NOTCH * n),
        WheelDirection::Left => mouse_input(MOUSEEVENTF_HWHEEL, -NOTCH * n),
    };
    send(&[input])
}
