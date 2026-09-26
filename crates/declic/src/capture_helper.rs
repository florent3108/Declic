//! `declic.exe --capture`: records key combinations for the main window.
//!
//! A low-level keyboard hook installed by the process that owns the
//! foreground window is not reliably called, so recording runs in this small
//! window-less helper process started by the main window. It swallows every
//! key event while it runs (so no shortcut and no Windows combination fires)
//! and reports them on its standard output, one line per event:
//!
//! - `ready` once the hook is installed;
//! - `mods <bits>` when the held modifiers change;
//! - `key <key-id> <bits>` when a main key is pressed.
//!
//! It exits when its standard input is closed (or after a safety timeout).

use declic_win::hook::{Decision, KeyboardHook};
use declic_win::keys::KeyInput;
use std::io::{BufRead, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

/// Longest time the helper keeps the keyboard captured.
const SAFETY_TIMEOUT: Duration = Duration::from_secs(60);

pub fn run() {
    let (lines_tx, lines_rx) = mpsc::channel::<String>();
    let hook_tx = lines_tx.clone();
    let hook = KeyboardHook::start_exclusive(move |event| {
        let line = match event.input {
            KeyInput::Modifier(m, side) => Some(format!("mods {}", event.mods.with(m, side, event.down).bits())),
            KeyInput::Key(key) if event.down && !event.repeat => Some(format!("key {} {}", key.id(), event.mods.bits())),
            KeyInput::Key(_) => None,
        };
        if let Some(line) = line {
            let _ = hook_tx.send(line);
        }
        Decision::Swallow
    });
    let hook = match hook {
        Ok(hook) => hook,
        Err(e) => {
            println!("error {e}");
            return;
        }
    };
    // Writing happens on its own thread so that the hook never waits on the pipe.
    std::thread::spawn(move || {
        let mut out = std::io::stdout().lock();
        for line in lines_rx {
            if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    });
    let _ = lines_tx.send("ready".to_string());
    let (done_tx, done_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
        let _ = done_tx.send(());
    });
    let _ = done_rx.recv_timeout(SAFETY_TIMEOUT);
    drop(hook);
    std::process::exit(0);
}

/// Parsed line of the helper's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperLine {
    Ready,
    Modifiers(declic_core::ModState),
    Key(declic_core::Key, declic_core::ModState),
}

pub fn parse_line(line: &str) -> Option<HelperLine> {
    let mut parts = line.split_whitespace();
    match parts.next()? {
        "ready" => Some(HelperLine::Ready),
        "mods" => Some(HelperLine::Modifiers(declic_core::ModState::from_bits(parts.next()?.parse().ok()?))),
        "key" => {
            let key = declic_core::Key::from_id(parts.next()?)?;
            let mods = declic_core::ModState::from_bits(parts.next()?.parse().ok()?);
            Some(HelperLine::Key(key, mods))
        }
        _ => None,
    }
}

/// Reads the helper's output lines until it ends.
pub fn read_lines(stdout: impl Read, mut on_line: impl FnMut(HelperLine) -> bool) {
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if let Some(parsed) = parse_line(&line)
            && !on_line(parsed)
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use declic_core::{Key, ModState, Modifier, Side};

    #[test]
    fn protocol_round_trip() {
        let mods = ModState::EMPTY.with(Modifier::Ctrl, Side::Right, true).with(Modifier::Alt, Side::Left, true);
        assert_eq!(parse_line("ready"), Some(HelperLine::Ready));
        assert_eq!(parse_line(&format!("mods {}", mods.bits())), Some(HelperLine::Modifiers(mods)));
        assert_eq!(parse_line(&format!("key {} {}", Key::Numpad1.id(), mods.bits())), Some(HelperLine::Key(Key::Numpad1, mods)));
        assert_eq!(parse_line("key NotAKey 0"), None);
        assert_eq!(parse_line("garbage"), None);
        let mut seen = Vec::new();
        read_lines("ready\nkey M 0\n".as_bytes(), |l| {
            seen.push(l);
            true
        });
        assert_eq!(seen, vec![HelperLine::Ready, HelperLine::Key(Key::M, ModState::EMPTY)]);
    }
}
