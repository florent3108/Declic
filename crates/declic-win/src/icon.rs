//! Creation of Windows icons from RGBA pixels.

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO};

/// An owned icon handle, destroyed on drop.
pub struct Icon(pub HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: the icon was created by us and is destroyed once.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

// SAFETY: icon handles are process-wide GDI objects usable from any thread.
unsafe impl Send for Icon {}

/// Builds an icon from straight (non-premultiplied) RGBA pixels, row by row
/// from the top.
pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Icon> {
    if rgba.len() != (width * height * 4) as usize {
        return None;
    }
    // SAFETY: the DIB section is sized for width*height 32-bit pixels and the
    // copy stays within it; GDI objects are deleted after the icon is created.
    unsafe {
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let info = BITMAPINFO { bmiHeader: header, ..Default::default() };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, rgba.len());
        for (d, s) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
        let mask_bits = vec![0u8; (width.div_ceil(16) * 2 * height) as usize];
        let mask = CreateBitmap(width as i32, height as i32, 1, 1, Some(mask_bits.as_ptr() as *const _));
        let icon_info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&icon_info);
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        icon.ok().map(Icon)
    }
}
