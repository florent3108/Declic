//! Finding, describing and activating top-level windows (macro steps
//! "activate a window" and "activate or launch", eyedropper).

use crate::process::process_name;
use crate::wide::from_wide;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GA_ROOT, GW_OWNER, GWL_EXSTYLE, GetAncestor, GetForegroundWindow, GetWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE,
    SetForegroundWindow, ShowWindow, WS_EX_TOOLWINDOW, WindowFromPoint,
};
use windows::core::BOOL;

/// A top-level window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    /// Raw window handle.
    pub hwnd: isize,
    pub title: String,
    /// Lowercase executable name of the owning process.
    pub program: Option<String>,
    /// Window rectangle on the virtual screen: left, top, right, bottom.
    pub rect: (i32, i32, i32, i32),
}

fn handle(raw: isize) -> HWND {
    HWND(raw as *mut _)
}

fn title_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: the buffer outlives the call.
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    from_wide(&buf[..len.max(0) as usize])
}

fn rect_of(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut r = RECT::default();
    // SAFETY: plain query.
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    (r.left, r.top, r.right, r.bottom)
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: plain query.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn describe(hwnd: HWND) -> WindowInfo {
    WindowInfo { hwnd: hwnd.0 as isize, title: title_of(hwnd), program: process_name(pid_of(hwnd)), rect: rect_of(hwnd) }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: the output is a u32 as documented for DWMWA_CLOAKED.
    let ok = unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, std::mem::size_of::<u32>() as u32)
    };
    ok.is_ok() && cloaked != 0
}

/// Whether a window is a "real" application window, as shown in Alt+Tab.
fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: plain queries on a handle given by EnumWindows.
    unsafe {
        IsWindowVisible(hwnd).as_bool()
            && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0
            && GetWindow(hwnd, GW_OWNER).map(|o| o.is_invalid()).unwrap_or(true)
            && !is_cloaked(hwnd)
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam points to the vector owned by `app_windows`.
    let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    if is_app_window(hwnd) {
        list.push(hwnd);
    }
    BOOL(1)
}

/// Application windows, from the top of the Z order to the bottom.
pub fn app_windows() -> Vec<WindowInfo> {
    let mut handles: Vec<HWND> = Vec::new();
    // SAFETY: the callback only writes to the local vector.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut handles as *mut _ as isize));
    }
    handles.into_iter().map(describe).filter(|w| !w.title.is_empty()).collect()
}

/// The topmost window matching a title pattern (wildcards `*`, `?`) and/or a program.
pub fn find(title_pattern: &str, program: &str) -> Option<WindowInfo> {
    app_windows()
        .into_iter()
        .find(|w| declic_core::window_matches(title_pattern, program, &w.title, w.program.as_deref()))
}

/// Waits (polling) until a matching window exists, at most `timeout`.
/// `stop` is checked regularly and ends the wait early when it returns true.
pub fn wait_for(title_pattern: &str, program: &str, timeout: Duration, stop: &dyn Fn() -> bool) -> Option<WindowInfo> {
    let start = Instant::now();
    loop {
        if let Some(w) = find(title_pattern, program) {
            return Some(w);
        }
        if start.elapsed() >= timeout || stop() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The foreground window.
pub fn foreground() -> Option<WindowInfo> {
    // SAFETY: plain query.
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then(|| describe(hwnd))
}

/// The top-level window under a screen position.
pub fn window_at(x: i32, y: i32) -> Option<WindowInfo> {
    // SAFETY: plain queries.
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.is_invalid() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        Some(describe(if root.is_invalid() { hwnd } else { root }))
    }
}

/// Brings a window to the foreground (restoring it if minimised).
/// Windows restricts which process may change the foreground window; if a
/// plain request is refused, the input of the current foreground thread is
/// attached briefly, then a neutral Alt press is used as a last resort.
pub fn activate(raw: isize) -> bool {
    let hwnd = handle(raw);
    // SAFETY: plain window calls; thread input is detached again right away.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd {
            return true;
        }
        let _ = SetForegroundWindow(hwnd);
        if GetForegroundWindow() == hwnd {
            return true;
        }
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let own_thread = GetCurrentThreadId();
        if foreground_thread != 0 && foreground_thread != own_thread {
            let _ = AttachThreadInput(own_thread, foreground_thread, true);
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(own_thread, foreground_thread, false);
        }
        if GetForegroundWindow() == hwnd {
            return true;
        }
        crate::inject::alt_nudge(|| {
            let _ = SetForegroundWindow(hwnd);
        });
        std::thread::sleep(Duration::from_millis(30));
        GetForegroundWindow() == hwnd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_does_not_fail() {
        // There may be no window in a headless session; only check consistency.
        for w in app_windows() {
            assert!(!w.title.is_empty());
            assert!(w.rect.2 >= w.rect.0);
        }
        assert!(find("", "").is_none(), "no criteria never matches");
    }
}
