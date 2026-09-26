//! Clipboard: setting text, and saving/restoring the previous content around
//! a paste (F-TXT-04).

use crate::wide::wide;
use std::cell::Cell;
use std::time::Duration;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE};
use windows::core::PCWSTR;

thread_local! {
    static OWNER: Cell<isize> = const { Cell::new(0) };
}

/// A hidden message-only window owning the clipboard (a null owner would make
/// `SetClipboardData` fail after `EmptyClipboard`). One per thread.
fn owner() -> Option<HWND> {
    let raw = OWNER.with(|o| o.get());
    if raw != 0 {
        return Some(HWND(raw as *mut _));
    }
    let class = wide("STATIC");
    // SAFETY: creates a message-only window of a system class.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
        .ok()?
    };
    OWNER.with(|o| o.set(hwnd.0 as isize));
    Some(hwnd)
}

/// Opens the clipboard, retrying while another program holds it.
fn open() -> bool {
    let owner = owner();
    for _ in 0..20 {
        // SAFETY: plain call; closed by the caller.
        if unsafe { OpenClipboard(owner) }.is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn close() {
    // SAFETY: the clipboard was opened by `open`.
    unsafe {
        let _ = CloseClipboard();
    }
}

fn global_from_bytes(bytes: &[u8]) -> Option<HGLOBAL> {
    // SAFETY: the block is allocated with the needed size and filled while locked.
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).ok()?;
        let ptr = GlobalLock(global) as *mut u8;
        if ptr.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(global);
        Some(global)
    }
}

fn put(format: u32, bytes: &[u8]) -> bool {
    match global_from_bytes(bytes) {
        // SAFETY: on success the system owns the memory block.
        Some(global) => unsafe { SetClipboardData(format, Some(HANDLE(global.0))).is_ok() },
        None => false,
    }
}

fn text_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16().chain(std::iter::once(0)).flat_map(|u| u.to_le_bytes()).collect()
}

fn register(name: &str) -> u32 {
    let name = wide(name);
    // SAFETY: the name outlives the call.
    unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) }
}

/// Replaces the clipboard content with a text. When `private` is set, the
/// content is marked so that clipboard history and cloud sync ignore it.
pub fn set_text(text: &str, private: bool) -> bool {
    if !open() {
        return false;
    }
    // SAFETY: the clipboard is open.
    let ok = unsafe { EmptyClipboard().is_ok() } && put(CF_UNICODETEXT.0 as u32, &text_bytes(text));
    if ok && private {
        put(register("ExcludeClipboardContentFromMonitorProcessing"), &[0]);
        put(register("CanIncludeInClipboardHistory"), &0u32.to_le_bytes());
        put(register("CanUploadToCloudClipboard"), &0u32.to_le_bytes());
    }
    close();
    ok
}

/// A copy of the clipboard content (formats stored in global memory; GDI
/// handles such as bitmaps are synthesised again by Windows from their
/// device-independent equivalents).
#[derive(Debug, Default)]
pub struct Backup(Vec<(u32, Vec<u8>)>);

/// Formats whose data is not a global memory block.
fn is_handle_format(format: u32) -> bool {
    matches!(format, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E) || (0x200..=0x3FF).contains(&format)
}

/// Saves the current clipboard content.
pub fn backup() -> Backup {
    let mut saved = Vec::new();
    if !open() {
        return Backup(saved);
    }
    // SAFETY: the clipboard is open; each block is only read while locked.
    unsafe {
        let mut format = EnumClipboardFormats(0);
        while format != 0 {
            if !is_handle_format(format)
                && let Ok(handle) = GetClipboardData(format)
                && !handle.is_invalid()
            {
                let global = HGLOBAL(handle.0);
                let size = GlobalSize(global);
                let ptr = GlobalLock(global) as *const u8;
                if !ptr.is_null() && size > 0 && size < 256 * 1024 * 1024 {
                    saved.push((format, std::slice::from_raw_parts(ptr, size).to_vec()));
                }
                if !ptr.is_null() {
                    let _ = GlobalUnlock(global);
                }
            }
            format = EnumClipboardFormats(format);
        }
    }
    close();
    Backup(saved)
}

/// Puts a saved content back into the clipboard.
pub fn restore(backup: &Backup) {
    if !open() {
        return;
    }
    // SAFETY: the clipboard is open.
    unsafe {
        let _ = EmptyClipboard();
    }
    for (format, bytes) in &backup.0 {
        put(*format, bytes);
    }
    close();
}

/// Text content of the clipboard, if any.
pub fn text() -> Option<String> {
    crate::system::clipboard_text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_formats() {
        assert!(is_handle_format(2));
        assert!(is_handle_format(0x250));
        assert!(!is_handle_format(13));
        assert_eq!(text_bytes("é"), vec![0xE9, 0, 0, 0]);
    }

    /// Modifies the clipboard (restoring it afterwards): ignored by default.
    #[test]
    #[ignore]
    fn set_backup_restore() {
        let before = backup();
        assert!(set_text("Declic ✓ test", true));
        assert_eq!(text().as_deref(), Some("Declic ✓ test"));
        restore(&before);
    }
}
