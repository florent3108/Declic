//! Declic — keyboard shortcuts for Windows.
//!
//! One executable, several roles:
//! - `declic.exe` : starts the background service (tray icon + keyboard hook)
//!   and opens the main window; if the service already runs, asks it to show
//!   the window;
//! - `declic.exe --background` : starts the service only (used at sign-in);
//! - `declic.exe --ui [--settings]` : the main window, started on demand by
//!   the service and ended when closed;
//! - `declic.exe --overlay <program>` : the cheat sheet, started by the
//!   service;
//! - `declic.exe --capture` / `--pick <hint>` : short-lived helpers used by
//!   the main window to record a key combination or to pick a window or a
//!   screen position.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Declic targets Windows 10/11.");

mod art;
mod capture_helper;
mod daemon;
mod exec;
mod log;
mod paths;
mod pick_helper;
mod tr;
mod ui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a.eq_ignore_ascii_case(flag));
    let value = |flag: &str| {
        args.iter().position(|a| a.eq_ignore_ascii_case(flag)).and_then(|i| args.get(i + 1)).cloned().unwrap_or_default()
    };
    if has("--capture") {
        capture_helper::run();
        return;
    }
    if has("--pick") {
        pick_helper::run(&value("--pick"));
        return;
    }
    if has("--overlay") {
        if let Err(e) = ui::run_overlay(value("--overlay")) {
            log::warning!("cheat sheet error: {e}");
        }
        return;
    }
    if has("--ui") {
        if let Err(e) = ui::run(has("--settings")) {
            log::warning!("main window error: {e}");
        }
        return;
    }
    let startup = if has("--background") { daemon::Startup::Background } else { daemon::Startup::Interactive };
    daemon::run(startup);
}
