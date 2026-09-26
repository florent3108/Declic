//! Opening programs, documents, folders, URLs and protocols (F-ACT-01/02).

use crate::wide::{from_wide, wide};
use declic_core::WindowState;
use std::path::Path;
use windows::Win32::Foundation::{ERROR_CANCELLED, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, GetLastError};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL};
use windows::core::{Interface, PCWSTR};

/// Why a target could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// The file, folder or program does not exist.
    NotFound,
    /// The user declined the elevation prompt.
    Cancelled,
    /// Any other failure, with the system error code.
    Failed(u32),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::NotFound => f.write_str("target not found"),
            LaunchError::Cancelled => f.write_str("cancelled by the user"),
            LaunchError::Failed(code) => write!(f, "system error {code}"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// Initialises COM on the current thread (single-threaded apartment), as
/// required by the shell functions. Safe to call several times.
pub fn init_com() {
    // SAFETY: standard COM initialisation; failures (already initialised) are harmless.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }
}

/// Whether the target is an explicit file-system path (as opposed to a
/// program name found through the PATH, a URL or a protocol).
pub fn is_explicit_path(target: &str) -> bool {
    let bytes = target.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
    drive || target.starts_with("\\\\")
}

/// Opens a target with the shell, like a double-click would.
pub fn open(target: &str, arguments: &str, working_dir: &str, window: WindowState, as_admin: bool) -> Result<(), LaunchError> {
    let target = target.trim().trim_matches('"');
    if target.is_empty() {
        return Err(LaunchError::NotFound);
    }
    if is_explicit_path(target) && !Path::new(target).exists() {
        return Err(LaunchError::NotFound);
    }
    // Default working directory: the folder containing the program or document.
    let default_dir;
    let working_dir = if working_dir.trim().is_empty() && is_explicit_path(target) && Path::new(target).is_file() {
        default_dir = Path::new(target).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        default_dir.as_str()
    } else {
        working_dir.trim()
    };
    let file = wide(target);
    let params = wide(arguments);
    let dir = wide(working_dir);
    let verb = wide("runas");
    let show = match window {
        WindowState::Normal => SW_SHOWNORMAL,
        WindowState::Minimized => SW_SHOWMINNOACTIVE,
        WindowState::Maximized => SW_SHOWMAXIMIZED,
    };
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: if as_admin { PCWSTR(verb.as_ptr()) } else { PCWSTR::null() },
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: if arguments.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
        lpDirectory: if working_dir.is_empty() { PCWSTR::null() } else { PCWSTR(dir.as_ptr()) },
        nShow: show.0,
        ..Default::default()
    };
    // SAFETY: all strings live until the end of the call.
    match unsafe { ShellExecuteExW(&mut info) } {
        Ok(()) => Ok(()),
        Err(_) => {
            // SAFETY: plain query function.
            let code = unsafe { GetLastError() };
            Err(match code {
                c if c == ERROR_FILE_NOT_FOUND || c == ERROR_PATH_NOT_FOUND => LaunchError::NotFound,
                c if c == ERROR_CANCELLED => LaunchError::Cancelled,
                c => LaunchError::Failed(c.0),
            })
        }
    }
}

/// Resolves a Windows shortcut (`.lnk`) to its target path, for display.
pub fn resolve_shortcut(path: &Path) -> Option<String> {
    init_com();
    // SAFETY: COM calls on interfaces created here; buffers outlive the calls.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        let path_w = wide(path.as_os_str());
        file.Load(PCWSTR(path_w.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let target = from_wide(&buf);
        (!target.is_empty()).then_some(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_paths() {
        assert!(is_explicit_path(r"C:\Windows\notepad.exe"));
        assert!(is_explicit_path(r"\\server\share"));
        assert!(!is_explicit_path("notepad.exe"));
        assert!(!is_explicit_path("https://example.org"));
        assert!(!is_explicit_path("ms-settings:display"));
    }

    #[test]
    fn missing_explicit_path_is_reported_without_side_effect() {
        assert_eq!(
            open(r"C:\this\path\does\not\exist\declic.exe", "", "", WindowState::Normal, false),
            Err(LaunchError::NotFound)
        );
    }
}
