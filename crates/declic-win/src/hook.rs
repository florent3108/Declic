//! Low-level keyboard hook running on its own thread.
//!
//! The hook procedure must return quickly (Windows removes slow hooks), so it
//! runs on a dedicated thread that does nothing else than pumping messages;
//! the decision callback must only do fast, non-blocking work.

use crate::inject::{INJECTION_TAG, released_by_injection, set_physical_modifiers};
use crate::keys::{KeyInput, classify, modifier_vk};
use declic_core::{LockState, ModState, Modifier, Side};
use std::cell::RefCell;
use std::io;
use std::sync::mpsc;
use std::thread::JoinHandle;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED,
    LLKHF_UP, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_QUIT, WM_USER,
};

/// A keyboard event as seen by the hook.
#[derive(Debug, Clone, Copy)]
pub struct HookEvent {
    pub input: KeyInput,
    pub vk: u16,
    pub down: bool,
    /// Auto-repeat of a key that is already held.
    pub repeat: bool,
    /// Injected by another program (on-screen keyboard, remote tools…).
    pub injected: bool,
    /// Modifiers physically held *before* this event.
    pub mods: ModState,
    /// Lock-key states when a key is pressed (default for other events).
    pub locks: LockState,
}

/// What to do with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Let the event reach the application normally.
    Pass,
    /// Hide the event from every application.
    Swallow,
}

type Handler = Box<dyn FnMut(&HookEvent) -> Decision>;

struct HookState {
    handler: Handler,
    resync: bool,
    mods: ModState,
    pressed: [bool; 256],
}

thread_local! {
    static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

/// Fake left-Ctrl sent by Windows before Right Alt on layouts with AltGr.
const ALTGR_FAKE_CTRL_SCAN: u32 = 0x21D;

fn is_down(vk: u16) -> bool {
    // SAFETY: plain query function.
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn toggled(vk: i32) -> bool {
    // SAFETY: plain query function.
    unsafe { GetKeyState(vk) & 1 != 0 }
}

impl HookState {
    /// Forgets modifiers that are no longer held although their release was
    /// not seen (e.g. released while the secure desktop was shown).
    fn resync_modifiers(&mut self) {
        let ours = released_by_injection();
        for m in Modifier::ALL {
            for side in [Side::Left, Side::Right] {
                if self.mods.is_down(m, side) && !ours.is_down(m, side) && !is_down(modifier_vk(m, side)) {
                    self.mods = self.mods.with(m, side, false);
                    self.pressed[modifier_vk(m, side) as usize] = false;
                    set_physical_modifiers(self.mods);
                }
            }
        }
    }

    fn process(&mut self, info: &KBDLLHOOKSTRUCT) -> Decision {
        let vk = info.vkCode as u16;
        let up = info.flags.0 & LLKHF_UP.0 != 0;
        let extended = info.flags.0 & LLKHF_EXTENDED.0 != 0;
        let injected = info.flags.0 & LLKHF_INJECTED.0 != 0;
        if info.dwExtraInfo == INJECTION_TAG || (vk == 0xA2 && info.scanCode == ALTGR_FAKE_CTRL_SCAN) {
            return Decision::Pass;
        }
        let Some(input) = classify(vk, info.scanCode, extended) else {
            return Decision::Pass;
        };
        let index = (vk & 0xFF) as usize;
        let repeat = !up && self.pressed[index];
        self.pressed[index] = !up;
        if self.resync && matches!(input, KeyInput::Key(_)) && !up {
            self.resync_modifiers();
        }
        // Lock states are read at each key press: toggle states are kept in
        // sync by Windows for every thread, so they cannot drift.
        let locks = if matches!(input, KeyInput::Key(_)) && !up {
            LockState { caps: toggled(0x14), num: toggled(0x90), scroll: toggled(0x91) }
        } else {
            LockState::default()
        };
        let event = HookEvent { input, vk, down: !up, repeat, injected, mods: self.mods, locks };
        if let KeyInput::Modifier(m, side) = input {
            self.mods = self.mods.with(m, side, !up);
            set_physical_modifiers(self.mods);
        }
        (self.handler)(&event)
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let decision = STATE.with(|state| match state.try_borrow_mut() {
            Ok(mut guard) => guard.as_mut().map(|s| s.process(info)).unwrap_or(Decision::Pass),
            Err(_) => Decision::Pass,
        });
        if decision == Decision::Swallow {
            return LRESULT(1);
        }
    }
    // SAFETY: forwarding the unchanged parameters.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// A running keyboard hook; dropping it removes the hook.
pub struct KeyboardHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl KeyboardHook {
    /// Installs a low-level keyboard hook calling `handler` for every event.
    pub fn start<F>(handler: F) -> io::Result<KeyboardHook>
    where
        F: FnMut(&HookEvent) -> Decision + Send + 'static,
    {
        Self::start_with(handler, true)
    }

    /// Installs a hook whose handler swallows every event (key capture). The
    /// logical key state then never changes, so it is not used to
    /// resynchronise the modifiers.
    pub fn start_exclusive<F>(handler: F) -> io::Result<KeyboardHook>
    where
        F: FnMut(&HookEvent) -> Decision + Send + 'static,
    {
        Self::start_with(handler, false)
    }

    fn start_with<F>(handler: F, resync: bool) -> io::Result<KeyboardHook>
    where
        F: FnMut(&HookEvent) -> Decision + Send + 'static,
    {
        let (ready_tx, ready_rx) = mpsc::channel::<io::Result<u32>>();
        let thread = std::thread::Builder::new()
            .name("declic-keyboard-hook".into())
            .spawn(move || {
                // SAFETY: standard hook installation and message loop on this thread.
                unsafe {
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                    let mut msg = MSG::default();
                    // Creates the message queue before announcing readiness.
                    let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
                    STATE.with(|s| {
                        *s.borrow_mut() =
                            Some(HookState { handler: Box::new(handler), resync, mods: ModState::EMPTY, pressed: [false; 256] })
                    });
                    let module = GetModuleHandleW(None).ok().map(|m| m.into());
                    let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0) {
                        Ok(hook) => hook,
                        Err(e) => {
                            let _ = ready_tx.send(Err(io::Error::other(e)));
                            return;
                        }
                    };
                    let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let _ = UnhookWindowsHookEx(hook);
                    STATE.with(|s| *s.borrow_mut() = None);
                }
            })?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(KeyboardHook { thread_id, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(io::Error::other("keyboard hook thread ended unexpectedly")),
        }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        // SAFETY: posting WM_QUIT to the hook thread's queue.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use declic_core::Key;
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
    };

    fn key(vk: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Injects F24 (a key without any effect) and checks that an exclusive
    /// hook sees and swallows it. Ignored by default: it simulates input.
    #[test]
    #[ignore]
    fn exclusive_hook_receives_injected_keys() {
        let (tx, rx) = mpsc::channel();
        let hook = KeyboardHook::start_exclusive(move |event| {
            let _ = tx.send((event.input, event.down));
            Decision::Swallow
        })
        .unwrap();
        // SAFETY: the inputs are valid for the call.
        unsafe { SendInput(&[key(0x87, false), key(0x87, true)], std::mem::size_of::<INPUT>() as i32) };
        let first = rx.recv_timeout(Duration::from_secs(2)).expect("hook called");
        assert_eq!(first, (KeyInput::Key(Key::F24), true));
        drop(hook);
    }

    /// Toggles Scroll Lock twice (restoring it) and checks that a thread which
    /// never processes keyboard messages sees the toggle state change.
    /// Ignored by default: it simulates input.
    #[test]
    #[ignore]
    fn background_thread_sees_toggle_state() {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
        let read = || std::thread::spawn(|| unsafe { GetKeyState(0x91) } & 1 != 0).join().unwrap();
        let before = read();
        unsafe { SendInput(&[key(0x91, false), key(0x91, true)], std::mem::size_of::<INPUT>() as i32) };
        std::thread::sleep(Duration::from_millis(100));
        let after = read();
        unsafe { SendInput(&[key(0x91, false), key(0x91, true)], std::mem::size_of::<INPUT>() as i32) };
        std::thread::sleep(Duration::from_millis(100));
        let restored = read();
        println!("scroll lock: before={before} after={after} restored={restored}");
        assert_ne!(before, after);
        assert_eq!(before, restored);
    }

    /// Same, but the state is read twice from the same long-lived thread.
    #[test]
    #[ignore]
    fn long_lived_thread_sees_toggle_state() {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let (res_tx, res_rx) = mpsc::channel::<bool>();
        std::thread::spawn(move || {
            while go_rx.recv().is_ok() {
                let _ = res_tx.send(unsafe { GetKeyState(0x91) } & 1 != 0);
            }
        });
        let read = || {
            go_tx.send(()).unwrap();
            res_rx.recv().unwrap()
        };
        let before = read();
        unsafe { SendInput(&[key(0x91, false), key(0x91, true)], std::mem::size_of::<INPUT>() as i32) };
        std::thread::sleep(Duration::from_millis(100));
        let after = read();
        unsafe { SendInput(&[key(0x91, false), key(0x91, true)], std::mem::size_of::<INPUT>() as i32) };
        std::thread::sleep(Duration::from_millis(100));
        let restored = read();
        println!("long-lived: before={before} after={after} restored={restored}");
        assert_ne!(before, after);
        assert_eq!(before, restored);
    }
}