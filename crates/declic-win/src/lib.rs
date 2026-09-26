//! Windows platform layer of Declic.
//!
//! Everything that talks to Win32 lives here: the low-level keyboard hook,
//! text injection, launching targets, foreground-process detection, the
//! notification-area icon, autostart, file dialogs and a few system queries.
//! On other platforms the crate compiles to nothing.

#[cfg(windows)]
pub mod apps;
#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod dialogs;
#[cfg(windows)]
pub mod hook;
#[cfg(windows)]
pub mod icon;
#[cfg(windows)]
pub mod inject;
#[cfg(windows)]
pub mod keys;
#[cfg(windows)]
pub mod launch;
#[cfg(windows)]
pub mod mouse_hook;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod reserved;
#[cfg(windows)]
pub mod system;
#[cfg(windows)]
pub mod tip;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod watch;
#[cfg(windows)]
pub mod window;
#[cfg(windows)]
pub mod winfind;

#[cfg(windows)]
pub(crate) mod wide {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    /// Null-terminated UTF-16 copy of a string.
    pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Converts a (possibly null-terminated) UTF-16 buffer to a `String`.
    pub fn from_wide(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    /// Copies `s` into a fixed-size UTF-16 buffer, truncating and null-terminating.
    pub fn copy_to<const N: usize>(dst: &mut [u16; N], s: &str) {
        let mut i = 0;
        for unit in s.encode_utf16() {
            if i + 1 >= N {
                break;
            }
            dst[i] = unit;
            i += 1;
        }
        dst[i] = 0;
    }
}
