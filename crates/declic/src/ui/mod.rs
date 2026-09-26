//! Main window (iced), run in its own short-lived process.

mod capture;
mod drag_list;
mod editor;
mod fit;
mod icons;
mod list;
mod overlay;
mod settings;
mod steps;
mod style;
mod transfer;
mod widgets;

pub use overlay::run_overlay;

use crate::art::{self, IconVariant};
use crate::daemon;
use crate::paths::Paths;
use crate::tr;
use capture::{CaptureEvent, PickEvent};
use declic_core::config::{self, FORMAT_VERSION};
use declic_core::conflict;
use declic_core::i18n::Catalog;
use declic_core::stats::Stats;
use declic_core::{
    Action, ActionKind, Config, Hotkey, KeyList, ModState, MoveOrigin, Shortcut, ShortcutId, Step, TextMode, ThemePref,
};
use editor::{EdMsg, EditKind, Editor};
use iced::widget::{Row, column, container};
use iced::{Element, Length, Padding, Size, Subscription, Task, Theme, keyboard, window};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use style::Tone;
use widgets::font;

const UNDO_SECONDS: u64 = 8;
const TOAST_SECONDS: u64 = 8;
const CAPTURE_TIMEOUT_SECONDS: u64 = 15;
const TEST_COUNTDOWN: Duration = Duration::from_secs(3);

const INITIAL_SIZE: Size = Size::new(1240.0, 760.0);
/// Smallest window in which the list stays readable next to the editor
/// (the sidebar is then hidden, see [`App::sidebar_width`]).
const MIN_SIZE: Size = Size::new(1000.0, 560.0);
/// Below this width, the side columns get narrower when a panel is open.
const WIDE_WINDOW: f32 = 1440.0;
/// Below this width, the sidebar is hidden while a panel is open.
const NARROW_WINDOW: f32 = 1180.0;

fn search_id() -> iced::widget::Id {
    iced::widget::Id::new("search")
}

fn group_input_id() -> iced::widget::Id {
    iced::widget::Id::new("group-name")
}

/// Chooses the font and the writing direction of this process from the
/// configured language, before any window is created.
pub fn prepare_interface(paths: &Paths) {
    let language = config::load(&paths.config).ok().flatten().map(|c| c.settings.language).unwrap_or_else(|| "auto".into());
    let tr = tr::load(&language);
    widgets::set_font_family(tr::font_family(&tr));
    widgets::set_rtl(tr.is_rtl());
}

/// Starts the main window. Returns when it is closed.
pub fn run(open_settings: bool) -> iced::Result {
    let paths = Paths::resolve();
    crate::log::init(&paths.log_file());
    prepare_interface(&paths);
    let icon = window::icon::from_rgba(art::render(64, IconVariant::Active), 64, 64).ok();
    iced::application(move || App::new(open_settings), App::update, App::view)
        .title(|app: &App| app.tr.get("app.name").to_string())
        .theme(|app: &App| app.theme.clone())
        .subscription(App::subscription)
        .default_font(font())
        .window(window::Settings {
            size: INITIAL_SIZE,
            min_size: Some(MIN_SIZE),
            position: window::Position::Centered,
            icon,
            exit_on_close_request: false,
            ..window::Settings::default()
        })
        .run()
}

/// Which shortcuts the side panel shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupFilter {
    All,
    Favorites,
    Group(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Created,
    Name,
    Keys,
    Kind,
    Uses,
    LastUsed,
}

impl SortKey {
    pub const ALL: [SortKey; 6] =
        [SortKey::Created, SortKey::Name, SortKey::Keys, SortKey::Kind, SortKey::Uses, SortKey::LastUsed];
}

/// Settings that are key combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingKey {
    Stop,
    OpenWindow,
    CheatSheet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    Editor,
    Search,
    Setting(SettingKey),
    /// Combination of a "press keys" step.
    StepKeys(usize),
    /// Keys of a "hold" or "release" step.
    StepHold(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickTarget {
    StepWindow(usize),
    StepPosition(usize),
    ConditionProgram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickPurpose {
    TargetFile,
    TargetFolder,
    Program,
    StepTarget(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Example {
    Email,
    Notepad,
    Date,
    Documents,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    Settings,
    Transfer,
}

/// Actions applied to the selected shortcuts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bulk {
    Delete,
    Enable,
    Disable,
    Duplicate,
    MoveTo(String),
    Export,
    ResetStats,
}

/// Answer to "the editor has unsaved changes".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Save,
    Discard,
    Cancel,
}

/// Group being created or renamed in the side panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupEdit {
    New(String),
    Rename { old: String, name: String },
}

/// An installed program shown in the editor's picker.
#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub path: PathBuf,
    pub icon: Option<iced::widget::image::Handle>,
}

type Rgba = (u32, u32, Vec<u8>);

#[derive(Debug, Clone)]
pub enum Message {
    Search(String),
    GroupFilter(GroupFilter),
    KindFilter(Option<ActionKind>),
    Sort(SortKey),
    SearchByKeys,
    ClearSearchKeys,
    New,
    Example(Example),
    Select(ShortcutId),
    RowClicked(ShortcutId),
    Toggle(ShortcutId, bool),
    Favorite(ShortcutId, bool),
    SelectOne(ShortcutId, bool),
    SelectAll(bool),
    Bulk(Bulk),
    Undo,
    DismissUndo,
    DismissToast,
    GroupStartNew,
    GroupStartRename(String),
    GroupInput(String),
    GroupSubmit,
    GroupCancel,
    GroupDelete(String),
    GroupDeleteConfirmed(bool),
    Ed(EdMsg),
    Save,
    CloseEditor,
    Delete,
    Test,
    RunTest(Action),
    ResetStats(Option<ShortcutId>),
    Confirm(ConfirmChoice),
    StartCapture(CaptureTarget),
    CancelCapture,
    Capture(CaptureEvent),
    StartPick(PickTarget),
    PickResult(PickEvent),
    Pick(PickPurpose),
    Picked(PickPurpose, Option<PathBuf>),
    AppsLoaded(Vec<(String, PathBuf, Option<Rgba>)>),
    AppChosen(PathBuf),
    IconsLoaded(Vec<(String, Option<Rgba>)>),
    OpenPanel(Panel),
    SetLanguage(String),
    SetTheme(ThemePref),
    SetAutostart(bool),
    SetTypingDelay(u32),
    SetDefaultTextMode(TextMode),
    SetNotifications(bool),
    ClearSettingKeys(SettingKey),
    OpenConfigFolder,
    OpenLog,
    Transfer(transfer::TransferMsg),
    StartService,
    Reload,
    Tick,
    Escape,
    FocusSearch,
    MoveStep(i32),
    FileDropped(PathBuf),
    WindowUnfocused,
    /// The window was resized (new width).
    WindowResized(f32),
    /// The macro step list asks the editor to scroll while a step is dragged.
    StepAutoScroll(f32),
    WindowId(Option<window::Id>),
    ModifiersChanged(keyboard::Modifiers),
    CloseRequested,
    Quit,
}

struct Undo {
    items: Vec<(usize, Shortcut)>,
    since: Instant,
}

pub struct App {
    paths: Paths,
    config: Config,
    load_error: Option<String>,
    file_stamp: Option<(SystemTime, u64)>,
    stats: Stats,
    stats_stamp: Option<(SystemTime, u64)>,
    tr: Catalog,
    languages: Vec<(String, String)>,
    /// Current width of the window (for the responsive layout).
    window_width: f32,
    theme: Theme,
    theme_key: (bool, Option<(u8, u8, u8)>),
    search: String,
    search_keys: Option<Hotkey>,
    group_filter: GroupFilter,
    kind_filter: Option<ActionKind>,
    sort: SortKey,
    selection: BTreeSet<ShortcutId>,
    selected: Option<ShortcutId>,
    editor: Option<Editor>,
    panel: Panel,
    pending: Option<Box<Message>>,
    group_edit: Option<GroupEdit>,
    group_delete: Option<String>,
    capture: Option<CaptureTarget>,
    capture_mods: ModState,
    capture_peak: ModState,
    capture_started: Instant,
    pick: Option<PickTarget>,
    undo: Option<Undo>,
    toast: Option<(String, Tone, Instant)>,
    conflicts: HashSet<ShortcutId>,
    reserved: HashSet<Hotkey>,
    service_running: bool,
    autostart: bool,
    running_programs: Vec<String>,
    installed_apps: Option<Vec<AppEntry>>,
    row_icons: HashMap<String, Option<iced::widget::image::Handle>>,
    test_countdown: Option<(Instant, Action)>,
    transfer: transfer::TransferState,
    window_id: Option<window::Id>,
    modifiers: keyboard::Modifiers,
    editor_opened: bool,
}

fn file_stamp(path: &std::path::Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

fn theme_inputs(pref: ThemePref) -> (bool, Option<(u8, u8, u8)>) {
    let dark = match pref {
        ThemePref::System => declic_win::system::apps_use_dark_theme(),
        ThemePref::Light => false,
        ThemePref::Dark => true,
    };
    (dark, declic_win::system::accent_color())
}

fn build_theme((dark, accent): (bool, Option<(u8, u8, u8)>)) -> Theme {
    let accent = match accent {
        Some((r, g, b)) => style::adapt_accent(iced::Color::from_rgb8(r, g, b), dark),
        None => style::default_accent(dark),
    };
    style::make_theme(dark, accent)
}

/// Runs a blocking function on a new thread and returns its result as a task.
fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    to_message: impl FnOnce(T) -> Message + Send + 'static,
) -> Task<Message> {
    Task::perform(
        async move {
            let (tx, rx) = iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(f());
            });
            rx.await.ok()
        },
        move |result| match result {
            Some(value) => to_message(value),
            None => Message::Tick,
        },
    )
}

/// File that an Open target designates, for its icon (programs found in
/// the PATH, documents, folders); `None` for URLs and protocols.
fn resolve_target(target: &str) -> Option<PathBuf> {
    let expanded =
        declic_core::vars::expand(target.trim().trim_matches('"'), &declic_win::system::SystemVars, &Default::default());
    if expanded.is_empty() || (expanded.contains(':') && !declic_win::launch::is_explicit_path(&expanded)) {
        return None;
    }
    let path = PathBuf::from(&expanded);
    if path.is_absolute() {
        return path.exists().then_some(path);
    }
    if expanded.contains(['\\', '/']) {
        return None;
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(windir) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(windir));
    }
    dirs.into_iter().map(|d| d.join(&expanded)).find(|p| p.is_file())
}

impl App {
    fn new(open_settings: bool) -> (App, Task<Message>) {
        let paths = Paths::resolve();
        let (config, load_error) = match config::load(&paths.config) {
            Ok(Some(config)) => (config, None),
            Ok(None) => {
                let config = Config::new();
                let _ = config::save(&paths.config, &config, 0);
                (config, None)
            }
            Err(e) => (Config::new(), Some(e.to_string())),
        };
        let tr = tr::load(&config.settings.language);
        let theme_key = theme_inputs(config.settings.theme);
        let stats_path = paths.stats_file();
        let mut app = App {
            file_stamp: file_stamp(&paths.config),
            stats: Stats::load(&stats_path),
            stats_stamp: file_stamp(&stats_path),
            paths,
            load_error,
            tr,
            languages: tr::available_languages(),
            window_width: INITIAL_SIZE.width,
            theme: build_theme(theme_key),
            theme_key,
            search: String::new(),
            search_keys: None,
            group_filter: GroupFilter::All,
            kind_filter: None,
            sort: SortKey::Created,
            selection: BTreeSet::new(),
            selected: None,
            editor: None,
            panel: if open_settings { Panel::Settings } else { Panel::None },
            pending: None,
            group_edit: None,
            group_delete: None,
            capture: None,
            capture_mods: ModState::EMPTY,
            capture_peak: ModState::EMPTY,
            capture_started: Instant::now(),
            pick: None,
            undo: None,
            toast: None,
            conflicts: HashSet::new(),
            reserved: HashSet::new(),
            service_running: daemon::is_running(),
            autostart: declic_win::autostart::is_enabled(),
            running_programs: Vec::new(),
            installed_apps: None,
            row_icons: HashMap::new(),
            test_countdown: None,
            transfer: transfer::TransferState::default(),
            window_id: None,
            modifiers: keyboard::Modifiers::default(),
            editor_opened: false,
            config,
        };
        app.refresh_conflicts();
        let icons = app.load_row_icons();
        (
            app,
            Task::batch([iced::widget::operation::focus(search_id()), window::latest().map(Message::WindowId), icons]),
        )
    }

    fn refresh_conflicts(&mut self) {
        self.conflicts = conflict::conflicting_ids(&self.config.shortcuts).into_iter().collect();
        self.refresh_reserved();
    }

    /// Probes which combinations are also registered by other programs (F-CNF-02).
    fn refresh_reserved(&mut self) {
        let mut keys: Vec<Hotkey> = self.config.shortcuts.iter().map(|s| s.keys).collect();
        if let Some(hk) = self.editor.as_ref().and_then(|e| e.hotkey) {
            keys.push(hk);
        }
        self.reserved = keys
            .into_iter()
            .filter(|hk| declic_win::reserved::probe(hk) == declic_win::reserved::Availability::UsedElsewhere)
            .collect();
    }

    /// Loads, in the background, the icons of Open targets not yet known.
    fn load_row_icons(&mut self) -> Task<Message> {
        let targets: Vec<String> = self
            .config
            .shortcuts
            .iter()
            .filter_map(|s| match &s.action {
                Action::Open(open) => Some(open.target.clone()),
                _ => None,
            })
            .filter(|t| !self.row_icons.contains_key(t))
            .collect();
        if targets.is_empty() {
            return Task::none();
        }
        for t in &targets {
            self.row_icons.insert(t.clone(), None);
        }
        blocking(
            move || {
                declic_win::launch::init_com();
                targets
                    .into_iter()
                    .map(|t| {
                        let icon = resolve_target(&t).and_then(|p| declic_win::apps::file_icon_rgba(&p));
                        (t, icon)
                    })
                    .collect()
            },
            Message::IconsLoaded,
        )
    }

    fn read_only(&self) -> bool {
        self.load_error.is_some()
    }

    fn set_toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.toast = Some((text.into(), tone, Instant::now()));
    }

    /// Writes the configuration to disk (auto-save, F-DAT-02).
    fn persist(&mut self) -> Task<Message> {
        if self.read_only() {
            return Task::none();
        }
        self.config.version = FORMAT_VERSION;
        match config::save(&self.paths.config, &self.config, config::DEFAULT_BACKUPS) {
            Ok(()) => self.file_stamp = file_stamp(&self.paths.config),
            Err(e) => {
                crate::log::warning!("save failed: {e}");
                let msg = self.tr.fmt("main.save_failed", &[("error", &e.to_string())]);
                self.set_toast(msg, Tone::Danger);
            }
        }
        let config = &self.config;
        self.selection.retain(|id| config.get(*id).is_some());
        self.refresh_conflicts();
        self.load_row_icons()
    }

    /// Name shown for a shortcut: its own, or a suggestion.
    fn display_name(&self, shortcut: &Shortcut) -> String {
        if !shortcut.name.trim().is_empty() {
            return shortcut.name.clone();
        }
        editor::suggested_name(&self.tr, &shortcut.action).unwrap_or_else(|| shortcut.keys.to_string())
    }

    fn reload_from_disk(&mut self) -> Task<Message> {
        match config::load(&self.paths.config) {
            Ok(Some(config)) => {
                let language_changed = config.settings.language != self.config.settings.language;
                let theme_changed = config.settings.theme != self.config.settings.theme;
                self.config = config;
                self.load_error = None;
                if language_changed {
                    self.apply_language();
                }
                if theme_changed {
                    self.refresh_theme(true);
                }
            }
            Ok(None) => {}
            Err(e) => self.load_error = Some(e.to_string()),
        }
        self.file_stamp = file_stamp(&self.paths.config);
        let config = &self.config;
        self.selection.retain(|id| config.get(*id).is_some());
        self.refresh_conflicts();
        self.load_row_icons()
    }

    fn apply_language(&mut self) {
        self.tr = tr::load(&self.config.settings.language);
        widgets::set_rtl(self.tr.is_rtl());
        if let Some(ed) = self.editor.as_mut() {
            ed.key_choices = iced::widget::combo_box::State::new(editor::key_choices(&self.tr));
        }
    }

    fn refresh_theme(&mut self, force: bool) {
        let key = theme_inputs(self.config.settings.theme);
        if force || key != self.theme_key {
            self.theme_key = key;
            self.theme = build_theme(key);
        }
    }

    fn open_editor(&mut self, editor: Editor) {
        self.editor_opened = true;
        self.panel = Panel::None;
        self.capture = None;
        self.editor = Some(editor);
    }

    fn start_capture(&mut self, target: CaptureTarget) {
        self.capture = Some(target);
        self.capture_mods = ModState::EMPTY;
        self.capture_peak = ModState::EMPTY;
        self.capture_started = Instant::now();
    }

    fn editor_dirty(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.is_dirty())
    }

    /// Asks for confirmation before leaving an editor with unsaved changes.
    /// Returns true when `next` may proceed now.
    fn guard(&mut self, next: &Message) -> bool {
        if self.editor_dirty() {
            self.pending = Some(Box::new(next.clone()));
            return false;
        }
        true
    }

    fn minimize(&self, minimized: bool) -> Task<Message> {
        match self.window_id {
            Some(id) if minimized => window::minimize(id, true),
            Some(id) => Task::batch([window::minimize(id, false), window::gain_focus(id)]),
            None => Task::none(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        if std::mem::take(&mut self.editor_opened) {
            return Task::batch([
                task,
                iced::widget::operation::snap_to(editor::scroll_id(), iced::widget::scrollable::RelativeOffset::START),
            ]);
        }
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Search(q) => self.search = q,
            Message::GroupFilter(f) => {
                self.group_filter = f;
                self.group_edit = None;
                self.group_delete = None;
            }
            Message::KindFilter(k) => self.kind_filter = k,
            Message::Sort(s) => self.sort = s,
            Message::SearchByKeys => self.start_capture(CaptureTarget::Search),
            Message::ClearSearchKeys => self.search_keys = None,
            Message::New => {
                if !self.guard(&Message::New) {
                    return Task::none();
                }
                let mut ed = Editor::new(&self.tr, self.config.settings.default_text_mode);
                ed.kind = EditKind::Text;
                if let GroupFilter::Group(g) = &self.group_filter {
                    ed.group = g.clone();
                }
                ed.favorite = self.group_filter == GroupFilter::Favorites;
                self.open_editor(ed);
                self.start_capture(CaptureTarget::Editor);
            }
            Message::Example(example) => {
                if !self.guard(&Message::Example(example)) {
                    return Task::none();
                }
                return self.add_example(example);
            }
            Message::Select(id) => {
                if self.editor.as_ref().is_some_and(|e| e.id == Some(id)) {
                    return Task::none();
                }
                if !self.guard(&Message::Select(id)) {
                    return Task::none();
                }
                if let Some(shortcut) = self.config.get(id) {
                    let ed = Editor::from_shortcut(shortcut, &self.tr);
                    self.selected = Some(id);
                    self.open_editor(ed);
                    self.refresh_reserved();
                }
            }
            Message::RowClicked(id) => {
                if self.modifiers.control() {
                    if !self.selection.remove(&id) {
                        self.selection.insert(id);
                    }
                } else {
                    return self.handle(Message::Select(id));
                }
            }
            Message::Toggle(id, enabled) => {
                if !self.read_only()
                    && let Some(s) = self.config.get_mut(id)
                {
                    s.enabled = enabled;
                    if let Some(ed) = self.editor.as_mut().filter(|e| e.id == Some(id)) {
                        ed.enabled = enabled;
                        if let Some(o) = ed.original.as_mut() {
                            o.enabled = enabled;
                        }
                    }
                    return self.persist();
                }
            }
            Message::Favorite(id, favorite) => {
                if !self.read_only()
                    && let Some(s) = self.config.get_mut(id)
                {
                    s.favorite = favorite;
                    if let Some(ed) = self.editor.as_mut().filter(|e| e.id == Some(id)) {
                        ed.favorite = favorite;
                        if let Some(o) = ed.original.as_mut() {
                            o.favorite = favorite;
                        }
                    }
                    return self.persist();
                }
            }
            Message::SelectOne(id, on) => {
                if on {
                    self.selection.insert(id);
                } else {
                    self.selection.remove(&id);
                }
            }
            Message::SelectAll(on) => {
                if on {
                    let ids: Vec<ShortcutId> = self.visible_shortcuts().iter().map(|s| s.id).collect();
                    self.selection.extend(ids);
                } else {
                    self.selection.clear();
                }
            }
            Message::Bulk(action) => return self.bulk(action),
            Message::Undo => {
                if let Some(undo) = self.undo.take() {
                    for (index, mut shortcut) in undo.items.into_iter() {
                        if self.config.get(shortcut.id).is_some() {
                            shortcut.id = self.config.next_id();
                        }
                        let index = index.min(self.config.shortcuts.len());
                        self.config.shortcuts.insert(index, shortcut);
                    }
                    return self.persist();
                }
            }
            Message::DismissUndo => self.undo = None,
            Message::DismissToast => self.toast = None,
            Message::GroupStartNew => {
                self.group_edit = Some(GroupEdit::New(String::new()));
                return iced::widget::operation::focus(group_input_id());
            }
            Message::GroupStartRename(old) => {
                self.group_edit = Some(GroupEdit::Rename { name: old.clone(), old });
                return iced::widget::operation::focus(group_input_id());
            }
            Message::GroupInput(value) => match self.group_edit.as_mut() {
                Some(GroupEdit::New(name)) | Some(GroupEdit::Rename { name, .. }) => *name = value,
                None => {}
            },
            Message::GroupSubmit => {
                if self.read_only() {
                    return Task::none();
                }
                match self.group_edit.take() {
                    Some(GroupEdit::New(name)) => {
                        if let Some(stored) = self.config.ensure_group(&name) {
                            self.group_filter = GroupFilter::Group(stored);
                            return self.persist();
                        }
                    }
                    Some(GroupEdit::Rename { old, name }) => {
                        if self.config.rename_group(&old, &name) {
                            self.group_filter = GroupFilter::Group(name.trim().to_string());
                            if let Some(ed) = self.editor.as_mut().filter(|e| e.group == old) {
                                ed.group = name.trim().to_string();
                            }
                            return self.persist();
                        }
                        let msg = self.tr.get("groups.name_taken").to_string();
                        self.set_toast(msg, Tone::Danger);
                    }
                    None => {}
                }
            }
            Message::GroupCancel => self.group_edit = None,
            Message::GroupDelete(name) => self.group_delete = Some(name),
            Message::GroupDeleteConfirmed(yes) => {
                if let Some(name) = self.group_delete.take()
                    && yes
                    && !self.read_only()
                {
                    self.config.delete_group(&name);
                    if let Some(ed) = self.editor.as_mut().filter(|e| e.group == name) {
                        ed.group.clear();
                    }
                    self.group_filter = GroupFilter::All;
                    return self.persist();
                }
            }
            Message::Ed(msg) => {
                let refresh = matches!(msg, EdMsg::RefreshRunning | EdMsg::ToggleAdvanced);
                let apps = matches!(msg, EdMsg::ToggleApps) && self.installed_apps.is_none();
                let default_mode = self.config.settings.default_text_mode;
                let manual = matches!(msg, EdMsg::ManualKey(_) | EdMsg::ManualMod(..) | EdMsg::Sided(_));
                if let Some(ed) = self.editor.as_mut() {
                    ed.update(msg, default_mode);
                }
                if manual {
                    self.refresh_reserved();
                }
                if refresh {
                    self.running_programs = declic_win::process::running_programs();
                }
                if apps {
                    return blocking(
                        || {
                            declic_win::launch::init_com();
                            declic_win::apps::installed_apps()
                                .into_iter()
                                .map(|a| {
                                    let icon = declic_win::apps::file_icon_rgba(&a.path);
                                    (a.name, a.path, icon)
                                })
                                .collect()
                        },
                        Message::AppsLoaded,
                    );
                }
            }
            Message::Save => return self.save_editor(),
            Message::CloseEditor => {
                if !self.guard(&Message::CloseEditor) {
                    return Task::none();
                }
                self.editor = None;
                self.capture = None;
            }
            Message::Delete => {
                if let Some(id) = self.editor.as_ref().and_then(|e| e.id)
                    && !self.read_only()
                    && let Some(index) = self.config.shortcuts.iter().position(|s| s.id == id)
                {
                    let shortcut = self.config.shortcuts.remove(index);
                    self.undo = Some(Undo { items: vec![(index, shortcut)], since: Instant::now() });
                    self.editor = None;
                    self.selected = None;
                    self.capture = None;
                    return self.persist();
                }
            }
            Message::Test => return self.start_test(),
            Message::RunTest(action) => {
                if !daemon::request_test_run(&action) {
                    let msg = self.tr.get("editor.test_no_service").to_string();
                    self.set_toast(msg, Tone::Danger);
                }
            }
            Message::ResetStats(id) => {
                if !daemon::request_stats_reset(id) {
                    // The service is not running: edit the file directly.
                    self.stats.reset(id);
                    let _ = self.stats.save(&self.paths.stats_file());
                }
                self.stats = Stats::load(&self.paths.stats_file());
                self.stats_stamp = file_stamp(&self.paths.stats_file());
            }
            Message::Confirm(choice) => {
                let pending = self.pending.take();
                match choice {
                    ConfirmChoice::Cancel => {}
                    ConfirmChoice::Discard => {
                        self.editor = None;
                        self.capture = None;
                        if let Some(next) = pending {
                            return self.handle(*next);
                        }
                    }
                    ConfirmChoice::Save => {
                        let task = self.save_editor();
                        if self.editor.is_none()
                            && let Some(next) = pending
                        {
                            return Task::batch([task, self.handle(*next)]);
                        }
                        return task;
                    }
                }
            }
            Message::StartCapture(target) => self.start_capture(target),
            Message::CancelCapture => self.capture = None,
            Message::Capture(event) => return self.on_capture(event),
            Message::StartPick(target) => {
                self.pick = Some(target);
                return self.minimize(true);
            }
            Message::PickResult(event) => {
                let target = self.pick.take();
                if let (Some(target), PickEvent::Picked(p)) = (target, event) {
                    self.apply_pick(target, p);
                }
                return self.minimize(false);
            }
            Message::Pick(purpose) => return self.pick_file(purpose),
            Message::Picked(purpose, path) => {
                if let (Some(path), Some(ed)) = (path, self.editor.as_mut()) {
                    let path = path.to_string_lossy().into_owned();
                    match purpose {
                        PickPurpose::TargetFile | PickPurpose::TargetFolder => {
                            ed.kind = EditKind::Open;
                            ed.target = path;
                            ed.refresh_lnk();
                        }
                        PickPurpose::Program => ed.add_program(&path),
                        PickPurpose::StepTarget(i) => {
                            ed.update(EdMsg::Step(i, steps::StepMsg::Target(path)), TextMode::Typing)
                        }
                    }
                }
            }
            Message::AppsLoaded(apps) => {
                self.installed_apps = Some(
                    apps.into_iter()
                        .map(|(name, path, icon)| AppEntry {
                            name,
                            path,
                            icon: icon.map(|(w, h, px)| iced::widget::image::Handle::from_rgba(w, h, px)),
                        })
                        .collect(),
                );
            }
            Message::AppChosen(path) => {
                if let Some(ed) = self.editor.as_mut() {
                    ed.kind = EditKind::Open;
                    ed.target = path.to_string_lossy().into_owned();
                    ed.apps_open = false;
                    ed.refresh_lnk();
                }
            }
            Message::IconsLoaded(icons) => {
                for (target, icon) in icons {
                    self.row_icons.insert(target, icon.map(|(w, h, px)| iced::widget::image::Handle::from_rgba(w, h, px)));
                }
            }
            Message::OpenPanel(panel) => {
                if panel != Panel::None && !self.guard(&Message::OpenPanel(panel)) {
                    return Task::none();
                }
                if panel != Panel::None {
                    self.editor = None;
                    self.capture = None;
                }
                self.panel = panel;
                self.autostart = declic_win::autostart::is_enabled();
            }
            Message::SetLanguage(code) => {
                self.config.settings.language = code;
                self.apply_language();
                return self.persist();
            }
            Message::SetTheme(pref) => {
                self.config.settings.theme = pref;
                self.refresh_theme(true);
                return self.persist();
            }
            Message::SetAutostart(enabled) => {
                if let Ok(exe) = std::env::current_exe() {
                    if let Err(e) = declic_win::autostart::set_enabled(enabled, &exe) {
                        self.set_toast(e.to_string(), Tone::Danger);
                    }
                    self.autostart = declic_win::autostart::is_enabled();
                }
            }
            Message::SetTypingDelay(ms) => {
                self.config.settings.typing_delay_ms = ms;
                return self.persist();
            }
            Message::SetDefaultTextMode(mode) => {
                self.config.settings.default_text_mode = mode;
                return self.persist();
            }
            Message::SetNotifications(on) => {
                self.config.settings.notifications = on;
                return self.persist();
            }
            Message::ClearSettingKeys(key) => {
                match key {
                    SettingKey::Stop => self.config.settings.stop_key = Hotkey::new(declic_core::Key::Escape),
                    SettingKey::OpenWindow => self.config.settings.open_window_keys = None,
                    SettingKey::CheatSheet => self.config.settings.cheat_sheet_keys = None,
                }
                return self.persist();
            }
            Message::OpenConfigFolder => {
                let dir = self.paths.dir.to_string_lossy().into_owned();
                let _ = declic_win::launch::open(&dir, "", "", declic_core::WindowState::Normal, false);
            }
            Message::OpenLog => {
                let log = self.paths.log_file().to_string_lossy().into_owned();
                let _ = declic_win::launch::open(&log, "", "", declic_core::WindowState::Normal, false);
            }
            Message::Transfer(msg) => return transfer::update(self, msg),
            Message::StartService => {
                if let Err(e) = daemon::spawn_background() {
                    self.set_toast(e.to_string(), Tone::Danger);
                }
            }
            Message::Reload => return self.reload_from_disk(),
            Message::Tick => return self.on_tick(),
            Message::Escape => {
                if let Some(ed) = self.editor.as_mut().filter(|ed| ed.dragging.is_some()) {
                    // Esc first cancels a step being dragged.
                    ed.dragging = None;
                } else if self.capture.is_some() {
                    self.capture = None;
                } else if self.pending.is_some() {
                    self.pending = None;
                } else if self.test_countdown.is_some() {
                    self.test_countdown = None;
                } else if self.group_edit.is_some() {
                    self.group_edit = None;
                } else if self.editor.is_some() {
                    return self.handle(Message::CloseEditor);
                } else if self.panel != Panel::None {
                    self.panel = Panel::None;
                } else if !self.selection.is_empty() {
                    self.selection.clear();
                } else if self.search_keys.is_some() || !self.search.is_empty() {
                    self.search.clear();
                    self.search_keys = None;
                }
            }
            Message::FocusSearch => return iced::widget::operation::focus(search_id()),
            Message::MoveStep(delta) => {
                if let Some(ed) = self.editor.as_mut().filter(|e| e.kind == EditKind::Macro) {
                    steps::move_selected(ed, delta);
                }
            }
            Message::FileDropped(path) => {
                let lower = path.to_string_lossy().to_lowercase();
                if lower.ends_with(".toml") && self.editor.is_none() {
                    // A dropped export file is imported rather than opened.
                    self.panel = Panel::Transfer;
                    return transfer::update(self, transfer::TransferMsg::ImportFile(Some(path)));
                }
                if self.editor.is_none() {
                    self.open_editor(Editor::new(&self.tr, self.config.settings.default_text_mode));
                }
                if let Some(ed) = self.editor.as_mut() {
                    ed.kind = EditKind::Open;
                    ed.target = path.to_string_lossy().into_owned();
                    ed.refresh_lnk();
                }
            }
            Message::WindowResized(width) => self.window_width = width,
            Message::StepAutoScroll(dy) => {
                return iced::widget::operation::scroll_by(editor::scroll_id(), iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: dy });
            }
            Message::WindowUnfocused => {
                if self.pick.is_none() {
                    self.capture = None;
                }
            }
            Message::WindowId(id) => self.window_id = id,
            Message::ModifiersChanged(m) => self.modifiers = m,
            Message::CloseRequested => {
                if !self.guard(&Message::Quit) {
                    return self.minimize(false);
                }
                return iced::exit();
            }
            Message::Quit => return iced::exit(),
        }
        Task::none()
    }

    fn apply_pick(&mut self, target: PickTarget, p: crate::pick_helper::Picked) {
        let Some(ed) = self.editor.as_mut() else { return };
        match target {
            PickTarget::ConditionProgram => {
                if !p.program.is_empty() {
                    ed.add_program(&p.program);
                    if ed.conditions.program_mode == declic_core::ProgramMode::Everywhere {
                        ed.conditions.program_mode = declic_core::ProgramMode::OnlyIn;
                    }
                }
            }
            PickTarget::StepWindow(i) => {
                ed.update(EdMsg::Step(i, steps::StepMsg::Title(p.title.clone())), TextMode::Typing);
                ed.update(EdMsg::Step(i, steps::StepMsg::Program(p.program.clone())), TextMode::Typing);
            }
            PickTarget::StepPosition(i) => {
                if let Some(draft) = ed.steps.get_mut(i)
                    && let Step::MoveMouse { origin, .. } = draft.step
                {
                    let (x, y) = match origin {
                        MoveOrigin::Window => (p.x - p.rect.0, p.y - p.rect.1),
                        _ => (p.x, p.y),
                    };
                    draft.numbers = [x.to_string(), y.to_string()];
                }
            }
        }
    }

    fn start_test(&mut self) -> Task<Message> {
        let Some(action) = self.editor.as_ref().and_then(|e| e.build_action().ok()) else {
            return Task::none();
        };
        if !self.service_running {
            let msg = self.tr.get("editor.test_no_service").to_string();
            self.set_toast(msg, Tone::Danger);
            return Task::none();
        }
        if action.kind() == ActionKind::Open {
            return Task::done(Message::RunTest(action));
        }
        // Text and macros are tested in the previous window, after a countdown.
        self.test_countdown = Some((Instant::now() + TEST_COUNTDOWN, action));
        Task::none()
    }

    fn on_capture(&mut self, event: CaptureEvent) -> Task<Message> {
        let Some(target) = self.capture else { return Task::none() };
        fn finish_hold(app: &mut App, i: usize, keys: KeyList) {
            if let Some(draft) = app.editor.as_mut().and_then(|e| e.steps.get_mut(i)) {
                match &mut draft.step {
                    Step::HoldKeys { keys: k } | Step::ReleaseKeys { keys: k } => *k = keys,
                    _ => {}
                }
            }
        }
        match event {
            CaptureEvent::Modifiers(mods) => {
                self.capture_mods = mods;
                self.capture_peak = ModState::from_bits(self.capture_peak.bits() | mods.bits());
                // Hold/release steps may use modifiers alone: finish when all are released.
                if let CaptureTarget::StepHold(i) = target
                    && mods.is_empty()
                    && !self.capture_peak.is_empty()
                {
                    let hk = Hotkey::from_state(declic_core::Key::Space, self.capture_peak, true);
                    let mut keys = KeyList::from_hotkey(&hk);
                    keys.0.pop();
                    self.capture = None;
                    finish_hold(self, i, keys);
                }
            }
            CaptureEvent::Failed => self.capture = None,
            CaptureEvent::Done(key, mods) => {
                self.capture = None;
                let lone_escape = key == declic_core::Key::Escape && mods.is_empty();
                match target {
                    CaptureTarget::Editor => {
                        // A lone Esc cancels the recording (it remains available through manual entry).
                        if !lone_escape && let Some(ed) = self.editor.as_mut() {
                            ed.set_hotkey(Some(Hotkey::from_state(key, mods, ed.sided)));
                        }
                        self.refresh_reserved();
                    }
                    CaptureTarget::Search => {
                        if !lone_escape {
                            self.search_keys = Some(Hotkey::from_state(key, mods, false));
                        }
                    }
                    CaptureTarget::Setting(setting) => {
                        let hk = Hotkey::from_state(key, mods, false);
                        match setting {
                            SettingKey::Stop => self.config.settings.stop_key = hk,
                            SettingKey::OpenWindow if !lone_escape => self.config.settings.open_window_keys = Some(hk),
                            SettingKey::CheatSheet if !lone_escape => self.config.settings.cheat_sheet_keys = Some(hk),
                            _ => return Task::none(),
                        }
                        return self.persist();
                    }
                    CaptureTarget::StepKeys(i) => {
                        let hk = Hotkey::from_state(key, mods, true);
                        if let Some(draft) = self.editor.as_mut().and_then(|e| e.steps.get_mut(i))
                            && let Step::PressKeys { keys, .. } = &mut draft.step
                        {
                            *keys = hk;
                        }
                    }
                    CaptureTarget::StepHold(i) => {
                        finish_hold(self, i, KeyList::from_hotkey(&Hotkey::from_state(key, mods, true)));
                    }
                }
            }
        }
        Task::none()
    }

    fn on_tick(&mut self) -> Task<Message> {
        if self.undo.as_ref().is_some_and(|u| u.since.elapsed().as_secs() >= UNDO_SECONDS) {
            self.undo = None;
        }
        if self.toast.as_ref().is_some_and(|(_, _, since)| since.elapsed().as_secs() >= TOAST_SECONDS) {
            self.toast = None;
        }
        if self.capture.is_some() && self.capture_started.elapsed().as_secs() >= CAPTURE_TIMEOUT_SECONDS {
            self.capture = None;
        }
        let mut tasks = Vec::new();
        if self.test_countdown.as_ref().is_some_and(|(deadline, _)| Instant::now() >= *deadline)
            && let Some((_, action)) = self.test_countdown.take()
        {
            // Minimise the window so that the previous one gets the focus back,
            // then run the action.
            tasks.push(self.minimize(true));
            tasks.push(Task::perform(
                async {
                    std::thread::sleep(Duration::from_millis(400));
                },
                move |_| Message::RunTest(action.clone()),
            ));
        }
        let stamp = file_stamp(&self.paths.config);
        if stamp.is_some() && stamp != self.file_stamp {
            tasks.push(self.reload_from_disk());
        }
        let stats_stamp = file_stamp(&self.paths.stats_file());
        if stats_stamp != self.stats_stamp {
            self.stats = Stats::load(&self.paths.stats_file());
            self.stats_stamp = stats_stamp;
        }
        self.service_running = daemon::is_running();
        self.refresh_theme(false);
        Task::batch(tasks)
    }

    fn save_editor(&mut self) -> Task<Message> {
        if self.read_only() {
            return Task::none();
        }
        let Some(ed) = self.editor.as_mut() else { return Task::none() };
        let id = ed.id.unwrap_or_else(|| self.config.next_id());
        let Some(mut shortcut) = ed.to_shortcut(id) else {
            ed.show_errors = true;
            return Task::none();
        };
        if !conflict::validate(&shortcut).is_empty() {
            ed.show_errors = true;
            return Task::none();
        }
        if shortcut.name.is_empty() {
            shortcut.name = editor::suggested_name(&self.tr, &shortcut.action).unwrap_or_default();
        }
        self.config.upsert(shortcut);
        self.selected = Some(id);
        self.editor = None;
        self.capture = None;
        self.persist()
    }

    fn add_example(&mut self, example: Example) -> Task<Message> {
        if self.read_only() {
            return Task::none();
        }
        let tr = &self.tr;
        let (keys, action, name) = match example {
            Example::Email => (
                "Ctrl+Alt+M",
                Action::text(tr.get("empty.example_email_text"), TextMode::Typing),
                tr.get("empty.example_email"),
            ),
            Example::Notepad => (
                "Ctrl+Alt+N",
                Action::steps([Step::ActivateOrLaunch {
                    title: String::new(),
                    program: "notepad.exe".into(),
                    timeout_ms: declic_core::DEFAULT_WINDOW_TIMEOUT_MS,
                    if_missing: declic_core::IfMissing::Stop,
                    launch: declic_core::OpenAction { target: "notepad.exe".into(), ..Default::default() },
                }]),
                tr.get("empty.example_notepad"),
            ),
            Example::Date => ("Ctrl+Alt+D", Action::text("%DATE%", TextMode::Typing), tr.get("empty.example_date")),
            Example::Documents => {
                ("Ctrl+Alt+E", Action::open(r"%USERPROFILE%\Documents"), tr.get("empty.example_documents"))
            }
        };
        let Ok(hotkey) = keys.parse::<Hotkey>() else { return Task::none() };
        let mut shortcut = Shortcut::new(self.config.next_id(), hotkey, action);
        shortcut.name = name.to_string();
        let id = shortcut.id;
        self.config.upsert(shortcut);
        let task = self.persist();
        if let Some(s) = self.config.get(id) {
            let ed = Editor::from_shortcut(s, &self.tr);
            self.selected = Some(id);
            self.open_editor(ed);
        }
        task
    }

    fn bulk(&mut self, action: Bulk) -> Task<Message> {
        if self.read_only() || self.selection.is_empty() {
            return Task::none();
        }
        let ids: Vec<ShortcutId> = self.selection.iter().copied().collect();
        match action {
            Bulk::Delete => {
                let mut items = Vec::new();
                for (index, s) in self.config.shortcuts.iter().enumerate() {
                    if ids.contains(&s.id) {
                        items.push((index, s.clone()));
                    }
                }
                self.config.shortcuts.retain(|s| !ids.contains(&s.id));
                if self.editor.as_ref().is_some_and(|e| e.id.is_some_and(|id| ids.contains(&id))) {
                    self.editor = None;
                }
                self.undo = Some(Undo { items, since: Instant::now() });
                self.selection.clear();
            }
            Bulk::Enable | Bulk::Disable => {
                for s in self.config.shortcuts.iter_mut().filter(|s| ids.contains(&s.id)) {
                    s.enabled = action == Bulk::Enable;
                }
            }
            Bulk::Duplicate => {
                let copies: Vec<Shortcut> = self.config.shortcuts.iter().filter(|s| ids.contains(&s.id)).cloned().collect();
                let suffix = self.tr.get("main.copy_suffix").to_string();
                let mut new_ids = BTreeSet::new();
                for mut copy in copies {
                    copy.name = format!("{} {suffix}", self.display_name(&copy));
                    copy.id = self.config.next_id();
                    // Copies are disabled so that they never create a silent conflict.
                    copy.enabled = false;
                    new_ids.insert(copy.id);
                    self.config.shortcuts.push(copy);
                }
                self.selection = new_ids;
            }
            Bulk::MoveTo(group) => {
                let group = self.config.ensure_group(&group).unwrap_or_default();
                for s in self.config.shortcuts.iter_mut().filter(|s| ids.contains(&s.id)) {
                    s.group = group.clone();
                }
            }
            Bulk::Export => {
                self.panel = Panel::Transfer;
                self.editor = None;
                self.transfer.selection_only = true;
                return Task::none();
            }
            Bulk::ResetStats => {
                for id in ids {
                    let _ = self.handle(Message::ResetStats(Some(id)));
                }
                return Task::none();
            }
        }
        self.persist()
    }

    fn pick_file(&self, purpose: PickPurpose) -> Task<Message> {
        use declic_win::dialogs::PickKind;
        let (kind, title) = match purpose {
            PickPurpose::TargetFile | PickPurpose::StepTarget(_) => (PickKind::File, self.tr.get("dialog.pick_file")),
            PickPurpose::TargetFolder => (PickKind::Folder, self.tr.get("dialog.pick_folder")),
            PickPurpose::Program => (PickKind::Program, self.tr.get("dialog.pick_program")),
        };
        let title = title.to_string();
        let programs = self.tr.get("dialog.programs").to_string();
        let all = self.tr.get("dialog.all_files").to_string();
        blocking(move || declic_win::dialogs::pick(kind, &title, &programs, &all), move |path| Message::Picked(purpose, path))
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            capture::ticks(if self.test_countdown.is_some() { 250 } else { 1000 }).map(|_| Message::Tick),
            iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    use keyboard::key::Named;
                    match key.as_ref() {
                        keyboard::Key::Named(Named::Escape) => Some(Message::Escape),
                        keyboard::Key::Named(Named::ArrowUp) if modifiers.alt() => Some(Message::MoveStep(-1)),
                        keyboard::Key::Named(Named::ArrowDown) if modifiers.alt() => Some(Message::MoveStep(1)),
                        keyboard::Key::Character("s") if modifiers.command() => Some(Message::Save),
                        keyboard::Key::Character("n") if modifiers.command() => Some(Message::New),
                        keyboard::Key::Character("f") if modifiers.command() => Some(Message::FocusSearch),
                        _ => None,
                    }
                }
                iced::Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => Some(Message::ModifiersChanged(m)),
                iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
                iced::Event::Window(window::Event::Unfocused) => Some(Message::WindowUnfocused),
                iced::Event::Window(window::Event::Resized(size)) => Some(Message::WindowResized(size.width)),
                iced::Event::Window(window::Event::Opened { size, .. }) => Some(Message::WindowResized(size.width)),
                iced::Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested),
                _ => None,
            }),
        ];
        if self.capture.is_some() {
            subs.push(capture::subscription().map(Message::Capture));
        }
        if self.pick.is_some() {
            let hint = self.tr.get("pick.hint").to_string();
            subs.push(capture::pick(hint).map(Message::PickResult));
        }
        Subscription::batch(subs)
    }

    // ------------------------------------------------------------------ view

    fn view(&self) -> Element<'_, Message> {
        let header = list::header(self);
        let mut body = column![].spacing(12).width(Length::Fill);
        for bar in list::info_bars(self) {
            body = body.push(bar);
        }
        if self.config.shortcuts.is_empty() && self.load_error.is_none() {
            body = body.push(list::empty_state(self));
        } else {
            body = body.push(list::toolbar(self)).push(list::list(self));
        }
        let mut main = Row::new().spacing(16).height(Length::Fill);
        if let Some(width) = self.sidebar_width() {
            main = main.push(list::sidebar(self, width));
        }
        main = main.push(body);
        if let Some(ed) = &self.editor {
            let mut panel = column![].spacing(8).height(Length::Fill);
            if self.pending.is_some() {
                panel = panel.push(list::confirm_bar(self));
            }
            panel = panel.push(editor::view(self, ed));
            main = main.push(panel);
        } else {
            match self.panel {
                Panel::Settings => main = main.push(settings::view(self)),
                Panel::Transfer => main = main.push(transfer::view(self)),
                Panel::None => {}
            }
        }
        widgets::directional(
            container(column![header, container(main).padding(Padding::from([4.0, 20.0]).bottom(20.0)).height(Length::Fill)])
                .width(Length::Fill)
                .height(Length::Fill),
        )
    }

    fn panel_open(&self) -> bool {
        self.editor.is_some() || self.panel != Panel::None
    }

    /// Width of the sidebar: narrower, then hidden, when a panel is open in
    /// a small window, so that the list keeps room for the shortcut names.
    fn sidebar_width(&self) -> Option<f32> {
        if !self.panel_open() || self.window_width >= WIDE_WINDOW {
            Some(210.0)
        } else if self.window_width >= NARROW_WINDOW {
            Some(170.0)
        } else {
            None
        }
    }

    /// Width of the editor panel.
    fn editor_width(&self) -> f32 {
        if self.window_width >= WIDE_WINDOW { 480.0 } else { 440.0 }
    }

    /// Width of the settings and import/export panels.
    fn panel_width(&self) -> f32 {
        if self.window_width >= WIDE_WINDOW { 420.0 } else { 390.0 }
    }

    /// Shortcuts shown by the list, filtered and sorted.
    fn visible_shortcuts(&self) -> Vec<&Shortcut> {
        let query = self.search.trim();
        let mut shortcuts: Vec<&Shortcut> = self
            .config
            .shortcuts
            .iter()
            .filter(|s| match &self.group_filter {
                GroupFilter::All => true,
                GroupFilter::Favorites => s.favorite,
                GroupFilter::Group(g) => &s.group == g,
            })
            .filter(|s| self.kind_filter.is_none_or(|k| s.action.kind() == k))
            .filter(|s| self.search_keys.is_none_or(|hk| hk.overlaps(&s.keys)))
            .filter(|s| {
                query.is_empty() || {
                    let displayed = tr::hotkey_text(&self.tr, &s.keys);
                    let mut hay = declic_core::search::haystack(s, &displayed);
                    hay.push('\n');
                    hay.push_str(&declic_core::search::fold(&self.display_name(s)));
                    declic_core::search::matches(&hay, query)
                }
            })
            .collect();
        match self.sort {
            SortKey::Created => {}
            SortKey::Name => shortcuts.sort_by_cached_key(|s| declic_core::search::fold(&self.display_name(s))),
            SortKey::Keys => shortcuts.sort_by_cached_key(|s| tr::hotkey_text(&self.tr, &s.keys).to_lowercase()),
            SortKey::Kind => shortcuts.sort_by_key(|s| match s.action.kind() {
                ActionKind::Open => 0,
                ActionKind::Text => 1,
                ActionKind::Macro => 2,
            }),
            SortKey::Uses => shortcuts.sort_by_key(|s| std::cmp::Reverse(self.stats.get(s.id).count)),
            SortKey::LastUsed => shortcuts.sort_by_key(|s| std::cmp::Reverse(self.stats.get(s.id).last_used)),
        }
        shortcuts
    }
}
