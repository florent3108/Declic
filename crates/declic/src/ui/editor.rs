//! Side-panel shortcut editor (§ 8.3): state and view.

use super::icons::{self, Icon};
use super::steps::{self, StepDraft, StepMsg};
use super::style::{self, Tone};
use super::widgets::{self, ButtonKind, Choice, font_semibold};
use super::{App, CaptureTarget, Message, PickPurpose, PickTarget};
use crate::tr;
use declic_core::conflict::{self, Unavailable, ValidationError, Warning};
use declic_core::i18n::Catalog;
use declic_core::naming::{self, NameHint};
use declic_core::{
    Action, Conditions, Hotkey, Key, LockReq, Macro, MacroStep, ModReq, Modifier, OpenAction, ProgramMode, Shortcut,
    ShortcutId, Step, StepKind, TextMode, WindowState, normalize_program,
};
use iced::widget::{
    Column, button, checkbox, column, combo_box, container, image, pick_list, row, scrollable, space, text, text_editor,
    text_input, toggler,
};
use iced::{Alignment, Element, Length, Padding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Open,
    Text,
    Macro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockKind {
    Caps,
    Num,
    Scroll,
}

/// Messages of the editor.
#[derive(Debug, Clone)]
pub enum EdMsg {
    Kind(EditKind),
    Target(String),
    Arguments(String),
    WorkingDir(String),
    Window(WindowState),
    Admin(bool),
    Text(text_editor::Action),
    TextMode(TextMode),
    InsertVar(&'static str),
    ConvertToMacro,
    Name(String),
    Group(String),
    Favorite(bool),
    Enabled(bool),
    Sided(bool),
    ToggleManual,
    ManualMod(Modifier, bool),
    ManualKey(Choice<Key>),
    ToggleAdvanced,
    ProgramMode(ProgramMode),
    ProgramInput(String),
    AddProgram,
    AddRunning(String),
    RemoveProgram(usize),
    RefreshRunning,
    Lock(LockKind, LockReq),
    Step(usize, StepMsg),
    AddStep(StepKind),
    ToggleAddMenu,
    /// A step started being dragged by its handle.
    StepDragStart(usize),
    /// A dragged step was dropped at a new index (from, to).
    StepDrop(usize, usize),
    /// A drag ended without change.
    StepDragCancel,
    ToggleApps,
    AppsFilter(String),
}

/// State of the shortcut being edited.
pub struct Editor {
    pub id: Option<ShortcutId>,
    /// The shortcut as it was when the editor opened (to detect changes).
    pub original: Option<Shortcut>,
    pub hotkey: Option<Hotkey>,
    pub sided: bool,
    pub kind: EditKind,
    pub target: String,
    pub arguments: String,
    pub working_dir: String,
    pub window: WindowState,
    pub run_as_admin: bool,
    pub text: text_editor::Content,
    pub text_mode: TextMode,
    pub steps: Vec<StepDraft>,
    pub selected_step: Option<usize>,
    /// Step being dragged in the block editor.
    pub dragging: Option<usize>,
    pub add_menu_open: bool,
    pub name: String,
    pub group: String,
    pub favorite: bool,
    pub enabled: bool,
    pub conditions: Conditions,
    pub program_input: String,
    pub advanced_open: bool,
    pub manual_open: bool,
    pub manual_mods: [bool; 4],
    pub manual_key: Option<Key>,
    pub key_choices: combo_box::State<Choice<Key>>,
    pub lnk_target: Option<String>,
    pub show_errors: bool,
    pub apps_open: bool,
    pub apps_filter: String,
}

pub fn key_choices(tr: &Catalog) -> Vec<Choice<Key>> {
    Key::ALL
        .iter()
        .map(|&k| {
            let label = tr::key_label(tr, k);
            let id = k.id();
            let label = if label == id { label } else { format!("{label} ({id})") };
            Choice::new(k, label)
        })
        .collect()
}

impl Editor {
    pub fn new(tr: &Catalog, default_mode: TextMode) -> Editor {
        Editor {
            id: None,
            original: None,
            hotkey: None,
            sided: false,
            kind: EditKind::Text,
            target: String::new(),
            arguments: String::new(),
            working_dir: String::new(),
            window: WindowState::Normal,
            run_as_admin: false,
            text: text_editor::Content::new(),
            text_mode: default_mode,
            steps: Vec::new(),
            selected_step: None,
            dragging: None,
            add_menu_open: false,
            name: String::new(),
            group: String::new(),
            favorite: false,
            enabled: true,
            conditions: Conditions::default(),
            program_input: String::new(),
            advanced_open: false,
            manual_open: false,
            manual_mods: [false; 4],
            manual_key: None,
            key_choices: combo_box::State::new(key_choices(tr)),
            lnk_target: None,
            show_errors: false,
            apps_open: false,
            apps_filter: String::new(),
        }
    }

    pub fn from_shortcut(shortcut: &Shortcut, tr: &Catalog) -> Editor {
        let mut editor = Editor::new(tr, TextMode::Typing);
        editor.id = Some(shortcut.id);
        editor.original = Some(shortcut.clone());
        editor.set_hotkey(Some(shortcut.keys));
        editor.sided = shortcut.keys.is_sided();
        editor.name = shortcut.name.clone();
        editor.group = shortcut.group.clone();
        editor.favorite = shortcut.favorite;
        editor.enabled = shortcut.enabled;
        editor.conditions = shortcut.conditions.clone();
        editor.advanced_open = !shortcut.conditions.is_default();
        match &shortcut.action {
            Action::Open(open) => {
                editor.kind = EditKind::Open;
                editor.target = open.target.clone();
                editor.arguments = open.arguments.clone();
                editor.working_dir = open.working_dir.clone();
                editor.window = open.window;
                editor.run_as_admin = open.run_as_admin;
                editor.refresh_lnk();
            }
            Action::Macro(m) => match m.as_text() {
                Some((text, mode)) => {
                    editor.kind = EditKind::Text;
                    editor.text = text_editor::Content::with_text(text);
                    editor.text_mode = mode;
                }
                None => {
                    editor.kind = EditKind::Macro;
                    editor.steps = m.steps.iter().map(StepDraft::from_step).collect();
                }
            },
        }
        editor
    }

    pub fn set_hotkey(&mut self, hotkey: Option<Hotkey>) {
        self.hotkey = hotkey;
        if let Some(hk) = hotkey {
            for (i, m) in Modifier::ALL.iter().enumerate() {
                self.manual_mods[i] = hk.req(*m) != ModReq::Off;
            }
            self.manual_key = Some(hk.key);
        }
    }

    fn apply_manual(&mut self) {
        if let Some(key) = self.manual_key {
            let mut hk = Hotkey::new(key);
            for (i, m) in Modifier::ALL.iter().enumerate() {
                if self.manual_mods[i] {
                    let previous = self.hotkey.map(|h| h.req(*m)).unwrap_or(ModReq::Off);
                    let req = match previous {
                        ModReq::Left | ModReq::Right if self.sided => previous,
                        _ if self.sided => ModReq::Left,
                        _ => ModReq::Any,
                    };
                    hk.set_req(*m, req);
                }
            }
            self.hotkey = Some(hk);
        }
    }

    pub fn refresh_lnk(&mut self) {
        let target = self.target.trim().trim_matches('"');
        self.lnk_target = if target.to_ascii_lowercase().ends_with(".lnk") {
            declic_win::launch::resolve_shortcut(std::path::Path::new(target))
        } else {
            None
        };
    }

    fn text_value(&self) -> String {
        let mut text = self.text.text();
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        text
    }

    /// The action as edited; fails on an invalid number in a macro step.
    pub fn build_action(&self) -> Result<Action, steps::BadNumber> {
        Ok(match self.kind {
            EditKind::Open => Action::Open(OpenAction {
                target: self.target.trim().to_string(),
                arguments: self.arguments.trim().to_string(),
                working_dir: self.working_dir.trim().to_string(),
                window: self.window,
                run_as_admin: self.run_as_admin,
            }),
            EditKind::Text => Action::text(self.text_value(), self.text_mode),
            EditKind::Macro => {
                let steps: Result<Vec<MacroStep>, _> = self.steps.iter().enumerate().map(|(i, s)| s.build(i)).collect();
                Action::Macro(Macro { steps: steps? })
            }
        })
    }

    /// The shortcut as currently edited (`None` until a combination is
    /// chosen, or while a number is invalid).
    pub fn to_shortcut(&self, id: ShortcutId) -> Option<Shortcut> {
        let keys = self.hotkey?;
        let mut shortcut = Shortcut::new(id, keys, self.build_action().ok()?);
        shortcut.name = self.name.trim().to_string();
        shortcut.group = self.group.clone();
        shortcut.favorite = self.favorite;
        shortcut.enabled = self.enabled;
        shortcut.conditions = self.conditions.clone();
        if shortcut.conditions.program_mode == ProgramMode::Everywhere {
            shortcut.conditions.programs.clear();
        }
        Some(shortcut)
    }

    /// Whether the editor holds changes that are not saved.
    pub fn is_dirty(&self) -> bool {
        match &self.original {
            Some(original) => {
                let mut current = self.to_shortcut(original.id);
                if let Some(c) = current.as_mut()
                    && c.name.is_empty()
                {
                    // An empty name means "suggested name": compare with it.
                    c.name = original.name.clone();
                }
                current.as_ref() != Some(original)
            }
            None => {
                self.hotkey.is_some()
                    || !self.text_value().is_empty()
                    || !self.target.trim().is_empty()
                    || !self.steps.is_empty()
                    || !self.name.trim().is_empty()
            }
        }
    }

    pub fn add_program(&mut self, input: &str) {
        if let Some(program) = normalize_program(input)
            && !self.conditions.programs.contains(&program)
        {
            self.conditions.programs.push(program);
        }
    }

    /// Text → macro: the text becomes the first step.
    fn convert_to_macro(&mut self) {
        if self.kind == EditKind::Macro {
            return;
        }
        if self.kind == EditKind::Text && !self.text_value().is_empty() {
            let step = MacroStep::new(Step::TypeText { text: self.text_value(), mode: self.text_mode });
            self.steps.insert(0, StepDraft::from_step(&step));
        }
        self.kind = EditKind::Macro;
        self.selected_step = self.steps.len().checked_sub(1);
    }

    /// Handles an editor message.
    pub fn update(&mut self, msg: EdMsg, default_mode: TextMode) {
        match msg {
            EdMsg::Kind(EditKind::Macro) => {
                self.convert_to_macro();
                if self.steps.is_empty() {
                    self.add_menu_open = true;
                }
            }
            EdMsg::Kind(EditKind::Text) if self.kind == EditKind::Macro => {
                // Keep the text of a single "type text" step, if any.
                if let Some(first) = self.steps.first()
                    && let Ok(MacroStep { step: Step::TypeText { text, mode }, .. }) = first.build(0)
                {
                    self.text = text_editor::Content::with_text(&text);
                    self.text_mode = mode;
                }
                self.kind = EditKind::Text;
            }
            EdMsg::Kind(kind) => self.kind = kind,
            EdMsg::ConvertToMacro => {
                self.convert_to_macro();
                self.add_menu_open = true;
            }
            EdMsg::Target(t) => {
                self.target = t;
                self.refresh_lnk();
            }
            EdMsg::Arguments(a) => self.arguments = a,
            EdMsg::WorkingDir(d) => self.working_dir = d,
            EdMsg::Window(w) => self.window = w,
            EdMsg::Admin(a) => self.run_as_admin = a,
            EdMsg::Text(action) => self.text.perform(action),
            EdMsg::TextMode(mode) => self.text_mode = mode,
            EdMsg::InsertVar(var) => {
                self.text.perform(text_editor::Action::Edit(text_editor::Edit::Paste(std::sync::Arc::new(var.to_string()))));
            }
            EdMsg::Name(n) => self.name = n,
            EdMsg::Group(g) => self.group = g,
            EdMsg::Favorite(f) => self.favorite = f,
            EdMsg::Enabled(e) => self.enabled = e,
            EdMsg::Sided(sided) => {
                self.sided = sided;
                self.hotkey = self.hotkey.map(|hk| if sided { hk.sided_default_left() } else { hk.unsided() });
            }
            EdMsg::ToggleManual => self.manual_open = !self.manual_open,
            EdMsg::ManualMod(m, on) => {
                if let Some(i) = Modifier::ALL.iter().position(|x| *x == m) {
                    self.manual_mods[i] = on;
                }
                self.apply_manual();
            }
            EdMsg::ManualKey(choice) => {
                self.manual_key = Some(choice.value);
                self.apply_manual();
            }
            EdMsg::ToggleAdvanced => self.advanced_open = !self.advanced_open,
            EdMsg::ProgramMode(mode) => self.conditions.program_mode = mode,
            EdMsg::ProgramInput(p) => self.program_input = p,
            EdMsg::AddProgram => {
                let input = std::mem::take(&mut self.program_input);
                self.add_program(&input);
            }
            EdMsg::AddRunning(p) => self.add_program(&p),
            EdMsg::RemoveProgram(i) => {
                if i < self.conditions.programs.len() {
                    self.conditions.programs.remove(i);
                }
            }
            EdMsg::RefreshRunning => {}
            EdMsg::Lock(kind, req) => match kind {
                LockKind::Caps => self.conditions.caps_lock = req,
                LockKind::Num => self.conditions.num_lock = req,
                LockKind::Scroll => self.conditions.scroll_lock = req,
            },
            EdMsg::Step(index, msg) => steps::update(self, index, msg),
            EdMsg::AddStep(kind) => steps::add_step(self, kind, default_mode),
            EdMsg::ToggleAddMenu => self.add_menu_open = !self.add_menu_open,
            EdMsg::StepDragStart(index) => steps::start_drag(self, index),
            EdMsg::StepDrop(from, to) => steps::drop_step(self, from, to),
            EdMsg::StepDragCancel => self.dragging = None,
            EdMsg::ToggleApps => self.apps_open = !self.apps_open,
            EdMsg::AppsFilter(f) => self.apps_filter = f,
        }
    }
}

/// Suggested name for an action ("Ouvrir Chrome", "Saisir « … »").
pub fn suggested_name(tr: &Catalog, action: &Action) -> Option<String> {
    naming::suggest(action).map(|hint| match hint {
        NameHint::Open(subject) => tr.fmt("name.open", &[("subject", &subject)]),
        NameHint::Type(excerpt) => tr.fmt("name.type", &[("excerpt", &excerpt)]),
        NameHint::Macro(steps) => tr.plural("name.macro", steps as i64, &[]),
    })
}

fn em(msg: EdMsg) -> Message {
    Message::Ed(msg)
}

fn segment<'a>(label: &'a str, selected: bool, message: Message) -> Element<'a, Message> {
    button(text(label).size(13))
        .padding(Padding::from([6.0, 12.0]))
        .style(style::button_segment(selected))
        .on_press(message)
        .into()
}

fn kind_card<'a>(icon: Icon, title: &'a str, selected: bool, message: Message) -> Element<'a, Message> {
    button(
        column![
            icons::icon(icon, 20.0, Some(if selected { icons::accent } else { icons::secondary })),
            text(title).size(13).font(font_semibold()),
        ]
        .spacing(4)
        .align_x(Alignment::Center),
    )
    .padding(Padding::from([8.0, 6.0]))
    .width(Length::Fill)
    .style(style::button_segment(selected))
    .on_press(message)
    .into()
}

fn chip<'a>(label: &'a str, var: &'static str) -> Element<'a, Message> {
    button(text(label).size(12))
        .padding(Padding::from([3.0, 9.0]))
        .style(style::button_segment(false))
        .on_press(em(EdMsg::InsertVar(var)))
        .into()
}

/// Short description of non-default conditions.
pub fn summary_conditions(tr: &Catalog, c: &Conditions) -> String {
    let mut parts = Vec::new();
    let programs = c.programs.join(", ");
    match c.program_mode {
        ProgramMode::Everywhere => {}
        _ if programs.is_empty() => {}
        ProgramMode::OnlyIn => parts.push(tr.fmt("row.only_in", &[("programs", &programs)])),
        ProgramMode::Except => parts.push(tr.fmt("row.except", &[("programs", &programs)])),
    }
    if c.lock_count() > 0 {
        parts.push(tr.get("row.locks").to_string());
    }
    parts.join(" · ")
}

/// Identifier of the editor's scrollable area (reset when another shortcut is opened).
pub fn scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("editor-scroll")
}

fn keys_section<'a>(app: &'a App, ed: &'a Editor, draft: Option<&Shortcut>, errors: &[ValidationError]) -> Column<'a, Message> {
    let tr = &app.tr;
    let capturing = app.capture == Some(CaptureTarget::Editor);
    let show = ed.show_errors;
    let capture_content: Element<'a, Message> = if capturing {
        let mut r = row![].spacing(10).align_y(Alignment::Center);
        if !app.capture_mods.is_empty() {
            let partial = Hotkey::from_state(Key::Other(0), app.capture_mods, ed.sided);
            let mut caps = tr::keycaps(tr, &partial);
            caps.pop();
            r = r.push(widgets::keycaps(caps, 13.0, true));
        }
        r.push(text(tr.get("editor.capture_prompt")).size(13).style(style::text_accent)).into()
    } else if let Some(hk) = &ed.hotkey {
        widgets::keycaps(tr::keycaps(tr, hk), 14.0, false)
    } else {
        text(tr.get("editor.capture_empty")).size(13).style(style::text_tertiary).into()
    };
    let capture_box = button(
        row![icons::icon(Icon::Keyboard, 18.0, Some(icons::secondary)), capture_content]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::from([12.0, 14.0]))
    .style(style::capture_box(capturing))
    .on_press(if capturing { Message::CancelCapture } else { Message::StartCapture(CaptureTarget::Editor) });

    let mut section = Column::new().spacing(8).push(widgets::section_title(tr.get("editor.step_keys"))).push(capture_box);
    if capturing {
        section = section.push(
            row![
                text(tr.get("editor.capture_hint")).size(12).style(style::text_tertiary).width(Length::Fill),
                widgets::text_button(tr.get("editor.capture_cancel"), Some(Message::CancelCapture), style::button_link)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    section = section.push(
        row![
            checkbox(ed.sided).label(tr.get("editor.sided")).on_toggle(|v| em(EdMsg::Sided(v))).size(16).text_size(13).style(style::check),
            space::horizontal(),
            button(
                row![
                    icons::icon(if ed.manual_open { Icon::ChevronDown } else { Icon::Chevron }, 14.0, Some(icons::accent)),
                    text(tr.get("editor.manual")).size(13)
                ]
                .spacing(4)
                .align_y(Alignment::Center)
            )
            .padding(Padding::from([4.0, 6.0]))
            .style(style::button_link)
            .on_press(em(EdMsg::ToggleManual)),
        ]
        .align_y(Alignment::Center),
    );
    if ed.manual_open {
        let mod_labels = [tr.get("modifiers.ctrl"), tr.get("modifiers.alt"), tr.get("modifiers.shift"), tr.get("modifiers.win")];
        let mut mods = row![].spacing(12);
        for (i, m) in Modifier::ALL.iter().enumerate() {
            let m = *m;
            mods = mods.push(
                checkbox(ed.manual_mods[i])
                    .label(mod_labels[i])
                    .on_toggle(move |v| em(EdMsg::ManualMod(m, v)))
                    .size(16)
                    .text_size(13)
                    .style(style::check),
            );
        }
        let selected = ed.manual_key.map(|k| Choice::new(k, tr::key_label(tr, k)));
        let keys = combo_box(&ed.key_choices, tr.get("editor.manual_key"), selected.as_ref(), |c| em(EdMsg::ManualKey(c)))
            .width(Length::Fill)
            .padding(Padding::from([6.0, 10.0]))
            .size(13.0)
            .input_style(style::input)
            .menu_style(style::menu);
        section = section.push(
            container(column![widgets::hint(tr.get("editor.manual_hint")), mods, keys].spacing(8)).padding(10).style(style::card),
        );
    }
    if show && ed.hotkey.is_none() {
        section = section.push(widgets::error(tr.get("editor.missing_keys")));
    }
    for e in errors {
        if let ValidationError::Unavailable(reason) = e {
            section = section.push(widgets::error(tr.get(match reason {
                Unavailable::SecureAttention => "editor.unavailable_secure",
                Unavailable::LockWorkstation => "editor.unavailable_lock",
            })));
        }
    }
    if let Some(hk) = &ed.hotkey
        && app.reserved.contains(hk)
    {
        section = section.push(widgets::infobar(Icon::Warning, Tone::Warning, text(tr.get("editor.reserved")).size(12.5), None));
    }
    if let Some(draft) = draft {
        let others: Vec<Shortcut> = app.config.shortcuts.iter().filter(|s| Some(s.id) != ed.id).cloned().collect();
        for id in conflict::find_conflicts(draft, &others) {
            if let Some(other) = app.config.get(id) {
                let name = app.display_name(other);
                section = section.push(widgets::infobar(
                    Icon::Warning,
                    Tone::Warning,
                    text(tr.fmt("editor.conflict", &[("name", &name)])).size(12.5),
                    Some(widgets::text_button(tr.get("editor.conflict_open"), Some(Message::Select(id)), style::button_secondary).into()),
                ));
            }
        }
        for w in conflict::warnings(draft) {
            let key = match w {
                Warning::NoModifier => "editor.warn_no_modifier",
                Warning::ShiftOnly => "editor.warn_shift_only",
                Warning::CommonShortcut => "editor.warn_common",
                Warning::AltGrCharacter => "editor.warn_altgr",
            };
            section = section.push(widgets::infobar(Icon::Info, Tone::Warning, text(tr.get(key)).size(12.5), None));
        }
    }
    section
}

fn apps_panel<'a>(app: &'a App, ed: &'a Editor) -> Element<'a, Message> {
    let tr = &app.tr;
    let content: Element<'a, Message> = match &app.installed_apps {
        None => widgets::hint(tr.get("editor.apps_loading")),
        Some(apps) => {
            let filter = declic_core::search::fold(ed.apps_filter.trim());
            let rows: Vec<Element<'a, Message>> = apps
                .iter()
                .filter(|a| filter.is_empty() || declic_core::search::fold(&a.name).contains(&filter))
                .take(200)
                .map(|a| {
                    let icon: Element<'a, Message> = match &a.icon {
                        Some(handle) => image(handle.clone()).width(Length::Fixed(20.0)).height(Length::Fixed(20.0)).into(),
                        None => icons::icon(Icon::Window, 18.0, Some(icons::secondary)).into(),
                    };
                    button(row![icon, text(a.name.clone()).size(13)].spacing(10).align_y(Alignment::Center))
                        .padding(Padding::from([5.0, 8.0]))
                        .width(Length::Fill)
                        .style(style::button_ghost)
                        .on_press(Message::AppChosen(a.path.clone()))
                        .into()
                })
                .collect();
            scrollable(Column::with_children(rows).spacing(1)).height(Length::Fixed(220.0)).style(style::scroll).into()
        }
    };
    container(
        column![
            text_input(tr.get("editor.apps_search"), &ed.apps_filter)
                .on_input(|v| em(EdMsg::AppsFilter(v)))
                .padding(Padding::from([6.0, 10.0]))
                .size(13)
                .style(style::input),
            content,
        ]
        .spacing(6),
    )
    .padding(8)
    .style(style::card)
    .into()
}

fn open_details<'a>(app: &'a App, ed: &'a Editor, errors: &[ValidationError]) -> Element<'a, Message> {
    let tr = &app.tr;
    let mut c = Column::new().spacing(12);
    c = c.push(
        column![
            widgets::field_label(tr.get("editor.target")),
            text_input(tr.get("editor.target_placeholder"), &ed.target)
                .on_input(|v| em(EdMsg::Target(v)))
                .padding(Padding::from([6.0, 10.0]))
                .size(13)
                .style(style::input),
            row![
                widgets::labeled_button(Icon::Apps, tr.get("editor.apps"), Some(em(EdMsg::ToggleApps)), ButtonKind::Secondary),
                widgets::labeled_button(Icon::File, tr.get("editor.browse_file"), Some(Message::Pick(PickPurpose::TargetFile)), ButtonKind::Secondary),
                widgets::labeled_button(Icon::Folder, tr.get("editor.browse_folder"), Some(Message::Pick(PickPurpose::TargetFolder)), ButtonKind::Secondary),
            ]
            .spacing(6),
            widgets::hint(tr.get("editor.target_hint")),
        ]
        .spacing(6),
    );
    if ed.apps_open {
        c = c.push(apps_panel(app, ed));
    }
    if let Some(lnk) = &ed.lnk_target {
        c = c.push(widgets::hint(tr.fmt("editor.lnk_target", &[("target", lnk)])));
    }
    if ed.show_errors && errors.contains(&ValidationError::MissingTarget) {
        c = c.push(widgets::error(tr.get("editor.missing_target")));
    }
    c = c.push(widgets::field(
        tr.get("editor.arguments"),
        text_input("", &ed.arguments).on_input(|v| em(EdMsg::Arguments(v))).padding(Padding::from([6.0, 10.0])).size(13).style(style::input),
    ));
    c = c.push(widgets::field(
        tr.get("editor.working_dir"),
        text_input(tr.get("editor.working_dir_placeholder"), &ed.working_dir)
            .on_input(|v| em(EdMsg::WorkingDir(v)))
            .padding(Padding::from([6.0, 10.0]))
            .size(13)
            .style(style::input),
    ));
    let states: Vec<Choice<WindowState>> = WindowState::ALL
        .iter()
        .map(|&w| {
            Choice::new(w, tr.get(match w {
                WindowState::Normal => "editor.window_state.normal",
                WindowState::Minimized => "editor.window_state.minimized",
                WindowState::Maximized => "editor.window_state.maximized",
            }))
        })
        .collect();
    let current = states.iter().find(|c| c.value == ed.window).cloned();
    c = c.push(
        row![
            text(tr.get("editor.window")).size(13).width(Length::Fixed(110.0)),
            pick_list(states, current, |c: Choice<WindowState>| em(EdMsg::Window(c.value)))
                .text_size(13)
                .padding(Padding::from([5.0, 10.0]))
                .style(style::pick)
                .menu_style(style::menu),
        ]
        .align_y(Alignment::Center),
    );
    c = c.push(
        checkbox(ed.run_as_admin)
            .label(tr.get("editor.run_as_admin"))
            .on_toggle(|v| em(EdMsg::Admin(v)))
            .size(16)
            .text_size(13)
            .style(style::check),
    );
    c.into()
}

fn text_details<'a>(app: &'a App, ed: &'a Editor, errors: &[ValidationError]) -> Element<'a, Message> {
    let tr = &app.tr;
    let vars = row![
        text(tr.get("editor.variables")).size(12).style(style::text_secondary),
        chip(tr.get("editor.var_clipboard"), "%CLIPBOARD%"),
        chip(tr.get("editor.var_date"), "%DATE%"),
        chip(tr.get("editor.var_time"), "%TIME%"),
        chip(tr.get("editor.var_user"), "%USERPROFILE%"),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .wrap();
    let modes = [TextMode::Typing, TextMode::Paste];
    let mut mode_row = row![text(tr.get("editor.text_mode")).size(13)].spacing(6).align_y(Alignment::Center);
    for mode in modes {
        let label = tr.get(if mode == TextMode::Typing { "steps.mode_typing" } else { "steps.mode_paste" });
        mode_row = mode_row.push(segment(label, ed.text_mode == mode, em(EdMsg::TextMode(mode))));
    }
    let mut c = column![
        widgets::field_label(tr.get("editor.text")),
        text_editor(&ed.text)
            .placeholder(tr.get("editor.text_placeholder"))
            .on_action(|a| em(EdMsg::Text(a)))
            .height(Length::Fixed(150.0))
            .padding(10)
            .size(13)
            .style(style::editor),
        vars,
        widgets::hint(tr.get("editor.variables_hint")),
        mode_row,
        widgets::hint(tr.get(if ed.text_mode == TextMode::Paste { "editor.paste_hint" } else { "editor.typing_hint" })),
    ]
    .spacing(8);
    if ed.show_errors && errors.contains(&ValidationError::MissingText) {
        c = c.push(widgets::error(tr.get("editor.missing_text")));
    }
    c = c.push(
        button(
            row![icons::icon(Icon::Steps, 14.0, Some(icons::accent)), text(tr.get("editor.convert_to_macro")).size(13)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .padding(Padding::from([4.0, 0.0]))
        .style(style::button_link)
        .on_press(em(EdMsg::ConvertToMacro)),
    );
    c.into()
}

fn advanced<'a>(app: &'a App, ed: &'a Editor, errors: &[ValidationError]) -> Element<'a, Message> {
    let tr = &app.tr;
    let header = button(
        row![
            icons::icon(if ed.advanced_open { Icon::ChevronDown } else { Icon::Chevron }, 16.0, Some(icons::secondary)),
            text(tr.get("editor.advanced")).size(13).font(font_semibold()),
            space::horizontal(),
            match summary_conditions(tr, &ed.conditions) {
                summary if summary.is_empty() => Element::from(space()),
                summary => widgets::badge(summary, Tone::Accent),
            },
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::from([8.0, 6.0]))
    .style(style::button_ghost)
    .on_press(em(EdMsg::ToggleAdvanced));
    let mut section = Column::new().spacing(12).push(header);
    if !ed.advanced_open {
        return section.into();
    }
    let mode = ed.conditions.program_mode;
    let mut programs = column![
        widgets::field_label(tr.get("editor.programs")),
        row![
            segment(tr.get("editor.program_everywhere"), mode == ProgramMode::Everywhere, em(EdMsg::ProgramMode(ProgramMode::Everywhere))),
            segment(tr.get("editor.program_only"), mode == ProgramMode::OnlyIn, em(EdMsg::ProgramMode(ProgramMode::OnlyIn))),
            segment(tr.get("editor.program_except"), mode == ProgramMode::Except, em(EdMsg::ProgramMode(ProgramMode::Except))),
        ]
        .spacing(6)
    ]
    .spacing(8);
    if mode != ProgramMode::Everywhere {
        let chips = ed.conditions.programs.iter().enumerate().map(|(i, p)| {
            container(
                row![
                    text(p.clone()).size(12.5),
                    button(icons::icon(Icon::Close, 12.0, Some(icons::secondary)))
                        .padding(2)
                        .style(style::button_ghost)
                        .on_press(em(EdMsg::RemoveProgram(i)))
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([3.0, 4.0]).left(10.0))
            .style(style::badge(Tone::Neutral))
            .into()
        });
        programs = programs.push(iced::widget::Row::with_children(chips).spacing(6).wrap());
        programs = programs.push(
            row![
                text_input(tr.get("editor.program_placeholder"), &ed.program_input)
                    .on_input(|v| em(EdMsg::ProgramInput(v)))
                    .on_submit(em(EdMsg::AddProgram))
                    .padding(Padding::from([6.0, 10.0]))
                    .size(13)
                    .style(style::input),
                widgets::text_button(tr.get("editor.program_add"), Some(em(EdMsg::AddProgram)), style::button_secondary),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        programs = programs.push(
            row![
                pick_list(app.running_programs.clone(), None::<String>, |p| em(EdMsg::AddRunning(p)))
                    .placeholder(tr.get("editor.program_running"))
                    .on_open(em(EdMsg::RefreshRunning))
                    .text_size(13)
                    .padding(Padding::from([5.0, 10.0]))
                    .width(Length::Fill)
                    .style(style::pick)
                    .menu_style(style::menu),
                widgets::labeled_button(Icon::Pipette, tr.get("editor.program_pipette"), Some(Message::StartPick(PickTarget::ConditionProgram)), ButtonKind::Secondary),
                widgets::text_button(tr.get("editor.program_browse"), Some(Message::Pick(PickPurpose::Program)), style::button_secondary),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        if ed.show_errors && errors.contains(&ValidationError::MissingPrograms) {
            programs = programs.push(widgets::error(tr.get("editor.missing_programs")));
        }
    }
    section = section.push(programs);
    let lock_choices: Vec<Choice<LockReq>> = LockReq::ALL
        .iter()
        .map(|&l| {
            Choice::new(l, tr.get(match l {
                LockReq::Any => "editor.lock_any",
                LockReq::On => "editor.lock_on",
                LockReq::Off => "editor.lock_off",
            }))
        })
        .collect();
    let lock_row = |label: &'a str, kind: LockKind, value: LockReq| -> Element<'a, Message> {
        let current = lock_choices.iter().find(|c| c.value == value).cloned();
        row![
            text(label).size(13).width(Length::Fixed(110.0)),
            pick_list(lock_choices.clone(), current, move |c: Choice<LockReq>| em(EdMsg::Lock(kind, c.value)))
                .text_size(13)
                .padding(Padding::from([5.0, 10.0]))
                .style(style::pick)
                .menu_style(style::menu),
        ]
        .align_y(Alignment::Center)
        .into()
    };
    section = section.push(
        column![
            widgets::field_label(tr.get("editor.locks")),
            lock_row(tr.get("editor.caps"), LockKind::Caps, ed.conditions.caps_lock),
            lock_row(tr.get("editor.num"), LockKind::Num, ed.conditions.num_lock),
            lock_row(tr.get("editor.scroll"), LockKind::Scroll, ed.conditions.scroll_lock),
        ]
        .spacing(6),
    );
    section.into()
}

fn name_section<'a>(app: &'a App, ed: &'a Editor) -> Element<'a, Message> {
    let tr = &app.tr;
    let suggestion = ed.build_action().ok().and_then(|a| suggested_name(tr, &a));
    let placeholder = match &suggestion {
        Some(s) => tr.fmt("editor.name_placeholder", &[("name", s)]),
        None => tr.get("editor.name_placeholder_empty").to_string(),
    };
    let mut groups = vec![Choice::new(String::new(), tr.get("groups.none"))];
    groups.extend(app.config.groups.iter().map(|g| Choice::new(g.clone(), g.clone())));
    let current_group = groups.iter().find(|c| c.value == ed.group).cloned();
    let mut c = column![
        widgets::section_title(tr.get("editor.step_name")),
        text_input(&placeholder, &ed.name)
            .on_input(|v| em(EdMsg::Name(v)))
            .on_submit(Message::Save)
            .padding(Padding::from([6.0, 10.0]))
            .size(13)
            .style(style::input),
        row![
            text(tr.get("editor.group")).size(13).width(Length::Fixed(110.0)),
            pick_list(groups, current_group, |c: Choice<String>| em(EdMsg::Group(c.value)))
                .text_size(13)
                .padding(Padding::from([5.0, 10.0]))
                .width(Length::Fill)
                .style(style::pick)
                .menu_style(style::menu),
        ]
        .align_y(Alignment::Center),
        row![
            toggler(ed.enabled).label(tr.get("editor.enabled")).on_toggle(|v| em(EdMsg::Enabled(v))).size(18).text_size(13).style(style::switch),
            space::horizontal(),
            checkbox(ed.favorite).label(tr.get("editor.favorite")).on_toggle(|v| em(EdMsg::Favorite(v))).size(16).text_size(13).style(style::check),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(8);
    if let Some(id) = ed.id {
        let usage = app.stats.get(id);
        let line = if usage.count == 0 {
            tr.get("editor.never_used").to_string()
        } else {
            let when = crate::tr::datetime(tr, usage.last_used);
            tr.plural("editor.usage", usage.count as i64, &[("date", &when)])
        };
        c = c.push(
            row![
                text(line).size(12).style(style::text_tertiary).width(Length::Fill),
                widgets::text_button(tr.get("editor.reset_stats"), (usage.count > 0).then_some(Message::ResetStats(Some(id))), style::button_link),
            ]
            .align_y(Alignment::Center),
        );
    }
    c.into()
}

pub fn view<'a>(app: &'a App, ed: &'a Editor) -> Element<'a, Message> {
    let tr = &app.tr;
    let read_only = app.load_error.is_some();
    let draft = ed.to_shortcut(ed.id.unwrap_or(0));
    let errors = draft.as_ref().map(conflict::validate).unwrap_or_default();

    let keys = keys_section(app, ed, draft.as_ref(), &errors);
    let kinds = column![
        widgets::section_title(tr.get("editor.step_action")),
        row![
            kind_card(Icon::Open, tr.get("editor.kind_open"), ed.kind == EditKind::Open, em(EdMsg::Kind(EditKind::Open))),
            kind_card(Icon::Text, tr.get("editor.kind_text"), ed.kind == EditKind::Text, em(EdMsg::Kind(EditKind::Text))),
            kind_card(Icon::Steps, tr.get("editor.kind_macro"), ed.kind == EditKind::Macro, em(EdMsg::Kind(EditKind::Macro))),
        ]
        .spacing(8),
        widgets::hint(tr.get(match ed.kind {
            EditKind::Open => "editor.kind_open_hint",
            EditKind::Text => "editor.kind_text_hint",
            EditKind::Macro => "editor.kind_macro_hint",
        })),
    ]
    .spacing(8);
    let details: Element<'a, Message> = match ed.kind {
        EditKind::Open => open_details(app, ed, &errors),
        EditKind::Text => text_details(app, ed, &errors),
        EditKind::Macro => {
            let mut c = Column::new().spacing(8).push(steps::view(app, ed));
            if ed.show_errors && errors.contains(&ValidationError::EmptyMacro) {
                c = c.push(widgets::error(tr.get("editor.empty_macro")));
            }
            c.into()
        }
    };
    let body = column![keys, kinds, details, advanced(app, ed, &errors), name_section(app, ed)]
        .spacing(22)
        .padding(Padding::from([4.0, 18.0]).bottom(18.0).right(22.0));

    let title = if ed.id.is_some() { tr.get("editor.title_edit") } else { tr.get("editor.title_new") };
    let header = row![
        text(title).size(18).font(font_semibold()),
        space::horizontal(),
        widgets::icon_button(Icon::Close, tr.get("editor.cancel"), Some(Message::CloseEditor)),
    ]
    .align_y(Alignment::Center)
    .padding(Padding::from([14.0, 18.0]).bottom(6.0).right(12.0));

    let mut footer = row![].spacing(8).align_y(Alignment::Center);
    if ed.id.is_some() {
        footer = footer.push(
            widgets::labeled_button(Icon::Trash, tr.get("editor.delete"), (!read_only).then_some(Message::Delete), ButtonKind::Ghost)
                .style(style::button_danger),
        );
    }
    let test_label = match app.test_countdown {
        Some((deadline, _)) => {
            let left = deadline.saturating_duration_since(std::time::Instant::now()).as_secs() + 1;
            tr.fmt("editor.test_countdown", &[("n", &left.to_string())])
        }
        None => tr.get("editor.test").to_string(),
    };
    footer = footer
        .push(
            button(row![icons::icon(Icon::Play, 14.0, None), text(test_label).size(13)].spacing(6).align_y(Alignment::Center))
                .padding(Padding::from([6.0, 12.0]))
                .style(style::button_secondary)
                .on_press_maybe((draft.is_some() && app.test_countdown.is_none()).then_some(Message::Test)),
        )
        .push(space::horizontal())
        .push(widgets::text_button(tr.get("editor.cancel"), Some(Message::CloseEditor), style::button_secondary))
        .push(widgets::text_button(tr.get("editor.save"), (!read_only).then_some(Message::Save), style::button_primary));
    let footer = column![
        container(space()).height(1).width(Length::Fill).style(|theme| container::Style {
            background: Some(style::tokens(theme).stroke.into()),
            ..Default::default()
        }),
        column![footer, widgets::hint(tr.get(if ed.kind == EditKind::Macro { "editor.shortcuts_hint_macro" } else { "editor.shortcuts_hint" }))]
            .spacing(6)
            .padding(Padding::from([12.0, 18.0])),
    ];

    container(column![header, scrollable(widgets::clip_text(body)).id(scroll_id()).height(Length::Fill).style(style::scroll), footer])
        .width(Length::Fixed(app.editor_width()))
        .height(Length::Fill)
        .style(style::panel)
        .into()
}
