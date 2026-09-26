//! Foreground program detection and process information (F-CND-01).

use crate::wide::from_wide;
use std::collections::BTreeSet;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GWL_EXSTYLE, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowThreadProcessId, GW_OWNER, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR};

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by us and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open_process(pid: u32) -> Option<Handle> {
    // SAFETY: plain query; the handle is closed by `Handle`.
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().map(Handle) }
}

/// Full path of a process image.
pub fn process_path(pid: u32) -> Option<String> {
    let process = open_process(pid)?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: the buffer and length are valid.
    unsafe { QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).ok()? };
    Some(from_wide(&buf[..len as usize]))
}

/// Lowercase executable name of a process, e.g. `chrome.exe`.
pub fn process_name(pid: u32) -> Option<String> {
    let path = process_path(pid)?;
    path.rsplit('\\').next().map(|n| n.to_lowercase())
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: plain query.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

unsafe extern "system" fn find_real_app(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam points to the (host pid, found pid) pair owned by the caller.
    let data = unsafe { &mut *(lparam.0 as *mut (u32, u32)) };
    let pid = window_pid(hwnd);
    if pid != 0 && pid != data.0 {
        data.1 = pid;
        return BOOL(0);
    }
    BOOL(1)
}

/// Process id of the program owning the foreground window. For Store apps
/// hosted by `ApplicationFrameHost.exe`, the hosted app is returned.
pub fn foreground_pid() -> Option<u32> {
    // SAFETY: plain queries; the callback only writes to the local pair.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let pid = window_pid(hwnd);
        if pid == 0 {
            return None;
        }
        if process_name(pid).as_deref() == Some("applicationframehost.exe") {
            let mut data = (pid, 0u32);
            let _ = EnumChildWindows(Some(hwnd), Some(find_real_app), LPARAM(&mut data as *mut _ as isize));
            if data.1 != 0 {
                return Some(data.1);
            }
        }
        Some(pid)
    }
}

/// Lowercase executable name of the foreground program.
pub fn foreground_program() -> Option<String> {
    foreground_pid().and_then(process_name)
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    // SAFETY: token handle closed by `Handle`; the output buffer is sized correctly.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let token = Handle(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .ok()?;
        Some(elevation.TokenIsElevated != 0)
    }
}

/// Whether Declic itself runs with administrator rights.
pub fn current_process_elevated() -> bool {
    // SAFETY: the pseudo-handle of the current process needs no closing.
    token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false)
}

/// Whether the foreground program runs with higher rights than Declic, in
/// which case Windows silently discards simulated keystrokes sent to it.
pub fn foreground_is_elevated_above_us() -> bool {
    if current_process_elevated() {
        return false;
    }
    let Some(pid) = foreground_pid() else { return false };
    match open_process(pid) {
        // Access to the token of an elevated process is denied to us.
        Some(process) => token_elevated(process.0).unwrap_or(true),
        None => false,
    }
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam points to the pid set owned by the caller.
    let pids = unsafe { &mut *(lparam.0 as *mut BTreeSet<u32>) };
    // SAFETY: plain queries on a window handle given by EnumWindows.
    unsafe {
        let visible = IsWindowVisible(hwnd).as_bool();
        let tool = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0;
        let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid());
        if visible && !tool && !owned && GetWindowTextLengthW(hwnd) > 0 {
            pids.insert(window_pid(hwnd));
        }
    }
    BOOL(1)
}

/// Executable names of the programs that currently have a visible window
/// (excluding Declic itself), sorted and deduplicated.
pub fn running_programs() -> Vec<String> {
    let mut pids = BTreeSet::new();
    // SAFETY: the callback only writes to the local set.
    unsafe {
        let _ = EnumWindows(Some(collect_window), LPARAM(&mut pids as *mut _ as isize));
    }
    // SAFETY: plain query.
    let own = unsafe { GetCurrentProcessId() };
    let names: BTreeSet<String> = pids
        .into_iter()
        .filter(|&pid| pid != own && pid != 0)
        .filter_map(process_name)
        .filter(|n| n != "applicationframehost.exe" && n != "textinputhost.exe")
        .collect();
    names.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_name_is_found() {
        // SAFETY: plain query.
        let pid = unsafe { GetCurrentProcessId() };
        let name = process_name(pid).expect("own process name");
        assert!(name.ends_with(".exe"));
        assert_eq!(name, name.to_lowercase());
    }
}
