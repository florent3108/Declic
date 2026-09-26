//! Visual block editor for macros (F-MAC-02): one card per step, "+" menu,
//! move up/down (buttons or Alt+↑/↓), duplicate, delete, disable.

use super::editor::{EdMsg, Editor};
use super::icons::{self, Icon};
use super::style::{self, Tone};
use super::widgets::{self, ButtonKind, Choice, font_semibold};
use super::{App, CaptureTarget, Message, PickPurpose, PickTarget};
use crate::tr;
use declic_core::i18n::Catalog;
use declic_core::{
    ClickKind, IfMissing, KeyList, MacroStep, MouseButton, MoveOrigin, OpenAction, Step, StepKind, TextMode,
    WheelDirection, WindowState,
};
use iced::widget::{
    Column, Row, button, checkbox, column, container, mouse_area, pick_list, row, space, text, text_editor, text_input,
    toggler,
};
use iced::{Alignment, Element, Length, Padding};

/// Editable state of one step. Numeric fields are kept as typed so that
/// they can be edited freely; they are parsed when the macro is built.
pub struct StepDraft {
    pub enabled: bool,
    pub step: Step,
    /// Text of "type text" and "copy to clipboard" steps.
    pub text: text_editor::Content,
    pub numbers: [String; 2],
}

/// A numeric field that does not contain a valid number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadNumber {
    pub step: usize,
}

fn numbers_of(step: &Step) -> [String; 2] {
    match step {
        Step::PressKeys { repeat, .. } => [repeat.to_string(), String::new()],
        Step::Wait { ms } => [ms.to_string(), String::new()],
        Step::ActivateWindow { timeout_ms, .. } | Step::ActivateOrLaunch { timeout_ms, .. } => {
            [timeout_ms.to_string(), String::new()]
        }
        Step::MoveMouse { x, y, .. } => [x.to_string(), y.to_string()],
        Step::Wheel { notches, .. } => [notches.to_string(), String::new()],
        _ => [String::new(), String::new()],
    }
}

impl StepDraft {
    pub fn from_step(step: &MacroStep) -> StepDraft {
        let text = match &step.step {
            Step::TypeText { text, .. } | Step::CopyText { text } => text_editor::Content::with_text(text),
            _ => text_editor::Content::new(),
        };
        StepDraft { enabled: step.enabled, numbers: numbers_of(&step.step), step: step.step.clone(), text }
    }

    pub fn new(kind: StepKind, default_mode: TextMode) -> StepDraft {
        let mut step = kind.default_step();
        if let Step::TypeText { mode, .. } = &mut step {
            *mode = default_mode;
        }
        StepDraft::from_step(&MacroStep::new(step))
    }

    fn content_text(&self) -> String {
        let mut text = self.text.text();
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        text
    }

    /// The step with the current field values.
    pub fn build(&self, index: usize) -> Result<MacroStep, BadNumber> {
        let bad = || BadNumber { step: index };
        let u = |i: usize| self.numbers[i].trim().parse::<u32>().map_err(|_| bad());
        let s = |i: usize| self.numbers[i].trim().parse::<i32>().map_err(|_| bad());
        let mut step = self.step.clone();
        match &mut step {
            Step::TypeText { text, .. } | Step::CopyText { text } => *text = self.content_text(),
            Step::PressKeys { repeat, .. } => *repeat = u(0)?,
            Step::Wait { ms } => *ms = u(0)?,
            Step::ActivateWindow { timeout_ms, .. } | Step::ActivateOrLaunch { timeout_ms, .. } => *timeout_ms = u(0)?,
            Step::MoveMouse { x, y, .. } => {
                *x = s(0)?;
                *y = s(1)?;
            }
            Step::Wheel { notches, .. } => *notches = u(0)?,
            _ => {}
        }
        Ok(MacroStep { enabled: self.enabled, step })
    }

    fn duplicate(&self, index: usize) -> StepDraft {
        let mut copy = match self.build(index) {
            Ok(step) => StepDraft::from_step(&step),
            Err(_) => StepDraft::from_step(&MacroStep::new(self.step.clone())),
        };
        copy.numbers = self.numbers.clone();
        copy
    }

    fn open_action_mut(&mut self) -> Option<&mut OpenAction> {
        match &mut self.step {
            Step::Open(open) => Some(open),
            Step::ActivateOrLaunch { launch, .. } => Some(launch),
            _ => None,
        }
    }
}

/// Messages of one step card.
#[derive(Debug, Clone)]
pub enum StepMsg {
    Enabled(bool),
    Up,
    Down,
    Duplicate,
    Delete,
    Select,
    Text(text_editor::Action),
    InsertVar(&'static str),
    Mode(TextMode),
    Number(usize, String),
    Title(String),
    Program(String),
    Target(String),
    Arguments(String),
    WorkingDir(String),
    Window(WindowState),
    Admin(bool),
    IfMissing(IfMissing),
    Button(MouseButton),
    Click(ClickKind),
    Origin(MoveOrigin),
    Direction(WheelDirection),
    Message(String),
    ReleaseAll,
}

/// Applies a message to the step list of the editor.
pub fn update(ed: &mut Editor, index: usize, msg: StepMsg) {
    let len = ed.steps.len();
    if index >= len {
        return;
    }
    match msg {
        StepMsg::Up => {
            if index > 0 {
                ed.steps.swap(index, index - 1);
                ed.selected_step = Some(index - 1);
            }
            return;
        }
        StepMsg::Down => {
            if index + 1 < len {
                ed.steps.swap(index, index + 1);
                ed.selected_step = Some(index + 1);
            }
            return;
        }
        StepMsg::Duplicate => {
            let copy = ed.steps[index].duplicate(index);
            ed.steps.insert(index + 1, copy);
            ed.selected_step = Some(index + 1);
            return;
        }
        StepMsg::Delete => {
            ed.steps.remove(index);
            ed.selected_step = if ed.steps.is_empty() { None } else { Some(index.min(ed.steps.len() - 1)) };
            return;
        }
        StepMsg::Select => {
            ed.selected_step = Some(index);
            return;
        }
        _ => {}
    }
    ed.selected_step = Some(index);
    let draft = &mut ed.steps[index];
    match msg {
        StepMsg::Enabled(on) => draft.enabled = on,
        StepMsg::Text(action) => draft.text.perform(action),
        StepMsg::InsertVar(var) => draft
            .text
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(std::sync::Arc::new(var.to_string())))),
        StepMsg::Number(i, value) => {
            if i < 2 {
                draft.numbers[i] = value;
            }
        }
        StepMsg::Title(value) => {
            if let Step::ActivateWindow { title, .. } | Step::ActivateOrLaunch { title, .. } = &mut draft.step {
                *title = value;
            }
        }
        StepMsg::Program(value) => {
            if let Step::ActivateWindow { program, .. } | Step::ActivateOrLaunch { program, .. } = &mut draft.step {
                *program = value;
            }
        }
        StepMsg::IfMissing(value) => {
            if let Step::ActivateWindow { if_missing, .. } | Step::ActivateOrLaunch { if_missing, .. } = &mut draft.step {
                *if_missing = value;
            }
        }
        StepMsg::Target(value) => {
            if let Some(open) = draft.open_action_mut() {
                open.target = value;
            }
        }
        StepMsg::Arguments(value) => {
            if let Some(open) = draft.open_action_mut() {
                open.arguments = value;
            }
        }
        StepMsg::WorkingDir(value) => {
            if let Some(open) = draft.open_action_mut() {
                open.working_dir = value;
            }
        }
        StepMsg::Window(value) => {
            if let Some(open) = draft.open_action_mut() {
                open.window = value;
            }
        }
        StepMsg::Admin(value) => {
            if let Some(open) = draft.open_action_mut() {
                open.run_as_admin = value;
            }
        }
        StepMsg::Mode(value) => {
            if let Step::TypeText { mode, .. } = &mut draft.step {
                *mode = value;
            }
        }
        StepMsg::Button(value) => {
            if let Step::Click { button, .. } = &mut draft.step {
                *button = value;
            }
        }
        StepMsg::Click(value) => {
            if let Step::Click { kind, .. } = &mut draft.step {
                *kind = value;
            }
        }
        StepMsg::Origin(value) => {
            if let Step::MoveMouse { origin, .. } = &mut draft.step {
                *origin = value;
            }
        }
        StepMsg::Direction(value) => {
            if let Step::Wheel { direction, .. } = &mut draft.step {
                *direction = value;
            }
        }
        StepMsg::Message(value) => {
            if let Step::Notify { text } = &mut draft.step {
                *text = value;
            }
        }
        StepMsg::ReleaseAll => {
            if let Step::ReleaseKeys { keys } = &mut draft.step {
                *keys = KeyList::default();
            }
        }
        StepMsg::Up | StepMsg::Down | StepMsg::Duplicate | StepMsg::Delete | StepMsg::Select => {}
    }
}

/// Inserts a new step after the selected one (or at the end).
pub fn add_step(ed: &mut Editor, kind: StepKind, default_mode: TextMode) {
    let at = ed.selected_step.map(|i| i + 1).unwrap_or(ed.steps.len()).min(ed.steps.len());
    ed.steps.insert(at, StepDraft::new(kind, default_mode));
    ed.selected_step = Some(at);
    ed.add_menu_open = false;
}

/// A step started being dragged by its handle.
pub fn start_drag(ed: &mut Editor, index: usize) {
    if index < ed.steps.len() {
        ed.dragging = Some(index);
        ed.selected_step = Some(index);
    }
}

/// A dragged step was dropped: moves it to its new place (the selection
/// follows it and the step numbers follow the new order).
pub fn drop_step(ed: &mut Editor, from: usize, to: usize) {
    ed.dragging = None;
    if from < ed.steps.len() && to < ed.steps.len() {
        declic_core::reorder::move_item(&mut ed.steps, from, to);
        ed.selected_step = ed.selected_step.map(|i| declic_core::reorder::index_after_move(i, from, to));
    }
}

/// Where the drag handle is in a step card (see [`step_card`]): the grip at
/// the start of the header, with some margin around it.
const DRAG_HANDLE: super::drag_list::Handle = super::drag_list::Handle { leading: 4.0, top: 2.0, width: 28.0, height: 32.0 };

/// Moves the selected step up (-1) or down (+1), except during a drag.
pub fn move_selected(ed: &mut Editor, delta: i32) {
    if let Some(i) = ed.selected_step.filter(|_| ed.dragging.is_none()) {
        update(ed, i, if delta < 0 { StepMsg::Up } else { StepMsg::Down });
    }
}

pub fn step_icon(kind: StepKind) -> Icon {
    match kind {
        StepKind::TypeText => Icon::Text,
        StepKind::PressKeys => Icon::Keyboard,
        StepKind::HoldKeys | StepKind::ReleaseKeys => Icon::Hand,
        StepKind::Wait => Icon::Clock,
        StepKind::ActivateWindow => Icon::Window,
        StepKind::ActivateOrLaunch => Icon::Apps,
        StepKind::Open => Icon::Open,
        StepKind::CopyText => Icon::Clipboard,
        StepKind::Click => Icon::Mouse,
        StepKind::MoveMouse => Icon::Move,
        StepKind::Wheel => Icon::Wheel,
        StepKind::Notify => Icon::Bell,
    }
}

pub fn step_title(tr: &Catalog, kind: StepKind) -> &str {
    tr.get(match kind {
        StepKind::TypeText => "steps.type_text",
        StepKind::PressKeys => "steps.press_keys",
        StepKind::HoldKeys => "steps.hold_keys",
        StepKind::ReleaseKeys => "steps.release_keys",
        StepKind::Wait => "steps.wait",
        StepKind::ActivateWindow => "steps.activate_window",
        StepKind::ActivateOrLaunch => "steps.activate_or_launch",
        StepKind::Open => "steps.open",
        StepKind::CopyText => "steps.copy_text",
        StepKind::Click => "steps.click",
        StepKind::MoveMouse => "steps.move_mouse",
        StepKind::Wheel => "steps.wheel",
        StepKind::Notify => "steps.notify",
    })
}

fn st(index: usize, msg: StepMsg) -> Message {
    Message::Ed(EdMsg::Step(index, msg))
}

fn small_icon_button<'a>(icon: Icon, tip: &'a str, message: Option<Message>) -> Element<'a, Message> {
    let b = button(icons::icon(icon, 14.0, Some(icons::secondary))).padding(5).style(style::button_ghost).on_press_maybe(message);
    iced::widget::tooltip(b, container(text(tip).size(12)).padding(6).style(style::card), iced::widget::tooltip::Position::Top)
        .gap(2)
        .into()
}

fn input<'a>(placeholder: &str, value: &str, on: impl Fn(String) -> Message + 'a) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value).on_input(on).padding(Padding::from([5.0, 8.0])).size(13).style(style::input)
}

fn number<'a>(value: &str, width: f32, on: impl Fn(String) -> Message + 'a) -> Element<'a, Message> {
    input("", value, on).width(Length::Fixed(width)).into()
}

fn labeled<'a>(label: &'a str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![text(label).size(12.5).style(style::text_secondary).width(Length::Fixed(118.0)), content.into()]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

fn picker<'a, T: Copy + PartialEq + 'a>(
    choices: Vec<Choice<T>>,
    value: T,
    on: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let current = choices.iter().find(|c| c.value == value).cloned();
    pick_list(choices, current, move |c: Choice<T>| on(c.value))
        .text_size(13)
        .padding(Padding::from([4.0, 8.0]))
        .style(style::pick)
        .menu_style(style::menu)
        .into()
}

fn var_chips<'a>(tr: &'a Catalog, index: usize) -> Element<'a, Message> {
    let chip = |label: &'a str, var: &'static str| -> Element<'a, Message> {
        button(text(label).size(11.5))
            .padding(Padding::from([2.0, 7.0]))
            .style(style::button_segment(false))
            .on_press(st(index, StepMsg::InsertVar(var)))
            .into()
    };
    row![
        chip(tr.get("editor.var_clipboard"), "%CLIPBOARD%"),
        chip(tr.get("editor.var_date"), "%DATE%"),
        chip(tr.get("editor.var_time"), "%TIME%"),
    ]
    .spacing(5)
    .into()
}

fn keys_field<'a>(app: &'a App, labels: Vec<String>, empty: &'a str, target: CaptureTarget) -> Element<'a, Message> {
    let tr = &app.tr;
    let capturing = app.capture == Some(target);
    let content: Element<'a, Message> = if capturing {
        text(tr.get("editor.capture_prompt")).size(12.5).style(style::text_accent).into()
    } else if labels.is_empty() {
        text(empty).size(12.5).style(style::text_tertiary).into()
    } else {
        widgets::keycaps(labels, 12.0, false)
    };
    button(row![icons::icon(Icon::Keyboard, 14.0, Some(icons::secondary)), content].spacing(8).align_y(Alignment::Center))
        .padding(Padding::from([6.0, 10.0]))
        .width(Length::Fill)
        .style(style::capture_box(capturing))
        .on_press(if capturing { Message::CancelCapture } else { Message::StartCapture(target) })
        .into()
}

fn key_list_labels(tr: &Catalog, keys: &KeyList) -> Vec<String> {
    keys.0
        .iter()
        .map(|k| match k {
            declic_core::HeldKey::Modifier(m, side) => {
                let hk = declic_core::Hotkey::new(declic_core::Key::Space).with(
                    *m,
                    if *side == declic_core::Side::Right { declic_core::ModReq::Right } else { declic_core::ModReq::Any },
                );
                tr::keycaps(tr, &hk).into_iter().next().unwrap_or_default()
            }
            declic_core::HeldKey::Key(key) => tr::key_label(tr, *key),
        })
        .collect()
}

fn open_fields<'a>(tr: &'a Catalog, index: usize, open: &'a OpenAction, launch: bool) -> Element<'a, Message> {
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
    let mut c = Column::new().spacing(6);
    c = c.push(labeled(
        tr.get(if launch { "steps.launch" } else { "editor.target" }),
        row![
            input(tr.get("editor.target_placeholder"), &open.target, move |v| st(index, StepMsg::Target(v))),
            small_icon_button(Icon::File, tr.get("editor.browse_file"), Some(Message::Pick(PickPurpose::StepTarget(index)))),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    ));
    if !launch {
        c = c.push(labeled(tr.get("editor.arguments"), input("", &open.arguments, move |v| st(index, StepMsg::Arguments(v)))));
        c = c.push(labeled(
            tr.get("editor.working_dir"),
            input(tr.get("editor.working_dir_placeholder"), &open.working_dir, move |v| st(index, StepMsg::WorkingDir(v))),
        ));
        c = c.push(labeled(tr.get("editor.window"), picker(states, open.window, move |v| st(index, StepMsg::Window(v)))));
        c = c.push(
            checkbox(open.run_as_admin)
                .label(tr.get("editor.run_as_admin"))
                .on_toggle(move |v| st(index, StepMsg::Admin(v)))
                .size(15)
                .text_size(12.5)
                .style(style::check),
        );
    }
    c.into()
}

fn window_fields<'a>(
    app: &'a App,
    index: usize,
    title: &'a str,
    program: &'a str,
    numbers: &'a [String; 2],
    if_missing: IfMissing,
) -> Element<'a, Message> {
    let tr = &app.tr;
    let missing: Vec<Choice<IfMissing>> = vec![
        Choice::new(IfMissing::Stop, tr.get("steps.if_missing_stop")),
        Choice::new(IfMissing::Continue, tr.get("steps.if_missing_continue")),
    ];
    column![
        labeled(
            tr.get("steps.window_title"),
            row![
                input(tr.get("steps.window_title_placeholder"), title, move |v| st(index, StepMsg::Title(v))),
                small_icon_button(Icon::Pipette, tr.get("steps.pipette_window"), Some(Message::StartPick(PickTarget::StepWindow(index)))),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        ),
        labeled(tr.get("steps.window_program"), input(tr.get("editor.program_placeholder"), program, move |v| st(index, StepMsg::Program(v)))),
        labeled(
            tr.get("steps.timeout"),
            row![number(&numbers[0], 90.0, move |v| st(index, StepMsg::Number(0, v))), text(tr.get("steps.ms")).size(12)]
                .spacing(6)
                .align_y(Alignment::Center),
        ),
        labeled(tr.get("steps.if_missing"), picker(missing, if_missing, move |v| st(index, StepMsg::IfMissing(v)))),
    ]
    .spacing(6)
    .into()
}

fn step_body<'a>(app: &'a App, ed: &'a Editor, index: usize) -> Element<'a, Message> {
    let tr = &app.tr;
    let draft = &ed.steps[index];
    match &draft.step {
        Step::TypeText { mode, .. } => {
            let modes: Vec<Choice<TextMode>> = vec![
                Choice::new(TextMode::Typing, tr.get("steps.mode_typing")),
                Choice::new(TextMode::Paste, tr.get("steps.mode_paste")),
            ];
            column![
                text_editor(&draft.text)
                    .placeholder(tr.get("editor.text_placeholder"))
                    .on_action(move |a| st(index, StepMsg::Text(a)))
                    .height(Length::Fixed(80.0))
                    .padding(8)
                    .size(13)
                    .style(style::editor),
                row![var_chips(tr, index), space::horizontal(), picker(modes, *mode, move |v| st(index, StepMsg::Mode(v)))]
                    .align_y(Alignment::Center),
            ]
            .spacing(6)
            .into()
        }
        Step::PressKeys { keys, .. } => column![
            keys_field(app, tr::keycaps(tr, keys), tr.get("steps.no_keys"), CaptureTarget::StepKeys(index)),
            labeled(tr.get("steps.repeat"), number(&draft.numbers[0], 70.0, move |v| st(index, StepMsg::Number(0, v)))),
        ]
        .spacing(6)
        .into(),
        Step::HoldKeys { keys } => column![
            keys_field(app, key_list_labels(tr, keys), tr.get("steps.no_keys"), CaptureTarget::StepHold(index)),
            widgets::hint(tr.get("steps.hold_hint")),
        ]
        .spacing(6)
        .into(),
        Step::ReleaseKeys { keys } => column![
            row![
                keys_field(app, key_list_labels(tr, keys), tr.get("steps.release_all"), CaptureTarget::StepHold(index)),
                small_icon_button(Icon::Close, tr.get("steps.release_all"), (!keys.is_empty()).then_some(st(index, StepMsg::ReleaseAll))),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        ]
        .into(),
        Step::Wait { .. } => labeled(
            tr.get("steps.duration"),
            row![number(&draft.numbers[0], 90.0, move |v| st(index, StepMsg::Number(0, v))), text(tr.get("steps.ms")).size(12)]
                .spacing(6)
                .align_y(Alignment::Center),
        ),
        Step::ActivateWindow { title, program, if_missing, .. } => {
            window_fields(app, index, title, program, &draft.numbers, *if_missing)
        }
        Step::ActivateOrLaunch { title, program, if_missing, launch, .. } => column![
            window_fields(app, index, title, program, &draft.numbers, *if_missing),
            open_fields(tr, index, launch, true),
        ]
        .spacing(6)
        .into(),
        Step::Open(open) => open_fields(tr, index, open, false),
        Step::CopyText { .. } => column![
            text_editor(&draft.text)
                .placeholder(tr.get("steps.copy_placeholder"))
                .on_action(move |a| st(index, StepMsg::Text(a)))
                .height(Length::Fixed(60.0))
                .padding(8)
                .size(13)
                .style(style::editor),
            var_chips(tr, index),
        ]
        .spacing(6)
        .into(),
        Step::Click { button: b, kind } => {
            let buttons: Vec<Choice<MouseButton>> = MouseButton::ALL
                .iter()
                .map(|&m| {
                    Choice::new(m, tr.get(match m {
                        MouseButton::Left => "steps.button_left",
                        MouseButton::Right => "steps.button_right",
                        MouseButton::Middle => "steps.button_middle",
                    }))
                })
                .collect();
            let kinds: Vec<Choice<ClickKind>> = ClickKind::ALL
                .iter()
                .map(|&k| {
                    Choice::new(k, tr.get(match k {
                        ClickKind::Single => "steps.click_single",
                        ClickKind::Double => "steps.click_double",
                        ClickKind::Down => "steps.click_down",
                        ClickKind::Up => "steps.click_up",
                    }))
                })
                .collect();
            row![
                picker(buttons, *b, move |v| st(index, StepMsg::Button(v))),
                picker(kinds, *kind, move |v| st(index, StepMsg::Click(v))),
            ]
            .spacing(8)
            .into()
        }
        Step::MoveMouse { origin, .. } => {
            let origins: Vec<Choice<MoveOrigin>> = MoveOrigin::ALL
                .iter()
                .map(|&o| {
                    Choice::new(o, tr.get(match o {
                        MoveOrigin::Screen => "steps.origin_screen",
                        MoveOrigin::Window => "steps.origin_window",
                        MoveOrigin::Cursor => "steps.origin_cursor",
                    }))
                })
                .collect();
            column![
                labeled(tr.get("steps.origin"), picker(origins, *origin, move |v| st(index, StepMsg::Origin(v)))),
                labeled(
                    tr.get("steps.position"),
                    row![
                        text("x").size(12),
                        number(&draft.numbers[0], 70.0, move |v| st(index, StepMsg::Number(0, v))),
                        text("y").size(12),
                        number(&draft.numbers[1], 70.0, move |v| st(index, StepMsg::Number(1, v))),
                        small_icon_button(
                            Icon::Pipette,
                            tr.get("steps.pipette_position"),
                            (*origin != MoveOrigin::Cursor).then_some(Message::StartPick(PickTarget::StepPosition(index))),
                        ),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                ),
            ]
            .spacing(6)
            .into()
        }
        Step::Wheel { direction, .. } => {
            let directions: Vec<Choice<WheelDirection>> = WheelDirection::ALL
                .iter()
                .map(|&d| {
                    Choice::new(d, tr.get(match d {
                        WheelDirection::Up => "steps.wheel_up",
                        WheelDirection::Down => "steps.wheel_down",
                        WheelDirection::Left => "steps.wheel_left",
                        WheelDirection::Right => "steps.wheel_right",
                    }))
                })
                .collect();
            row![
                picker(directions, *direction, move |v| st(index, StepMsg::Direction(v))),
                number(&draft.numbers[0], 60.0, move |v| st(index, StepMsg::Number(0, v))),
                text(tr.get("steps.notches")).size(12),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        }
        Step::Notify { text: message } => {
            input(tr.get("steps.notify_placeholder"), message, move |v| st(index, StepMsg::Message(v))).into()
        }
    }
}

fn step_card<'a>(app: &'a App, ed: &'a Editor, index: usize, problems: &[String]) -> Element<'a, Message> {
    let tr = &app.tr;
    let draft = &ed.steps[index];
    let kind = draft.step.kind();
    let selected = ed.selected_step == Some(index);
    let count = ed.steps.len();
    let number_badge = container(text((index + 1).to_string()).size(11.5).font(font_semibold()))
        .padding(Padding::from([1.0, 7.0]))
        .style(style::badge(if draft.enabled { Tone::Accent } else { Tone::Neutral }));
    let title = mouse_area(
        row![
            number_badge,
            icons::icon(step_icon(kind), 16.0, Some(if draft.enabled { icons::accent } else { icons::secondary })),
            text(step_title(tr, kind)).size(13).font(font_semibold()),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(st(index, StepMsg::Select));
    // Drag handle: the drag itself is handled by the list (see DRAG_HANDLE).
    let grip = icons::icon(Icon::Grip, 16.0, Some(icons::secondary));
    let header = row![
        grip,
        title,
        space::horizontal(),
        iced::widget::tooltip(
            toggler(draft.enabled).on_toggle(move |v| st(index, StepMsg::Enabled(v))).size(14).style(style::switch),
            container(text(tr.get("steps.enabled_tip")).size(12)).padding(6).style(style::card),
            iced::widget::tooltip::Position::Top,
        ),
        small_icon_button(Icon::ChevronUp, tr.get("steps.move_up"), (index > 0).then_some(st(index, StepMsg::Up))),
        small_icon_button(Icon::ChevronDown, tr.get("steps.move_down"), (index + 1 < count).then_some(st(index, StepMsg::Down))),
        small_icon_button(Icon::Copy, tr.get("steps.duplicate"), Some(st(index, StepMsg::Duplicate))),
        small_icon_button(Icon::Trash, tr.get("steps.delete"), Some(st(index, StepMsg::Delete))),
    ]
    .spacing(2)
    .align_y(Alignment::Center);
    let mut content = Column::new().spacing(8).push(header);
    if draft.enabled {
        content = content.push(step_body(app, ed, index));
    } else {
        content = content.push(widgets::hint(tr.get("steps.disabled")));
    }
    for p in problems {
        content = content.push(widgets::error(p.clone()));
    }
    container(content).padding(Padding::from([8.0, 10.0])).width(Length::Fill).style(style::step_card(selected, draft.enabled)).into()
}

/// The "+" menu listing step kinds.
fn add_menu<'a>(tr: &'a Catalog) -> Element<'a, Message> {
    let items: Vec<Element<'a, Message>> = StepKind::ALL
        .iter()
        .map(|&kind| {
            button(
                row![icons::icon(step_icon(kind), 15.0, Some(icons::accent)), text(step_title(tr, kind)).size(12.5)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .padding(Padding::from([6.0, 8.0]))
            .width(Length::Fill)
            .style(style::button_ghost)
            .on_press(Message::Ed(EdMsg::AddStep(kind)))
            .into()
        })
        .collect();
    let mut grid = Column::new().spacing(2);
    let mut iter = items.into_iter();
    loop {
        let Some(a) = iter.next() else { break };
        let b: Element<'a, Message> = iter.next().unwrap_or_else(|| space().width(Length::Fill).into());
        grid = grid.push(Row::with_children([a, b]).spacing(4));
    }
    container(grid).padding(6).width(Length::Fill).style(style::card).into()
}

/// Human-readable problems of a step (for display under its card).
pub fn problem_texts(tr: &Catalog, problems: &[declic_core::StepProblem], bad_number: bool) -> Vec<String> {
    let mut out: Vec<String> = problems
        .iter()
        .map(|p| {
            tr.get(match p {
                declic_core::StepProblem::EmptyText => "steps.problem_text",
                declic_core::StepProblem::EmptyKeys => "steps.problem_keys",
                declic_core::StepProblem::NoWindowCriteria => "steps.problem_window",
                declic_core::StepProblem::EmptyTarget => "steps.problem_target",
                declic_core::StepProblem::ZeroCount => "steps.problem_count",
            })
            .to_string()
        })
        .collect();
    if bad_number {
        out.push(tr.get("steps.problem_number").to_string());
    }
    out
}

/// The macro section of the editor.
pub fn view<'a>(app: &'a App, ed: &'a Editor) -> Element<'a, Message> {
    let tr = &app.tr;
    let mut list = Column::new().spacing(8);
    if ed.steps.is_empty() {
        list = list.push(widgets::hint(tr.get("steps.empty")));
    }
    let cards: Vec<Element<'a, Message>> = (0..ed.steps.len())
        .map(|index| {
            let problems = if ed.show_errors {
                match ed.steps[index].build(index) {
                    Ok(step) if step.enabled => problem_texts(tr, &declic_core::step_problems(&step.step), false),
                    Ok(_) => Vec::new(),
                    Err(_) => problem_texts(tr, &[], true),
                }
            } else {
                Vec::new()
            };
            step_card(app, ed, index, &problems)
        })
        .collect();
    if !cards.is_empty() {
        list = list.push(super::drag_list::drag_list(
            cards,
            8.0,
            DRAG_HANDLE,
            ed.dragging,
            |index| Message::Ed(EdMsg::StepDragStart(index)),
            |from, to| Message::Ed(EdMsg::StepDrop(from, to)),
            Message::Ed(EdMsg::StepDragCancel),
            Message::StepAutoScroll,
        ));
    }
    let add = widgets::labeled_button(Icon::Plus, tr.get("steps.add"), Some(Message::Ed(EdMsg::ToggleAddMenu)), ButtonKind::Secondary);
    list = list.push(add);
    if ed.add_menu_open {
        list = list.push(add_menu(tr));
    }
    column![list, widgets::hint(tr.fmt("steps.hint", &[("key", &tr::hotkey_text(tr, &app.config.settings.stop_key))]))]
        .spacing(8)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_round_trip_and_validation() {
        let step = MacroStep::new(Step::MoveMouse { x: -12, y: 30, origin: MoveOrigin::Window });
        let mut draft = StepDraft::from_step(&step);
        assert_eq!(draft.build(0).unwrap(), step);
        draft.numbers[0] = "abc".into();
        assert_eq!(draft.build(3), Err(BadNumber { step: 3 }));
        let text = MacroStep::new(Step::TypeText { text: "a\nb".into(), mode: TextMode::Paste });
        assert_eq!(StepDraft::from_step(&text).build(0).unwrap(), text);
    }

    #[test]
    fn dropping_a_step_moves_it_and_marks_the_editor_modified() {
        let tr = tr::load("fr");
        let steps = (1..=5).map(|ms| MacroStep::new(Step::Wait { ms })).collect();
        let keys = "Ctrl+Alt+Q".parse().expect("valid combination");
        let shortcut = declic_core::Shortcut::new(1, keys, declic_core::Action::Macro(declic_core::Macro { steps }));
        let mut ed = Editor::from_shortcut(&shortcut, &tr);
        let order = |ed: &Editor| ed.steps.iter().map(|d| d.build(0).map(|s| s.step)).collect::<Result<Vec<_>, _>>().unwrap();
        assert!(!ed.is_dirty());

        start_drag(&mut ed, 0);
        assert_eq!((ed.dragging, ed.selected_step), (Some(0), Some(0)));
        drop_step(&mut ed, 0, 3);
        assert_eq!(ed.dragging, None);
        let waits = |ms: &[u32]| ms.iter().map(|&ms| Step::Wait { ms }).collect::<Vec<_>>();
        assert_eq!(order(&ed), waits(&[2, 3, 4, 1, 5]));
        assert_eq!(ed.selected_step, Some(3));
        assert!(ed.is_dirty());

        // Moving it back restores the original: nothing left to save.
        drop_step(&mut ed, 3, 0);
        assert_eq!(order(&ed), waits(&[1, 2, 3, 4, 5]));
        assert!(!ed.is_dirty());

        // Out-of-range values (a list changed meanwhile) are ignored.
        drop_step(&mut ed, 9, 0);
        assert_eq!(order(&ed), waits(&[1, 2, 3, 4, 5]));
    }
}
