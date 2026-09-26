//! Watching a directory for changes, without polling (F-DAT-04).

use crate::wide::wide;
use std::io;
use std::path::Path;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects};
use windows::core::PCWSTR;

#[derive(Clone, Copy)]
struct RawHandle(isize);

// SAFETY: kernel object handles can be used from any thread.
unsafe impl Send for RawHandle {}

impl RawHandle {
    fn get(self) -> HANDLE {
        HANDLE(self.0 as *mut _)
    }
}

/// Watches a directory; `on_change` is called (on a background thread) after
/// changes, debounced. Dropping the watcher stops it.
pub struct DirWatcher {
    stop: RawHandle,
    thread: Option<JoinHandle<()>>,
}

impl DirWatcher {
    pub fn start<F>(dir: &Path, mut on_change: F) -> io::Result<DirWatcher>
    where
        F: FnMut() + Send + 'static,
    {
        let dir_w = wide(dir.as_os_str());
        // SAFETY: handles are closed by the watcher thread / Drop.
        let (change, stop) = unsafe {
            let change = FindFirstChangeNotificationW(
                PCWSTR(dir_w.as_ptr()),
                false,
                FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_SIZE,
            )
            .map_err(io::Error::other)?;
            let stop = match CreateEventW(None, true, false, PCWSTR::null()) {
                Ok(stop) => stop,
                Err(e) => {
                    let _ = FindCloseChangeNotification(change);
                    return Err(io::Error::other(e));
                }
            };
            (RawHandle(change.0 as isize), RawHandle(stop.0 as isize))
        };
        let thread = std::thread::Builder::new().name("declic-config-watch".into()).spawn(move || {
            let handles = [change.get(), stop.get()];
            loop {
                // SAFETY: both handles stay valid until this thread ends.
                let result = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
                if result != WAIT_OBJECT_0 {
                    break;
                }
                // Let a burst of writes (temporary file + rename) settle.
                std::thread::sleep(Duration::from_millis(150));
                // SAFETY: re-arming the notification handle.
                if unsafe { FindNextChangeNotification(change.get()) }.is_err() {
                    break;
                }
                on_change();
            }
            // SAFETY: closing the change handle owned by this thread.
            unsafe {
                let _ = FindCloseChangeNotification(change.get());
            }
        })?;
        Ok(DirWatcher { stop, thread: Some(thread) })
    }
}

impl Drop for DirWatcher {
    fn drop(&mut self) {
        // SAFETY: signalling then closing our own event handle.
        unsafe {
            let _ = SetEvent(self.stop.get());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // SAFETY: the thread has ended; the event is closed once.
        unsafe {
            let _ = CloseHandle(self.stop.get());
        }
    }
}
