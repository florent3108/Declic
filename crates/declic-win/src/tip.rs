//! A small always-on-top label that follows the pointer (eyedropper hint).
//! It never takes the focus and lets clicks through.

use crate::wide::wide;
use std::cell::RefCell;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DT_LEFT,
    DT_NOPREFIX, DT_WORDBREAK, DeleteObject, DrawTextW, EndPaint, FF_DONTCARE, FW_NORMAL, FillRect, HGDIOBJ,
    InvalidateRect, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_TOPMOST, RegisterClassExW, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
    WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP, SetLayeredWindowAttributes, LWA_ALPHA,
};
use windows::core::PCWSTR;

thread_local! {
    static TEXT: RefCell<String> = const { RefCell::new(String::new()) };
}

const WIDTH: i32 = 360;
const HEIGHT: i32 = 64;

unsafe extern "system" fn tip_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_PAINT {
        // SAFETY: standard painting between BeginPaint and EndPaint; GDI
        // objects created here are deleted before returning.
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let rect = RECT { left: 0, top: 0, right: WIDTH, bottom: HEIGHT };
            let background = CreateSolidBrush(COLORREF(0x0020_2020));
            FillRect(hdc, &rect, background);
            let accent = CreateSolidBrush(COLORREF(0x00FF_B24C));
            FillRect(hdc, &RECT { left: 0, top: 0, right: 4, bottom: HEIGHT }, accent);
            let face = wide("Segoe UI");
            let font = CreateFontW(
                -15,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                FF_DONTCARE.0 as u32,
                PCWSTR(face.as_ptr()),
            );
            let old = SelectObject(hdc, HGDIOBJ(font.0));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(0x00FF_FFFF));
            let mut text: Vec<u16> = TEXT.with(|t| t.borrow().encode_utf16().collect());
            let mut text_rect = RECT { left: 14, top: 8, right: WIDTH - 8, bottom: HEIGHT - 6 };
            DrawTextW(hdc, &mut text, &mut text_rect, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);
            SelectObject(hdc, old);
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = DeleteObject(HGDIOBJ(background.0));
            let _ = DeleteObject(HGDIOBJ(accent.0));
            let _ = EndPaint(hwnd, &ps);
        }
        return LRESULT(0);
    }
    // SAFETY: default processing of the unchanged message.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// The label window. Must be used from the thread that created it, which
/// must run a message loop.
pub struct Tip {
    hwnd: HWND,
}

impl Tip {
    pub fn new(text: &str) -> Option<Tip> {
        TEXT.with(|t| *t.borrow_mut() = text.to_string());
        let class = wide("Declic.Tip");
        // SAFETY: registers a class with a valid window procedure and creates
        // a click-through, non-activating popup.
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(tip_proc),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                WIDTH,
                HEIGHT,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);
            Some(Tip { hwnd })
        }
    }

    /// Raw handle, e.g. to post messages to the tip's thread.
    pub fn raw(&self) -> isize {
        self.hwnd.0 as isize
    }

    /// Shows the label near a pointer position, with a new text.
    pub fn show_at(&self, x: i32, y: i32, text: &str) {
        TEXT.with(|t| *t.borrow_mut() = text.to_string());
        // SAFETY: plain window calls on our own window.
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x + 18, y + 22, WIDTH, HEIGHT, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }
}
