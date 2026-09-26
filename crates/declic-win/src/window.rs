//! Hidden message window, message loop and inter-process window helpers.

use crate::wide::wide;
use std::cell::RefCell;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, FindWindowW,
    GetMessageW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, MSG, PostMessageW, PostQuitMessage,
    RegisterClassExW, RegisterWindowMessageW, SW_RESTORE, SetForegroundWindow, ShowWindow, TranslateMessage,
    WM_CLOSE, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR};

type Handler = Box<dyn FnMut(HWND, u32, WPARAM, LPARAM) -> Option<LRESULT>>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // The handler is taken out while it runs, so that re-entrant messages
    // (e.g. during a popup menu) fall back to the default procedure.
    let handler = HANDLER.with(|h| h.borrow_mut().take());
    if let Some(mut handler) = handler {
        let result = handler(hwnd, msg, wparam, lparam);
        HANDLER.with(|h| {
            let mut slot = h.borrow_mut();
            if slot.is_none() {
                *slot = Some(handler);
            }
        });
        if let Some(result) = result {
            return result;
        }
    }
    // SAFETY: default processing of the unchanged message.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Creates a hidden top-level window of the given class on the current
/// thread, whose messages are handled by `handler` (return `None` for the
/// default processing). Only one such window per thread is supported.
pub fn create_hidden_window<F>(class_name: &str, handler: F) -> windows::core::Result<HWND>
where
    F: FnMut(HWND, u32, WPARAM, LPARAM) -> Option<LRESULT> + 'static,
{
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
    let class = wide(class_name);
    // SAFETY: the class name outlives both calls; the window procedure is a
    // valid `extern "system"` function.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

/// Runs the message loop of the current thread until `WM_QUIT`.
pub fn run_message_loop() {
    let mut msg = MSG::default();
    // SAFETY: standard message loop.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Ends the message loop of the current thread.
pub fn quit_message_loop() {
    // SAFETY: plain call.
    unsafe { PostQuitMessage(0) };
}

/// Finds a top-level window by class name; returns its raw handle.
pub fn find_window(class_name: &str) -> Option<isize> {
    let class = wide(class_name);
    // SAFETY: the class name outlives the call.
    unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()).ok().map(|h| h.0 as isize) }
}

/// Posts a message to a window given by its raw handle.
pub fn post_message(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> bool {
    // SAFETY: posting to a possibly stale handle is harmless (it fails).
    unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(wparam), LPARAM(lparam)).is_ok() }
}

/// Registers (or retrieves) a system-wide message identifier.
pub fn register_message(name: &str) -> u32 {
    let name = wide(name);
    // SAFETY: the string outlives the call.
    unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) }
}

/// Makes the process DPI-aware (sharp tray icon on high-DPI screens).
pub fn set_dpi_aware() {
    // SAFETY: plain call; failure (already set) is harmless.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Lets another process bring its window to the foreground.
pub fn allow_foreground(pid: Option<u32>) {
    // SAFETY: plain call. u32::MAX is ASFW_ANY.
    unsafe {
        let _ = AllowSetForegroundWindow(pid.unwrap_or(u32::MAX));
    }
}

struct Search {
    pid: u32,
    windows: Vec<HWND>,
}

unsafe extern "system" fn collect_process_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam points to the `Search` owned by the caller.
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    let mut pid = 0u32;
    // SAFETY: plain queries.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == search.pid && IsWindowVisible(hwnd).as_bool() {
            search.windows.push(hwnd);
        }
    }
    BOOL(1)
}

fn process_windows(pid: u32) -> Vec<HWND> {
    let mut search = Search { pid, windows: Vec::new() };
    // SAFETY: the callback only writes to the local search state.
    unsafe {
        let _ = EnumWindows(Some(collect_process_windows), LPARAM(&mut search as *mut _ as isize));
    }
    search.windows
}

/// Restores and activates the main visible window of a process.
pub fn focus_process_window(pid: u32) -> bool {
    let Some(&hwnd) = process_windows(pid).first() else { return false };
    // SAFETY: plain calls on a valid window handle.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}

/// Asks the visible windows of a process to close.
pub fn close_process_windows(pid: u32) {
    for hwnd in process_windows(pid) {
        // SAFETY: posting WM_CLOSE to a window of another process.
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// A named mutex held for the lifetime of the process instance.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    /// Acquires the named instance, or returns `None` if another process holds it.
    pub fn acquire(name: &str) -> Option<SingleInstance> {
        let name = wide(name);
        // SAFETY: the name outlives the call; the handle is closed on drop.
        unsafe {
            let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr())).ok()?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return None;
            }
            Some(SingleInstance(handle))
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: closing our own handle once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Kinds of data sent to the background service with `WM_COPYDATA`.
pub mod copydata {
    /// Run an action once (the payload is the action in TOML), e.g. the
    /// editor's Test button.
    pub const RUN_ACTION: usize = 1;
    /// Reset usage statistics (payload: a shortcut id, or empty for all).
    pub const RESET_STATS: usize = 2;
}

/// Sends a UTF-8 payload to another window with `WM_COPYDATA` (waiting at
/// most one second).
pub fn send_copydata(hwnd: isize, kind: usize, payload: &str) -> bool {
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::UI::WindowsAndMessaging::{SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_COPYDATA};
    let data = COPYDATASTRUCT { dwData: kind, cbData: payload.len() as u32, lpData: payload.as_ptr() as *mut _ };
    let mut result = 0usize;
    // SAFETY: the structure and payload outlive the synchronous call.
    let ok = unsafe {
        SendMessageTimeoutW(
            HWND(hwnd as *mut _),
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&data as *const _ as isize),
            SMTO_ABORTIFHUNG,
            1000,
            Some(&mut result),
        )
    };
    ok.0 != 0
}

/// Decodes a `WM_COPYDATA` message: (kind, UTF-8 payload).
///
/// # Safety
/// `lparam` must be the parameter of a `WM_COPYDATA` message being handled.
pub unsafe fn read_copydata(lparam: LPARAM) -> Option<(usize, String)> {
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    // SAFETY: guaranteed by the caller.
    let data = unsafe { (lparam.0 as *const COPYDATASTRUCT).as_ref()? };
    let bytes = if data.cbData == 0 || data.lpData.is_null() {
        &[][..]
    } else {
        // SAFETY: the system keeps the buffer valid during the message.
        unsafe { std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize) }
    };
    Some((data.dwData, String::from_utf8_lossy(bytes).into_owned()))
}