//! Installed programs (Start menu shortcuts) and file icons (F-ACT-03).

use crate::wide::wide;
use std::path::{Path, PathBuf};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HGDIOBJ,
    ReleaseDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};
use windows::core::PCWSTR;

/// A program found in the Start menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    /// The Start menu shortcut (opening it starts the program).
    pub path: PathBuf,
}

fn start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["ProgramData", "APPDATA"] {
        if let Some(base) = std::env::var_os(var) {
            dirs.push(PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"));
        }
    }
    dirs
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<InstalledApp>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth + 1, out);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(ext.as_str(), "lnk" | "url" | "appref-ms") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let lower = name.to_lowercase();
        if ["uninstall", "désinstaller", "desinstaller", "readme", "lisez-moi", "help", "aide"]
            .iter()
            .any(|w| lower.contains(w))
        {
            continue;
        }
        out.push(InstalledApp { name: name.to_string(), path });
    }
}

/// Programs listed in the Start menu (all users and current user), sorted
/// by name, without duplicates.
pub fn installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    for dir in start_menu_dirs() {
        walk(&dir, 0, &mut apps);
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

/// Icon of a file, program or shortcut as RGBA pixels (width, height, pixels).
pub fn file_icon_rgba(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let path_w = wide(path.as_os_str());
    let mut info = SHFILEINFOW::default();
    // SAFETY: the structure and path outlive the call; the icon, bitmaps and
    // device context obtained here are released before returning.
    unsafe {
        let ok = SHGetFileInfoW(
            PCWSTR(path_w.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if ok == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let icon = info.hIcon;
        let mut icon_info = ICONINFO::default();
        let result = if GetIconInfo(icon, &mut icon_info).is_ok() && !icon_info.hbmColor.is_invalid() {
            let mut bitmap = BITMAP::default();
            GetObjectW(HGDIOBJ(icon_info.hbmColor.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bitmap as *mut _ as *mut _));
            let (w, h) = (bitmap.bmWidth.max(0) as u32, bitmap.bmHeight.max(0) as u32);
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let hdc = GetDC(None);
            let lines = GetDIBits(hdc, icon_info.hbmColor, 0, h, Some(pixels.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
            ReleaseDC(None, hdc);
            if lines > 0 && w > 0 && h > 0 {
                let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
                for p in pixels.chunks_exact_mut(4) {
                    p.swap(0, 2);
                    if !has_alpha {
                        p[3] = 255;
                    }
                }
                Some((w, h, pixels))
            } else {
                None
            }
        } else {
            None
        };
        if !icon_info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(icon_info.hbmColor.0));
        }
        if !icon_info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(icon_info.hbmMask.0));
        }
        let _ = DestroyIcon(icon);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_and_icon_do_not_fail() {
        let apps = installed_apps();
        for pair in apps.windows(2) {
            assert!(pair[0].name.to_lowercase() <= pair[1].name.to_lowercase());
        }
        let exe = std::env::current_exe().unwrap();
        if let Some((w, h, px)) = file_icon_rgba(&exe) {
            assert_eq!(px.len() as u32, w * h * 4);
        }
    }
}
