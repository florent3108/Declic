//! System queries: language, theme, accent colour, time, clipboard, variables.

use crate::wide::{from_wide, wide};
use declic_core::vars::{LocalDateTime, VarProvider};
use std::path::PathBuf;
use windows::Win32::Foundation::{HGLOBAL, SYSTEMTIME};
use windows::Win32::Globalization::{
    DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, GetUserDefaultUILanguage, LCIDToLocaleName, TIME_NOSECONDS,
};
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::PCWSTR;

/// Reads a DWORD value under HKEY_CURRENT_USER.
pub(crate) fn read_dword(root: HKEY, subkey: &str, value: &str) -> Option<u32> {
    let subkey = wide(subkey);
    let value = wide(value);
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: the output buffer is a u32 of the announced size.
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(data)
}

/// Language of the Windows user interface, as a locale name such as `fr-FR`.
pub fn ui_language() -> String {
    // SAFETY: plain queries with a local buffer.
    unsafe {
        let langid = GetUserDefaultUILanguage();
        let mut buf = [0u16; 85];
        let len = LCIDToLocaleName(langid as u32, Some(&mut buf), 0);
        if len > 0 { from_wide(&buf) } else { "en-US".to_string() }
    }
}

/// Whether applications should use a dark theme (Windows "app mode").
pub fn apps_use_dark_theme() -> bool {
    read_dword(HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme")
        .map(|v| v == 0)
        .unwrap_or(false)
}

/// Accent colour chosen by the user in Windows settings, as (r, g, b).
pub fn accent_color() -> Option<(u8, u8, u8)> {
    // Stored as 0xAABBGGRR.
    let abgr = read_dword(HKEY_CURRENT_USER, r"Software\Microsoft\Windows\DWM", "AccentColor")?;
    Some(((abgr & 0xFF) as u8, ((abgr >> 8) & 0xFF) as u8, ((abgr >> 16) & 0xFF) as u8))
}

/// Whether the user asked Windows to show animations.
pub fn animations_enabled() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW};
    let mut enabled = windows::core::BOOL(1);
    // SAFETY: the output is a BOOL as documented for this parameter.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut enabled as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    enabled.as_bool()
}

fn to_local(st: &SYSTEMTIME) -> LocalDateTime {
    LocalDateTime {
        year: st.wYear as i32,
        month: st.wMonth as u8,
        day: st.wDay as u8,
        hour: st.wHour as u8,
        minute: st.wMinute as u8,
        second: st.wSecond as u8,
        // SYSTEMTIME: 0 = Sunday; LocalDateTime: 0 = Monday.
        weekday: ((st.wDayOfWeek + 6) % 7) as u8,
    }
}

fn to_systemtime(dt: &LocalDateTime) -> SYSTEMTIME {
    SYSTEMTIME {
        wYear: dt.year as u16,
        wMonth: dt.month as u16,
        wDayOfWeek: ((dt.weekday + 1) % 7) as u16,
        wDay: dt.day as u16,
        wHour: dt.hour as u16,
        wMinute: dt.minute as u16,
        wSecond: dt.second as u16,
        wMilliseconds: 0,
    }
}

/// Current local date and time.
pub fn local_now() -> LocalDateTime {
    // SAFETY: plain query.
    to_local(&unsafe { GetLocalTime() })
}

/// Date in the user's regional short format.
pub fn format_short_date(dt: &LocalDateTime) -> Option<String> {
    let st = to_systemtime(dt);
    let mut buf = [0u16; 128];
    // SAFETY: buffers live for the duration of the call.
    let len = unsafe { GetDateFormatEx(PCWSTR::null(), DATE_SHORTDATE, Some(&st), PCWSTR::null(), Some(&mut buf), PCWSTR::null()) };
    (len > 0).then(|| from_wide(&buf))
}

/// Time in the user's regional short format (without seconds).
pub fn format_short_time(dt: &LocalDateTime) -> Option<String> {
    let st = to_systemtime(dt);
    let mut buf = [0u16; 128];
    // SAFETY: buffers live for the duration of the call.
    let len = unsafe { GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, Some(&st), PCWSTR::null(), Some(&mut buf)) };
    (len > 0).then(|| from_wide(&buf))
}

/// Text content of the clipboard, if any.
pub fn clipboard_text() -> Option<String> {
    // The clipboard may be briefly locked by another program.
    let mut opened = false;
    for _ in 0..10 {
        // SAFETY: plain call; closed below.
        if unsafe { OpenClipboard(None) }.is_ok() {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if !opened {
        return None;
    }
    // SAFETY: the clipboard is open; the global memory is locked while read.
    let text = unsafe {
        match GetClipboardData(CF_UNICODETEXT.0 as u32) {
            Ok(handle) if !handle.is_invalid() => {
                let global = HGLOBAL(handle.0);
                let ptr = GlobalLock(global) as *const u16;
                if ptr.is_null() {
                    None
                } else {
                    let mut len = 0usize;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                    let _ = GlobalUnlock(global);
                    Some(text)
                }
            }
            _ => None,
        }
    };
    // SAFETY: the clipboard was opened above.
    unsafe {
        let _ = CloseClipboard();
    }
    text
}

/// Variable provider backed by the real system.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemVars;

impl VarProvider for SystemVars {
    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn clipboard_text(&self) -> Option<String> {
        clipboard_text()
    }

    fn now(&self) -> LocalDateTime {
        local_now()
    }

    fn format_short_date(&self, dt: &LocalDateTime) -> Option<String> {
        format_short_date(dt)
    }

    fn format_short_time(&self, dt: &LocalDateTime) -> Option<String> {
        format_short_time(dt)
    }
}

/// Directory containing the running executable.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

/// Per-user roaming application data directory (`%APPDATA%`).
pub fn app_data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

/// List separator of the user's regional settings (`;` in French, `,` in English).
pub fn list_separator() -> char {
    let mut buf = [0u16; 8];
    // SAFETY: the buffer outlives the call. 0x0C is LOCALE_SLIST.
    let len = unsafe { windows::Win32::Globalization::GetLocaleInfoEx(PCWSTR::null(), 0x0C, Some(&mut buf)) };
    if len > 0 { from_wide(&buf).chars().next().unwrap_or(';') } else { ';' }
}

/// Local date and time of a Unix timestamp, in the user's short formats.
pub fn format_unix_time(secs: u64) -> Option<String> {
    format_unix_time_in(secs, None)
}

/// Name of the user's regional format ("Regional format" in Windows
/// settings), such as `fr-FR`.
pub fn user_locale() -> String {
    let mut buf = [0u16; 85];
    // SAFETY: the buffer outlives the call.
    let len = unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut buf) };
    if len > 0 { from_wide(&buf) } else { String::new() }
}

/// Local date and time of a Unix timestamp in the short formats of a locale
/// such as `de-DE` (`None`: the user's own regional settings).
pub fn format_unix_time_in(secs: u64, locale: Option<&str>) -> Option<String> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    let ticks = (secs + 11_644_473_600) * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // SAFETY: plain conversions between valid structures.
    unsafe {
        FileTimeToSystemTime(&ft, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
    }
    let name = locale.map(wide);
    let locale = name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null());
    let mut date = [0u16; 128];
    let mut time = [0u16; 128];
    // SAFETY: the buffers and the locale name live for the duration of the calls.
    let (date_len, time_len) = unsafe {
        (
            GetDateFormatEx(locale, DATE_SHORTDATE, Some(&local), PCWSTR::null(), Some(&mut date), PCWSTR::null()),
            GetTimeFormatEx(locale, TIME_NOSECONDS, Some(&local), PCWSTR::null(), Some(&mut time)),
        )
    };
    (date_len > 0 && time_len > 0).then(|| format!("{} {}", from_wide(&date), from_wide(&time)))
}

/// Digit-group separator and grouping (`LOCALE_SGROUPING`, e.g. `3;0`) of a
/// locale (`None`: the user's own regional settings).
pub fn number_grouping(locale: Option<&str>) -> (String, String) {
    let name = locale.map(wide);
    let locale = name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null());
    let query = |kind: u32| {
        let mut buf = [0u16; 16];
        // SAFETY: the buffer and the locale name outlive the call.
        let len = unsafe { windows::Win32::Globalization::GetLocaleInfoEx(locale, kind, Some(&mut buf)) };
        if len > 0 { from_wide(&buf) } else { String::new() }
    };
    // 0x0F is LOCALE_STHOUSAND, 0x10 LOCALE_SGROUPING.
    (query(0x0F), query(0x10))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_conversion_round_trip() {
        let now = local_now();
        assert!((1..=12).contains(&now.month));
        assert!(now.weekday < 7);
        let st = to_systemtime(&now);
        assert_eq!(to_local(&st), now);
        assert!(format_short_date(&now).is_some_and(|s| !s.is_empty()));
        assert!(format_short_time(&now).is_some_and(|s| !s.is_empty()));
    }

    #[test]
    fn dates_and_numbers_follow_the_locale() {
        // 2026-01-02 03:04:05 UTC: the day and month are unambiguous.
        let secs = 1_767_323_045;
        let de = format_unix_time_in(secs, Some("de-DE")).unwrap();
        assert!(de.contains("02.01.2026"), "{de}");
        let us = format_unix_time_in(secs, Some("en-US")).unwrap();
        assert!(us.contains("1/2/2026"), "{us}");
        assert!(format_unix_time(secs).is_some());
        assert_eq!(number_grouping(Some("en-US")), (",".to_string(), "3;0".to_string()));
        assert_eq!(number_grouping(Some("hi-IN")).1, "3;2;0");
        assert!(!user_locale().is_empty());
    }

    #[test]
    fn language_is_a_locale_name() {
        let lang = ui_language();
        assert!(lang.len() >= 2, "{lang}");
    }
}
