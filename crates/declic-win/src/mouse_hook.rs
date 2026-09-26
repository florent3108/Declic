//! Low-level mouse hook on its own thread (used by the eyedropper helper).

use crate::hook::Decision;
use crate::inject::INJECTION_TAG;
use std::cell::RefCell;
use std::io;
use std::sync::mpsc;
use std::thread::JoinHandle;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_USER,
};

/// What happened with the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Move,
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
    MiddleDown,
    MiddleUp,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    pub action: MouseAction,
    /// Position on the virtual screen (physical pixels).
    pub x: i32,
    pub y: i32,
}

type Handler = Box<dyn FnMut(&MouseEvent) -> Decision>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if info.dwExtraInfo != INJECTION_TAG {
            let action = match wparam.0 as u32 {
                WM_MOUSEMOVE => MouseAction::Move,
                WM_LBUTTONDOWN => MouseAction::LeftDown,
                WM_LBUTTONUP => MouseAction::LeftUp,
                WM_RBUTTONDOWN => MouseAction::RightDown,
                WM_RBUTTONUP => MouseAction::RightUp,
                WM_MBUTTONDOWN => MouseAction::MiddleDown,
                WM_MBUTTONUP => MouseAction::MiddleUp,
                _ => MouseAction::Other,
            };
            let event = MouseEvent { action, x: info.pt.x, y: info.pt.y };
            let decision = HANDLER.with(|h| match h.try_borrow_mut() {
                Ok(mut guard) => guard.as_mut().map(|f| f(&event)).unwrap_or(Decision::Pass),
                Err(_) => Decision::Pass,
            });
            if decision == Decision::Swallow {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: forwarding the unchanged parameters.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// A running mouse hook; dropping it removes the hook.
pub struct MouseHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl MouseHook {
    pub fn start<F>(handler: F) -> io::Result<MouseHook>
    where
        F: FnMut(&MouseEvent) -> Decision + Send + 'static,
    {
        let (ready_tx, ready_rx) = mpsc::channel::<io::Result<u32>>();
        let thread = std::thread::Builder::new().name("declic-mouse-hook".into()).spawn(move || {
            // SAFETY: standard hook installation and message loop on this thread.
            unsafe {
                let mut msg = MSG::default();
                let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
                HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
                let module = GetModuleHandleW(None).ok().map(|m| m.into());
                let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0) {
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
                HANDLER.with(|h| *h.borrow_mut() = None);
            }
        })?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(MouseHook { thread_id, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(io::Error::other("mouse hook thread ended unexpectedly")),
        }
    }
}

impl Drop for MouseHook {
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
