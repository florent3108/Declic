//! Recording a key combination (F-COMB-03).
//!
//! While recording, a helper process (`declic.exe --capture`, see
//! `capture_helper`) swallows every key event with a low-level keyboard hook.
//! Being installed last, its hook is called before the background service's
//! hook, so no existing shortcut fires and combinations normally handled by
//! Windows (Win+E…) are recorded instead of executed. Stopping the recording
//! ends the helper process.

use crate::capture_helper::{self, HelperLine};
use declic_core::{Key, ModState};
use iced::Subscription;
use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Copy)]
pub enum CaptureEvent {
    /// The modifiers currently held changed.
    Modifiers(ModState),
    /// A main key was pressed with the given modifiers.
    Done(Key, ModState),
    /// The recording could not start or stopped unexpectedly.
    Failed,
}

pub fn subscription() -> Subscription<CaptureEvent> {
    Subscription::run(capture_stream)
}

/// The helper process; killed when the recording stops.
struct Helper(Child);

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_helper() -> std::io::Result<Helper> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe()?;
    let child = Command::new(exe)
        .arg("--capture")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(Helper(child))
}

fn capture_stream() -> impl Stream<Item = CaptureEvent> {
    iced::stream::channel(32, async |mut output: mpsc::Sender<CaptureEvent>| {
        let mut helper = match start_helper() {
            Ok(helper) => helper,
            Err(e) => {
                crate::log::warning!("key recording failed: {e}");
                let _ = output.send(CaptureEvent::Failed).await;
                iced::futures::future::pending::<()>().await;
                return;
            }
        };
        let (tx, mut rx) = mpsc::unbounded::<CaptureEvent>();
        if let Some(stdout) = helper.0.stdout.take() {
            std::thread::spawn(move || {
                capture_helper::read_lines(stdout, |line| {
                    let event = match line {
                        HelperLine::Ready => return true,
                        HelperLine::Modifiers(mods) => CaptureEvent::Modifiers(mods),
                        HelperLine::Key(key, mods) => CaptureEvent::Done(key, mods),
                    };
                    tx.unbounded_send(event).is_ok()
                });
                // The helper ended: the recording cannot go on.
                let _ = tx.unbounded_send(CaptureEvent::Failed);
            });
        }
        while let Some(event) = rx.next().await {
            if output.send(event).await.is_err() {
                break;
            }
        }
        drop(helper);
    })
}
/// A subscription producing a message every `interval_ms` milliseconds
/// (housekeeping: undo banner, external changes, theme, test countdown).
pub fn ticks(interval_ms: u64) -> Subscription<()> {
    Subscription::run_with(interval_ms, tick_stream)
}

fn tick_stream(interval_ms: &u64) -> impl Stream<Item = ()> + use<> {
    let interval = std::time::Duration::from_millis(*interval_ms);
    iced::stream::channel(1, async move |mut output: mpsc::Sender<()>| {
        let (tx, mut rx) = mpsc::unbounded::<()>();
        std::thread::Builder::new()
            .name("declic-ui-tick".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(interval);
                    if tx.unbounded_send(()).is_err() {
                        break;
                    }
                }
            })
            .ok();
        while rx.next().await.is_some() {
            if output.send(()).await.is_err() {
                break;
            }
        }
    })
}

/// Result of the eyedropper.
#[derive(Debug, Clone)]
pub enum PickEvent {
    Picked(crate::pick_helper::Picked),
    Cancelled,
}

/// Runs the eyedropper helper with the given hint text.
pub fn pick(hint: String) -> Subscription<PickEvent> {
    Subscription::run_with(hint, pick_stream)
}

// Subscription::run_with passes its data by reference: &String is imposed.
#[allow(clippy::ptr_arg)]
fn pick_stream(hint: &String) -> impl Stream<Item = PickEvent> + use<> {
    let hint = hint.clone();
    iced::stream::channel(4, async move |mut output: mpsc::Sender<PickEvent>| {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let spawned = std::env::current_exe().and_then(|exe| {
            Command::new(exe)
                .arg("--pick")
                .arg(&hint)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
        });
        let mut helper = match spawned {
            Ok(child) => Helper(child),
            Err(_) => {
                let _ = output.send(PickEvent::Cancelled).await;
                iced::futures::future::pending::<()>().await;
                return;
            }
        };
        let (tx, mut rx) = mpsc::unbounded::<PickEvent>();
        if let Some(stdout) = helper.0.stdout.take() {
            std::thread::spawn(move || {
                use std::io::BufRead;
                let mut line = String::new();
                let _ = std::io::BufReader::new(stdout).read_line(&mut line);
                let event = match crate::pick_helper::parse_line(line.trim_end()) {
                    Some(picked) => PickEvent::Picked(picked),
                    None => PickEvent::Cancelled,
                };
                let _ = tx.unbounded_send(event);
            });
        }
        if let Some(event) = rx.next().await {
            let _ = output.send(event).await;
        }
        drop(helper);
        iced::futures::future::pending::<()>().await;
    })
}