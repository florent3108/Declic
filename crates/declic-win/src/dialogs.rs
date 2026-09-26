//! Standard Windows file and folder pickers.

use crate::launch::init_com;
use crate::wide::wide;
use std::path::PathBuf;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_NODEREFERENCELINKS, FOS_OVERWRITEPROMPT, FOS_PICKFOLDERS, FileOpenDialog, FileSaveDialog,
    IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::PCWSTR;

/// What to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    /// Any file; Windows shortcuts are returned as-is (not dereferenced).
    File,
    /// A program (`.exe`).
    Program,
    Folder,
}

/// Shows a modal picker owned by the current foreground window (the Declic
/// window that requested it). Blocks until the user closes it, so call it
/// from a background thread. Returns `None` when cancelled.
pub fn pick(kind: PickKind, title: &str, programs_label: &str, all_files_label: &str) -> Option<PathBuf> {
    init_com();
    // SAFETY: COM objects created and used on this thread; strings outlive the calls.
    unsafe {
        let owner = GetForegroundWindow();
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let mut options = dialog.GetOptions().ok()? | FOS_FORCEFILESYSTEM;
        match kind {
            PickKind::Folder => options |= FOS_PICKFOLDERS,
            PickKind::File => options |= FOS_NODEREFERENCELINKS,
            PickKind::Program => {}
        }
        dialog.SetOptions(options).ok()?;
        let title_w = wide(title);
        let _ = dialog.SetTitle(PCWSTR(title_w.as_ptr()));
        let programs_w = wide(programs_label);
        let exe_w = wide("*.exe");
        let all_w = wide(all_files_label);
        let any_w = wide("*.*");
        if kind == PickKind::Program {
            let filters = [
                COMDLG_FILTERSPEC { pszName: PCWSTR(programs_w.as_ptr()), pszSpec: PCWSTR(exe_w.as_ptr()) },
                COMDLG_FILTERSPEC { pszName: PCWSTR(all_w.as_ptr()), pszSpec: PCWSTR(any_w.as_ptr()) },
            ];
            let _ = dialog.SetFileTypes(&filters);
        }
        let owner = (!owner.is_invalid()).then_some(owner);
        dialog.Show(owner).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(PathBuf::from)
    }
}

/// Shows an "open" dialog restricted to one file type (plus all files).
pub fn pick_filtered(title: &str, filter_label: &str, pattern: &str, all_files_label: &str) -> Option<PathBuf> {
    init_com();
    // SAFETY: COM objects created and used on this thread; strings outlive the calls.
    unsafe {
        let owner = GetForegroundWindow();
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()? | FOS_FORCEFILESYSTEM;
        dialog.SetOptions(options).ok()?;
        let title_w = wide(title);
        let _ = dialog.SetTitle(PCWSTR(title_w.as_ptr()));
        let (label_w, pattern_w, all_w, any_w) = (wide(filter_label), wide(pattern), wide(all_files_label), wide("*.*"));
        let filters = [
            COMDLG_FILTERSPEC { pszName: PCWSTR(label_w.as_ptr()), pszSpec: PCWSTR(pattern_w.as_ptr()) },
            COMDLG_FILTERSPEC { pszName: PCWSTR(all_w.as_ptr()), pszSpec: PCWSTR(any_w.as_ptr()) },
        ];
        let _ = dialog.SetFileTypes(&filters);
        let owner = (!owner.is_invalid()).then_some(owner);
        dialog.Show(owner).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(PathBuf::from)
    }
}

/// Shows a "save as" dialog. `extension` is without dot (e.g. `csv`).
pub fn save(title: &str, default_name: &str, filter_label: &str, extension: &str) -> Option<PathBuf> {
    init_com();
    // SAFETY: COM objects created and used on this thread; strings outlive the calls.
    unsafe {
        let owner = GetForegroundWindow();
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()? | FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT;
        dialog.SetOptions(options).ok()?;
        let title_w = wide(title);
        let _ = dialog.SetTitle(PCWSTR(title_w.as_ptr()));
        let name_w = wide(default_name);
        let _ = dialog.SetFileName(PCWSTR(name_w.as_ptr()));
        let ext_w = wide(extension);
        let _ = dialog.SetDefaultExtension(PCWSTR(ext_w.as_ptr()));
        let label_w = wide(filter_label);
        let pattern_w = wide(format!("*.{extension}"));
        let filters = [COMDLG_FILTERSPEC { pszName: PCWSTR(label_w.as_ptr()), pszSpec: PCWSTR(pattern_w.as_ptr()) }];
        let _ = dialog.SetFileTypes(&filters);
        let owner = (!owner.is_invalid()).then_some(owner);
        dialog.Show(owner).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(PathBuf::from)
    }
}