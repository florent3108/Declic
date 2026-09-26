//! Starting Declic with Windows, per user, without administrator rights.

use crate::wide::{from_wide, wide};
use std::io;
use std::path::Path;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::PCWSTR;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Declic";

/// Command line registered for automatic start.
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" --background", exe.display())
}

/// The command line currently registered, if any.
pub fn registered_command() -> Option<String> {
    let key = wide(RUN_KEY);
    let name = wide(VALUE_NAME);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: the buffer and its size in bytes are valid.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    status.is_ok().then(|| from_wide(&buf))
}

/// Whether Declic starts with Windows.
pub fn is_enabled() -> bool {
    registered_command().is_some()
}

/// Enables or disables the automatic start of `exe` for the current user.
pub fn set_enabled(enabled: bool, exe: &Path) -> io::Result<()> {
    let key = wide(RUN_KEY);
    let name = wide(VALUE_NAME);
    // SAFETY: strings live until the end of the calls.
    let status = unsafe {
        if enabled {
            let data = wide(command_line(exe));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                REG_SZ.0,
                Some(data.as_ptr() as *const _),
                (data.len() * 2) as u32,
            )
        } else {
            let status = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()));
            if status == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            status
        }
    };
    if status.is_ok() { Ok(()) } else { Err(io::Error::from_raw_os_error(status.0 as i32)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_is_quoted() {
        assert_eq!(command_line(Path::new(r"C:\Apps\Declic\declic.exe")), r#""C:\Apps\Declic\declic.exe" --background"#);
    }
}
