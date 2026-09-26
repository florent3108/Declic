//! Background service: keyboard hook, action execution and tray icon.
//!
//! This process stays resident and is deliberately small: it never loads the
//! graphical toolkit. The main window runs in a separate process (`--ui`)
//! started on demand and ended when it is closed, so no rendering resources
//! stay in memory while the window is closed. Both processes share the
//! configuration file; this one reloads it whenever it changes on disk.
//!
//! Only one action runs at a time. A shortcut pressed while another action
//! (typically a macro) is running is **ignored** rather than queued: queued
//! actions would run later, in a context the user no longer expects. The
//! running macro can be interrupted with the stop key (Esc by default).

use crate::art::{self, IconVariant};
use crate::exec::{self, Env, Notice};
use crate::log::{info, warning};
use crate::paths::Paths;
use crate::tr;
use declic_core::config::{self, ConfigError};
use declic_core::i18n::Catalog;
use declic_core::stats::{Stats, Usage, now_unix};
use declic_core::vars::DateNames;
use declic_core::{Action, ActionKind, Config, Engine, Hotkey, ShortcutId};
use declic_win::hook::{Decision, HookEvent, KeyboardHook};
use declic_win::icon::Icon;
use declic_win::keys::KeyInput;
use declic_win::mouse_hook::{MouseAction, MouseHook};
use declic_win::tray::{MenuEntry, TrayEvent, TrayIcon};
use declic_win::watch::DirWatcher;
use declic_win::window::{self, copydata};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::process::{Child, Command};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, KillTimer, SM_CXSMICON, SetTimer, WM_APP, WM_COPYDATA, WM_TIMER,
};

pub const WINDOW_CLASS: &str = "Declic.Service";
const INSTANCE_NAME: &str = "Local\\Declic.Service";

const WM_TRAY: u32 = WM_APP + 1;
/// Posted by other Declic processes: open the main window (wparam 1 = settings).
pub const WM_OPEN_UI: u32 = WM_APP + 2;
const WM_WAKE: u32 = WM_APP + 3;
/// Posted by the executor: wparam 1 when a macro starts, 0 when it ends.
const WM_RUN_STATE: u32 = WM_APP + 4;
/// Posted by the hook when the cheat-sheet combination is pressed.
const WM_CHEAT_SHEET: u32 = WM_APP + 5;
/// Posted by the hook when a key is pressed while the cheat sheet is shown.
const WM_CLOSE_CHEAT_SHEET: u32 = WM_APP + 6;
/// Posted by the mouse hook while the cheat sheet is shown: a button was
/// pressed at (wparam, lparam) on the screen.
const WM_CHEAT_SHEET_CLICK: u32 = WM_APP + 7;

const TIMER_RUNNING: usize = 1;
const TIMER_STATS: usize = 2;
/// Checks whether the cheat sheet closed itself (click elsewhere…).
const TIMER_CHEAT_SHEET: usize = 3;
const CHEAT_SHEET_POLL_MS: u32 = 200;

const CMD_OPEN: u32 = 1;
const CMD_PAUSE: u32 = 2;
const CMD_SETTINGS: u32 = 3;
const CMD_QUIT: u32 = 4;

/// Delay before the "macro running" indicator appears (short actions do
/// not make the icon flicker).
const RUNNING_INDICATOR_DELAY_MS: u32 = 250;
/// Statistics are written at most this often.
const STATS_FLUSH_MS: u32 = 3000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Startup {
    /// Started by the user: show the main window.
    Interactive,
    /// Started with Windows: stay in the notification area.
    Background,
}

/// Settings used by the hook and executor threads.
struct Runtime {
    stop_key: Hotkey,
    open_window: Option<Hotkey>,
    cheat_sheet: Option<Hotkey>,
    notifications: bool,
    typing_delay: Duration,
    names: DateNames,
}

impl Runtime {
    fn new(config: &Config, tr: &Catalog) -> Runtime {
        let s = &config.settings;
        Runtime {
            stop_key: s.stop_key,
            open_window: s.open_window_keys,
            cheat_sheet: s.cheat_sheet_keys,
            notifications: s.notifications,
            typing_delay: Duration::from_millis(s.typing_delay_ms as u64),
            names: tr::date_names(tr),
        }
    }
}

/// State shared with the keyboard hook and executor threads.
struct Shared {
    engine: RwLock<Arc<Engine>>,
    runtime: RwLock<Arc<Runtime>>,
    paused: AtomicBool,
    /// An action is running (or queued for the executor).
    busy: AtomicBool,
    /// The user asked to stop the running macro.
    abort: AtomicBool,
    /// The cheat sheet is shown: the next key press closes it.
    cheat_sheet_open: AtomicBool,
}

impl Shared {
    fn runtime(&self) -> Arc<Runtime> {
        self.runtime.read().map(|r| Arc::clone(&r)).unwrap_or_else(|p| Arc::clone(&p.into_inner()))
    }

    /// Reserves the executor; false if an action is already running.
    fn try_start(&self) -> bool {
        if self.busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.abort.store(false, Ordering::Relaxed);
        true
    }
}

/// Work for the executor thread.
struct Job {
    /// The triggered shortcut (`None` for a test run from the editor).
    id: Option<ShortcutId>,
    action: Action,
}

enum Event {
    ConfigChanged,
    Notice(Notice),
    Used(ShortcutId),
}

/// Payload of a test run sent by the main window.
#[derive(Serialize, Deserialize)]
pub struct TestRun {
    pub action: Action,
}

struct Service {
    paths: Paths,
    shared: Arc<Shared>,
    jobs: Sender<Job>,
    tr: Catalog,
    language_pref: String,
    tray: Option<TrayIcon>,
    icon_active: Option<Icon>,
    icon_paused: Option<Icon>,
    icon_running: Option<Icon>,
    running: bool,
    events: Receiver<Event>,
    ui: Option<Child>,
    overlay: Option<Child>,
    /// Watches clicks while the cheat sheet is shown (a click elsewhere closes it).
    overlay_mouse: Option<MouseHook>,
    hwnd: HWND,
    taskbar_created: u32,
    last_config_text: Option<String>,
    stats: Stats,
    stats_dirty: bool,
    /// Identity of each shortcut (see [`fingerprints`]).
    shortcut_prints: HashMap<ShortcutId, String>,
    /// Statistics of deleted shortcuts, kept for this session so that undoing
    /// a deletion keeps the counters: identity, usage.
    removed_stats: HashMap<ShortcutId, (String, Usage)>,
}

/// Combination and action of each shortcut. Identifiers can be reused after a
/// deletion, so statistics are only given back to a shortcut that comes back
/// identical (an undone deletion), not to a new one with the same identifier.
fn fingerprints(config: &Config) -> HashMap<ShortcutId, String> {
    config.shortcuts.iter().map(|s| (s.id, format!("{}|{:?}", s.keys, s.action))).collect()
}

fn spawn_ui(settings: bool) -> Option<Child> {
    let exe = std::env::current_exe().ok()?;
    window::allow_foreground(None);
    let mut command = Command::new(exe);
    command.arg("--ui");
    if settings {
        command.arg("--settings");
    }
    match command.spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            warning!("cannot start the main window: {e}");
            None
        }
    }
}

/// Runs the service. Returns when the user quits.
pub fn run(startup: Startup) {
    let Some(_instance) = window::SingleInstance::acquire(INSTANCE_NAME) else {
        // Already running: ask the running instance to show its window.
        if startup == Startup::Interactive
            && let Some(hwnd) = window::find_window(WINDOW_CLASS)
        {
            window::allow_foreground(None);
            window::post_message(hwnd, WM_OPEN_UI, 0, 0);
        }
        return;
    };
    // The window process starts first, in parallel with the service
    // initialisation, so that it appears as soon as possible.
    let early_ui = if startup == Startup::Interactive { spawn_ui(false) } else { None };
    window::set_dpi_aware();
    let paths = Paths::resolve();
    crate::log::init(&paths.log_file());
    info!("service starting (config: {})", paths.config.display());

    let (config, config_text, load_error) = load_or_create(&paths);
    let tr = tr::load(&config.settings.language);
    let shared = Arc::new(Shared {
        engine: RwLock::new(Arc::new(Engine::new(&config.shortcuts))),
        runtime: RwLock::new(Arc::new(Runtime::new(&config, &tr))),
        paused: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        abort: AtomicBool::new(false),
        cheat_sheet_open: AtomicBool::new(false),
    });

    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let service = Rc::new(RefCell::new(None::<Service>));
    let handler_service = Rc::clone(&service);
    let taskbar_created = window::register_message("TaskbarCreated");
    let hwnd = match window::create_hidden_window(WINDOW_CLASS, move |hwnd, msg, wparam, lparam| {
        handle_message(&handler_service, hwnd, msg, wparam, lparam)
    }) {
        Ok(hwnd) => hwnd,
        Err(e) => {
            warning!("cannot create the service window: {e}");
            return;
        }
    };
    let raw_hwnd = hwnd.0 as isize;

    // Executor thread: runs actions outside of the hook thread, one at a time.
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    {
        let shared = Arc::clone(&shared);
        let event_tx = event_tx.clone();
        let _ = std::thread::Builder::new().name("declic-executor".into()).spawn(move || {
            declic_win::launch::init_com();
            while let Ok(job) = job_rx.recv() {
                let is_macro = job.action.kind() == ActionKind::Macro;
                if is_macro {
                    window::post_message(raw_hwnd, WM_RUN_STATE, 1, 0);
                }
                let runtime = shared.runtime();
                let notify = |notice: Notice| {
                    let _ = event_tx.send(Event::Notice(notice));
                    window::post_message(raw_hwnd, WM_WAKE, 0, 0);
                };
                let env = Env {
                    names: runtime.names.clone(),
                    typing_delay: runtime.typing_delay,
                    abort: &shared.abort,
                    notify: &notify,
                };
                let outcome = exec::run(&job.action, &env);
                info!("action {:?} ended: {outcome:?}", job.id);
                if is_macro {
                    window::post_message(raw_hwnd, WM_RUN_STATE, 0, 0);
                }
                if let Some(id) = job.id {
                    let _ = event_tx.send(Event::Used(id));
                    window::post_message(raw_hwnd, WM_WAKE, 0, 0);
                }
                shared.busy.store(false, Ordering::Release);
            }
        });
    }

    let hook = start_hook(Arc::clone(&shared), job_tx.clone(), raw_hwnd);
    let watcher = {
        let event_tx = event_tx.clone();
        DirWatcher::start(&paths.dir, move || {
            let _ = event_tx.send(Event::ConfigChanged);
            window::post_message(raw_hwnd, WM_WAKE, 0, 0);
        })
    };
    if let Err(e) = &watcher {
        warning!("cannot watch the configuration folder: {e}");
    }

    // SAFETY: plain query.
    let icon_size = (unsafe { GetSystemMetrics(SM_CXSMICON) }).clamp(16, 64) as u32;
    let make_icon = |variant| declic_win::icon::from_rgba(icon_size, icon_size, &art::render(icon_size, variant));
    let icon_active = make_icon(IconVariant::Active);
    let icon_paused = make_icon(IconVariant::Paused);
    let icon_running = make_icon(IconVariant::Running);
    let tray = icon_active.as_ref().and_then(|icon| TrayIcon::add(hwnd, 1, WM_TRAY, icon.0, tr.get("tray.tooltip")));
    if tray.is_none() {
        warning!("cannot add the notification-area icon");
    }
    let mut stats = Stats::load(&paths.stats_file());
    let shortcut_prints = fingerprints(&config);
    stats.retain(|id| shortcut_prints.contains_key(&id));

    *service.borrow_mut() = Some(Service {
        paths,
        shared,
        jobs: job_tx,
        language_pref: config.settings.language.clone(),
        tr,
        tray,
        icon_active,
        icon_paused,
        icon_running,
        running: false,
        events: event_rx,
        ui: early_ui,
        overlay: None,
        overlay_mouse: None,
        hwnd,
        taskbar_created,
        last_config_text: config_text,
        stats,
        stats_dirty: false,
        shortcut_prints,
        removed_stats: HashMap::new(),
    });

    if let Some(service) = service.borrow_mut().as_mut() {
        match &hook {
            Ok(_) => info!("keyboard hook installed"),
            Err(e) => {
                warning!("keyboard hook failed: {e}");
                let text = service.tr.fmt("notify.hook_failed", &[("error", &e.to_string())]);
                service.notify(service.tr.get("notify.hook_failed_title"), &text, true, true);
            }
        }
        if let Some(error) = load_error {
            let text = service.tr.fmt("notify.config_error", &[("error", &error)]);
            service.notify(service.tr.get("notify.config_error_title"), &text, true, true);
        }
    }

    window::run_message_loop();

    info!("service stopping");
    drop(hook);
    drop(watcher);
    if let Some(mut service) = service.borrow_mut().take() {
        service.flush_stats();
        for child in [service.ui.as_mut(), service.overlay.as_mut()].into_iter().flatten() {
            if matches!(child.try_wait(), Ok(None)) {
                window::close_process_windows(child.id());
            }
        }
    }
}

/// Loads the configuration, creating a default file on first run.
/// Returns the configuration, its text on disk and a load error, if any.
fn load_or_create(paths: &Paths) -> (Config, Option<String>, Option<String>) {
    match config::load(&paths.config) {
        Ok(Some(config)) => {
            let text = std::fs::read_to_string(&paths.config).ok();
            (config, text, None)
        }
        Ok(None) => {
            let config = Config::new();
            match config::save(&paths.config, &config, 0) {
                Ok(()) => info!("created {}", paths.config.display()),
                Err(e) => warning!("cannot create {}: {e}", paths.config.display()),
            }
            let text = std::fs::read_to_string(&paths.config).ok();
            (config, text, None)
        }
        Err(e) => {
            warning!("invalid configuration: {e}");
            (Config::new(), None, Some(e.to_string()))
        }
    }
}

fn start_hook(shared: Arc<Shared>, jobs: Sender<Job>, service_hwnd: isize) -> std::io::Result<KeyboardHook> {
    // Keys whose press was handled: their repeats and release are hidden too.
    let mut swallowed = [false; 256];
    KeyboardHook::start(move |event: &HookEvent| {
        let KeyInput::Key(key) = event.input else { return Decision::Pass };
        let index = (event.vk & 0xFF) as usize;
        if !event.down {
            return if std::mem::take(&mut swallowed[index]) { Decision::Swallow } else { Decision::Pass };
        }
        if swallowed[index] {
            return Decision::Swallow; // auto-repeat of a handled key
        }
        let runtime = shared.runtime();
        let handled = |swallowed: &mut [bool; 256]| {
            swallowed[index] = true;
            Decision::Swallow
        };
        // Stop key: only while an action runs (F-MAC-03).
        if shared.busy.load(Ordering::Acquire) && runtime.stop_key.matches(key, event.mods) {
            shared.abort.store(true, Ordering::Release);
            return handled(&mut swallowed);
        }
        if runtime.open_window.is_some_and(|h| h.matches(key, event.mods)) {
            window::post_message(service_hwnd, WM_OPEN_UI, 0, 0);
            return handled(&mut swallowed);
        }
        if runtime.cheat_sheet.is_some_and(|h| h.matches(key, event.mods)) {
            window::post_message(service_hwnd, WM_CHEAT_SHEET, 0, 0);
            return handled(&mut swallowed);
        }
        // Any other key closes the cheat sheet (§ 8.4). This is done here
        // rather than in the cheat-sheet window because Windows does not always
        // let that window take the keyboard focus. The key is consumed, like a
        // key that dismisses a menu.
        if shared.cheat_sheet_open.swap(false, Ordering::AcqRel) {
            window::post_message(service_hwnd, WM_CLOSE_CHEAT_SHEET, 0, 0);
            return handled(&mut swallowed);
        }
        if shared.paused.load(Ordering::Relaxed) {
            return Decision::Pass;
        }
        let engine = match shared.engine.read() {
            Ok(engine) => Arc::clone(&engine),
            Err(_) => return Decision::Pass,
        };
        if !engine.uses_key(key) {
            return Decision::Pass;
        }
        let hit = engine.resolve(key, event.mods, event.locks, &mut declic_win::process::foreground_program);
        match hit {
            Some(shortcut) => {
                if shared.try_start() {
                    let _ = jobs.send(Job { id: Some(shortcut.id), action: shortcut.action.clone() });
                } else {
                    info!("shortcut {} ignored: another action is running", shortcut.id);
                }
                handled(&mut swallowed)
            }
            None => Decision::Pass,
        }
    })
}

fn handle_message(
    service: &Rc<RefCell<Option<Service>>>,
    _hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    let mut guard = service.try_borrow_mut().ok()?;
    let service = guard.as_mut()?;
    match msg {
        WM_TRAY => match declic_win::tray::decode_event(wparam.0, lparam.0) {
            TrayEvent::Activate => service.open_ui(false),
            TrayEvent::ContextMenu { x, y } => service.show_menu(Some((x, y))),
            TrayEvent::NotificationClicked | TrayEvent::Other => {}
        },
        WM_OPEN_UI => service.open_ui(wparam.0 == 1),
        WM_WAKE => service.drain_events(),
        WM_RUN_STATE => service.run_state(wparam.0 == 1),
        WM_CHEAT_SHEET => service.toggle_cheat_sheet(),
        WM_CLOSE_CHEAT_SHEET => {
            service.close_cheat_sheet();
        }
        WM_CHEAT_SHEET_CLICK => service.cheat_sheet_click(wparam.0 as i32, lparam.0 as i32),
        WM_TIMER => service.timer(wparam.0),
        WM_COPYDATA => {
            // SAFETY: this is a WM_COPYDATA message being handled.
            if let Some((kind, payload)) = unsafe { window::read_copydata(lparam) } {
                service.copydata(kind, &payload);
            }
            return Some(LRESULT(1));
        }
        m if m == service.taskbar_created && m != 0 => service.readd_tray(),
        _ => return None,
    }
    Some(LRESULT(0))
}

impl Service {
    /// Shows a notification. `important` ones (errors of the service itself)
    /// ignore the "notifications" setting.
    fn notify(&self, title: &str, text: &str, warning: bool, important: bool) {
        if !important && !self.shared.runtime().notifications {
            return;
        }
        match &self.tray {
            Some(tray) => tray.notify(title, text, warning),
            None => warning!("{title}: {text}"),
        }
    }

    fn current_icon(&self) -> Option<&Icon> {
        if self.running {
            self.icon_running.as_ref()
        } else if self.shared.paused.load(Ordering::Relaxed) {
            self.icon_paused.as_ref()
        } else {
            self.icon_active.as_ref()
        }
    }

    fn tooltip(&self) -> String {
        if self.running {
            let key = tr::hotkey_text(&self.tr, &self.shared.runtime().stop_key);
            return self.tr.fmt("tray.tooltip_running", &[("key", &key)]);
        }
        self.tr
            .get(if self.shared.paused.load(Ordering::Relaxed) { "tray.tooltip_paused" } else { "tray.tooltip" })
            .to_string()
    }

    fn refresh_tray(&self) {
        if let (Some(tray), Some(icon)) = (&self.tray, self.current_icon()) {
            tray.update(icon.0, &self.tooltip());
        }
    }

    fn readd_tray(&mut self) {
        let tip = self.tooltip();
        if let (Some(tray), Some(icon)) = (&self.tray, self.current_icon()) {
            tray.readd(icon.0, &tip);
        } else if let Some(icon) = self.current_icon() {
            let tray = TrayIcon::add(self.hwnd, 1, WM_TRAY, icon.0, &tip);
            self.tray = tray;
        }
    }

    fn run_state(&mut self, started: bool) {
        // SAFETY: timers on our own window.
        unsafe {
            if started {
                SetTimer(Some(self.hwnd), TIMER_RUNNING, RUNNING_INDICATOR_DELAY_MS, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_RUNNING);
                if self.running {
                    self.running = false;
                    self.refresh_tray();
                }
            }
        }
    }

    fn timer(&mut self, id: usize) {
        // SAFETY: killing our own one-shot timers.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), id);
        }
        match id {
            TIMER_RUNNING if self.shared.busy.load(Ordering::Acquire) => {
                self.running = true;
                self.refresh_tray();
            }
            TIMER_STATS => self.flush_stats(),
            TIMER_CHEAT_SHEET => {
                if self.overlay.as_mut().is_some_and(|child| matches!(child.try_wait(), Ok(None))) {
                    // SAFETY: timer on our own window.
                    unsafe {
                        SetTimer(Some(self.hwnd), TIMER_CHEAT_SHEET, CHEAT_SHEET_POLL_MS, None);
                    }
                } else {
                    self.overlay = None;
                    self.overlay_mouse = None;
                    self.shared.cheat_sheet_open.store(false, Ordering::Release);
                }
            }
            _ => {}
        }
    }

    fn flush_stats(&mut self) {
        if self.stats_dirty {
            if let Err(e) = self.stats.save(&self.paths.stats_file()) {
                warning!("cannot save statistics: {e}");
            }
            self.stats_dirty = false;
        }
    }

    fn mark_stats_dirty(&mut self) {
        if !self.stats_dirty {
            self.stats_dirty = true;
            // SAFETY: timer on our own window.
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_STATS, STATS_FLUSH_MS, None);
            }
        }
    }

    fn copydata(&mut self, kind: usize, payload: &str) {
        match kind {
            copydata::RUN_ACTION => match toml::from_str::<TestRun>(payload) {
                Ok(run) => {
                    if self.shared.try_start() {
                        let _ = self.jobs.send(Job { id: None, action: run.action });
                    } else {
                        info!("test run ignored: another action is running");
                    }
                }
                Err(e) => warning!("invalid test run: {e}"),
            },
            copydata::RESET_STATS => {
                self.stats.reset(payload.trim().parse().ok());
                self.stats_dirty = true;
                self.flush_stats();
            }
            _ => {}
        }
    }

    fn show_menu(&mut self, at: Option<(i32, i32)>) {
        let paused = self.shared.paused.load(Ordering::Relaxed);
        let entries = [
            MenuEntry::Item { id: CMD_OPEN, label: self.tr.get("tray.open").into(), checked: false, default: true },
            MenuEntry::Item { id: CMD_PAUSE, label: self.tr.get("tray.pause").into(), checked: paused, default: false },
            MenuEntry::Item { id: CMD_SETTINGS, label: self.tr.get("tray.settings").into(), checked: false, default: false },
            MenuEntry::Separator,
            MenuEntry::Item { id: CMD_QUIT, label: self.tr.get("tray.quit").into(), checked: false, default: false },
        ];
        match declic_win::tray::show_menu(self.hwnd, &entries, at, self.tr.is_rtl()) {
            Some(CMD_OPEN) => self.open_ui(false),
            Some(CMD_PAUSE) => {
                self.shared.paused.store(!paused, Ordering::Relaxed);
                info!("shortcuts {}", if paused { "resumed" } else { "paused" });
                self.refresh_tray();
            }
            Some(CMD_SETTINGS) => self.open_ui(true),
            Some(CMD_QUIT) => window::quit_message_loop(),
            _ => {}
        }
        // Messages posted while the menu was open may have been consumed by its loop.
        self.drain_events();
    }

    /// Shows the main window: focuses it if already open, else starts it.
    fn open_ui(&mut self, settings: bool) {
        if let Some(child) = self.ui.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                window::focus_process_window(child.id());
                return;
            }
            self.ui = None;
        }
        self.ui = spawn_ui(settings);
    }

    /// Shows the cheat sheet for the foreground program, or hides it.
    fn toggle_cheat_sheet(&mut self) {
        if self.close_cheat_sheet() {
            return;
        }
        let program = declic_win::process::foreground_program().unwrap_or_default();
        let Ok(exe) = std::env::current_exe() else { return };
        window::allow_foreground(None);
        match Command::new(exe).arg("--overlay").arg(&program).spawn() {
            Ok(child) => {
                self.overlay = Some(child);
                self.shared.cheat_sheet_open.store(true, Ordering::Release);
                // SAFETY: timer on our own window.
                unsafe {
                    SetTimer(Some(self.hwnd), TIMER_CHEAT_SHEET, CHEAT_SHEET_POLL_MS, None);
                }
                // The cheat sheet closes itself when it loses the focus, but
                // Windows does not always give it the focus: clicks are
                // watched here too. The hook only reports them.
                let hwnd = self.hwnd.0 as isize;
                let hook = MouseHook::start(move |event| {
                    if matches!(event.action, MouseAction::LeftDown | MouseAction::RightDown | MouseAction::MiddleDown) {
                        window::post_message(hwnd, WM_CHEAT_SHEET_CLICK, event.x as usize, event.y as isize);
                    }
                    Decision::Pass
                });
                self.overlay_mouse = hook.map_err(|e| warning!("cannot watch clicks for the cheat sheet: {e}")).ok();
            }
            Err(e) => warning!("cannot show the cheat sheet: {e}"),
        }
    }

    /// A button was pressed while the cheat sheet is shown: a click on a
    /// window of another program closes it.
    fn cheat_sheet_click(&mut self, x: i32, y: i32) {
        if !self.shared.cheat_sheet_open.load(Ordering::Acquire) {
            return;
        }
        let own = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        let target = declic_win::winfind::window_at(x, y).and_then(|w| w.program);
        if own.is_none() || target != own {
            self.close_cheat_sheet();
        }
    }

    /// Closes the cheat sheet; returns whether it was still shown.
    fn close_cheat_sheet(&mut self) -> bool {
        self.overlay_mouse = None;
        self.shared.cheat_sheet_open.store(false, Ordering::Release);
        // SAFETY: timer on our own window.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_CHEAT_SHEET);
        }
        let Some(mut child) = self.overlay.take() else { return false };
        if !matches!(child.try_wait(), Ok(None)) {
            return false;
        }
        let _ = child.kill();
        let _ = child.wait();
        true
    }

    fn drain_events(&mut self) {
        let mut reload = false;
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::ConfigChanged => reload = true,
                Event::Notice(notice) => self.show_notice(notice),
                Event::Used(id) => {
                    self.stats.record(id, now_unix());
                    self.mark_stats_dirty();
                }
            }
        }
        if reload {
            self.reload_config();
        }
    }

    fn show_notice(&self, notice: Notice) {
        let tr = &self.tr;
        let (title, text, warning, always) = match notice {
            Notice::NotFound(target) => {
                (tr.get("notify.not_found_title"), tr.fmt("notify.not_found", &[("target", &target)]), true, false)
            }
            Notice::LaunchFailed(target, code) => (
                tr.get("notify.launch_failed_title"),
                tr.fmt("notify.launch_failed", &[("target", &target), ("code", &code.to_string())]),
                true,
                false,
            ),
            Notice::Elevated => (tr.get("notify.elevated_title"), tr.get("notify.elevated").to_string(), true, false),
            Notice::InjectFailed => {
                (tr.get("notify.inject_failed_title"), tr.get("notify.inject_failed").to_string(), true, false)
            }
            Notice::WindowNotFound(window) => (
                tr.get("notify.window_not_found_title"),
                tr.fmt("notify.window_not_found", &[("window", &window)]),
                true,
                false,
            ),
            Notice::Stopped => (tr.get("notify.stopped_title"), tr.get("notify.stopped").to_string(), false, false),
            Notice::Message(text) => (tr.get("app.name"), text, false, true),
        };
        info!("{title}: {text}");
        self.notify(title, &text, warning, always);
    }

    fn reload_config(&mut self) {
        let Ok(text) = std::fs::read_to_string(&self.paths.config) else { return };
        if self.last_config_text.as_deref() == Some(text.as_str()) {
            return;
        }
        self.last_config_text = Some(text.clone());
        match config::parse(text.trim_start_matches('\u{feff}')) {
            Ok(config) => {
                if let Ok(mut engine) = self.shared.engine.write() {
                    *engine = Arc::new(Engine::new(&config.shortcuts));
                }
                if config.settings.language != self.language_pref {
                    self.language_pref = config.settings.language.clone();
                    self.tr = tr::load(&self.language_pref);
                }
                if let Ok(mut runtime) = self.shared.runtime.write() {
                    *runtime = Arc::new(Runtime::new(&config, &self.tr));
                }
                self.refresh_tray();
                let prints = fingerprints(&config);
                let before = self.stats.clone();
                for (id, usage) in self.stats.take_missing(|id| prints.contains_key(&id)) {
                    if let Some(print) = self.shortcut_prints.get(&id) {
                        self.removed_stats.insert(id, (print.clone(), usage));
                    }
                }
                self.removed_stats.retain(|id, (print, usage)| match prints.get(id) {
                    Some(current) if current == print => {
                        self.stats.restore(*id, *usage);
                        false
                    }
                    // The identifier now belongs to another shortcut.
                    Some(_) => false,
                    None => true,
                });
                self.shortcut_prints = prints;
                if self.stats != before {
                    self.mark_stats_dirty();
                }
                info!("configuration reloaded ({} shortcuts)", config.shortcuts.len());
            }
            Err(e) => {
                let error = match &e {
                    ConfigError::TooNew { .. } | ConfigError::Parse(_) | ConfigError::Io(_) => e.to_string(),
                };
                warning!("configuration not reloaded: {error}");
                let text = self.tr.fmt("notify.config_error", &[("error", &error)]);
                self.notify(self.tr.get("notify.config_error_title"), &text, true, true);
            }
        }
    }
}

/// Raw handle of the background service's window, if it runs.
pub fn service_window() -> Option<isize> {
    window::find_window(WINDOW_CLASS)
}

/// Whether the background service is running (used by the main window).
pub fn is_running() -> bool {
    service_window().is_some()
}

/// Starts the background service in its own process.
pub fn spawn_background() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(exe).arg("--background").spawn().map(|_| ())
}

/// Asks the service to run an action once (editor's Test button).
pub fn request_test_run(action: &Action) -> bool {
    let Some(hwnd) = service_window() else { return false };
    let Ok(payload) = toml::to_string(&TestRun { action: action.clone() }) else { return false };
    window::allow_foreground(None);
    window::send_copydata(hwnd, copydata::RUN_ACTION, &payload)
}

/// Asks the service to reset statistics (one shortcut, or all).
pub fn request_stats_reset(id: Option<ShortcutId>) -> bool {
    let Some(hwnd) = service_window() else { return false };
    window::send_copydata(hwnd, copydata::RESET_STATS, &id.map(|i| i.to_string()).unwrap_or_default())
}
