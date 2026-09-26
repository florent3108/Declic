//! Icon in the notification area, its menu and its notifications.

use crate::wide::{copy_to, wide};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIIF_RESPECT_QUIET_TIME, NIIF_WARNING,
    NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow, HICON, MENU_ITEM_FLAGS, MF_CHECKED, MF_DEFAULT,
    MF_SEPARATOR, MF_STRING, MFT_RIGHTORDER, TPM_LAYOUTRTL, TRACK_POPUP_MENU_FLAGS,
    PostMessageW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL, WM_USER,
};
use windows::core::PCWSTR;

const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;

/// Interpretation of a tray callback message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Left click or keyboard selection: open the main window.
    Activate,
    /// Right click or context-menu key, at the given screen position.
    ContextMenu { x: i32, y: i32 },
    /// The user clicked a notification.
    NotificationClicked,
    Other,
}

/// Decodes the parameters of the tray callback message.
pub fn decode_event(wparam: usize, lparam: isize) -> TrayEvent {
    let event = (lparam as u32) & 0xFFFF;
    match event {
        NIN_SELECT | NIN_KEYSELECT | WM_LBUTTONUP | WM_LBUTTONDBLCLK => TrayEvent::Activate,
        WM_CONTEXTMENU => {
            let x = (wparam & 0xFFFF) as u16 as i16 as i32;
            let y = ((wparam >> 16) & 0xFFFF) as u16 as i16 as i32;
            TrayEvent::ContextMenu { x, y }
        }
        NIN_BALLOONUSERCLICK => TrayEvent::NotificationClicked,
        _ => TrayEvent::Other,
    }
}

/// A notification-area icon attached to a window.
pub struct TrayIcon {
    hwnd: HWND,
    id: u32,
    callback: u32,
}

impl TrayIcon {
    fn data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: self.id,
            uCallbackMessage: self.callback,
            ..Default::default()
        }
    }

    /// Adds the icon. `callback` is the message posted to `hwnd` on user interaction.
    pub fn add(hwnd: HWND, id: u32, callback: u32, icon: HICON, tip: &str) -> Option<TrayIcon> {
        let tray = TrayIcon { hwnd, id, callback };
        let mut data = tray.data();
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.hIcon = icon;
        copy_to(&mut data.szTip, tip);
        // SAFETY: the structure is fully initialised.
        unsafe {
            if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                return None;
            }
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
        }
        Some(tray)
    }

    /// Re-adds the icon, e.g. after Explorer restarted.
    pub fn readd(&self, icon: HICON, tip: &str) -> bool {
        TrayIcon::add(self.hwnd, self.id, self.callback, icon, tip).is_some()
    }

    /// Changes the icon and its tooltip.
    pub fn update(&self, icon: HICON, tip: &str) {
        let mut data = self.data();
        data.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.hIcon = icon;
        copy_to(&mut data.szTip, tip);
        // SAFETY: the structure is fully initialised.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    /// Shows a non-blocking notification.
    pub fn notify(&self, title: &str, text: &str, warning: bool) {
        let mut data = self.data();
        data.uFlags = NIF_INFO;
        copy_to(&mut data.szInfoTitle, title);
        copy_to(&mut data.szInfo, text);
        data.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO } | NIIF_RESPECT_QUIET_TIME;
        // SAFETY: the structure is fully initialised.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let data = self.data();
        // SAFETY: removing our own icon.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
    }
}

/// An entry of the tray menu.
#[derive(Debug, Clone)]
pub enum MenuEntry {
    Item { id: u32, label: String, checked: bool, default: bool },
    Separator,
}

/// Shows a popup menu at the given position (or at the cursor) and returns
/// the chosen command id. `rtl` lays the menu out from right to left (Arabic,
/// Hebrew): items read right to left, check marks on the right.
pub fn show_menu(hwnd: HWND, entries: &[MenuEntry], at: Option<(i32, i32)>, rtl: bool) -> Option<u32> {
    // SAFETY: the menu is created, used and destroyed here; label buffers
    // outlive the AppendMenuW calls (the menu copies them).
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for entry in entries {
            match entry {
                MenuEntry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                MenuEntry::Item { id, label, checked, default } => {
                    let label = wide(label);
                    let mut flags = MF_STRING;
                    if rtl {
                        // MFT_RIGHTORDER: right-to-left reading order.
                        flags |= MENU_ITEM_FLAGS(MFT_RIGHTORDER.0);
                    }
                    if *checked {
                        flags |= MF_CHECKED;
                    }
                    if *default {
                        flags |= MF_DEFAULT;
                    }
                    let _ = AppendMenuW(menu, flags, *id as usize, PCWSTR(label.as_ptr()));
                }
            }
        }
        let (x, y) = match at {
            Some(p) => p,
            None => {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                (p.x, p.y)
            }
        };
        // Required so that the menu closes when clicking elsewhere.
        let previous = GetForegroundWindow();
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | if rtl { TPM_LAYOUTRTL } else { TRACK_POPUP_MENU_FLAGS(0) },
            x,
            y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        // The hidden window must not keep the keyboard focus: give it back.
        if GetForegroundWindow() == hwnd && !previous.is_invalid() {
            let _ = SetForegroundWindow(previous);
        }
        (cmd.0 > 0).then_some(cmd.0 as u32)
    }
}
