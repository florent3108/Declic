//! Cheat sheet (§ 8.4): an always-on-top window listing the shortcuts
//! active in the program that was in the foreground. Closed by any key, a
//! click outside, or the cheat-sheet combination again.

use super::style;
use super::widgets::{self, font, font_semibold};
use crate::art::{self, IconVariant};
use crate::paths::Paths;
use crate::tr;
use declic_core::config;
use declic_core::i18n::Catalog;
use iced::widget::{Column, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding, Size, Subscription, Task, Theme, keyboard, window};
use std::time::Duration;

const AUTO_CLOSE: Duration = Duration::from_secs(120);

struct Entry {
    name: String,
    keys: Vec<String>,
    group: String,
    specific: bool,
}

struct Overlay {
    tr: Catalog,
    theme: Theme,
    program: String,
    entries: Vec<Entry>,
    focused: bool,
}

#[derive(Debug, Clone)]
enum Msg {
    Close,
    Focused,
    Unfocused,
}

pub fn run_overlay(program: String) -> iced::Result {
    let paths = Paths::resolve();
    crate::log::init(&paths.log_file());
    super::prepare_interface(&paths);
    let icon = window::icon::from_rgba(art::render(64, IconVariant::Active), 64, 64).ok();
    iced::application(move || Overlay::new(&paths, program.clone()), Overlay::update, Overlay::view)
        .title(|o: &Overlay| o.tr.get("cheat.title").to_string())
        .theme(|o: &Overlay| o.theme.clone())
        .subscription(Overlay::subscription)
        .default_font(font())
        .window(window::Settings {
            size: Size::new(620.0, 560.0),
            position: window::Position::Centered,
            decorations: false,
            resizable: false,
            level: window::Level::AlwaysOnTop,
            icon,
            platform_specific: window::settings::PlatformSpecific { skip_taskbar: true, ..Default::default() },
            ..window::Settings::default()
        })
        .run()
}

impl Overlay {
    fn new(paths: &Paths, program: String) -> (Overlay, Task<Msg>) {
        let config = config::load(&paths.config).ok().flatten().unwrap_or_default();
        let tr = tr::load(&config.settings.language);
        let dark = match config.settings.theme {
            declic_core::ThemePref::System => declic_win::system::apps_use_dark_theme(),
            declic_core::ThemePref::Light => false,
            declic_core::ThemePref::Dark => true,
        };
        let accent = declic_win::system::accent_color()
            .map(|(r, g, b)| style::adapt_accent(iced::Color::from_rgb8(r, g, b), dark))
            .unwrap_or(style::default_accent(dark));
        let current = (!program.is_empty()).then_some(program.as_str());
        let mut entries: Vec<Entry> = config
            .shortcuts
            .iter()
            .filter(|s| s.enabled && s.conditions.accepts_program(current))
            .map(|s| Entry {
                name: if s.name.trim().is_empty() {
                    super::editor::suggested_name(&tr, &s.action).unwrap_or_else(|| s.keys.to_string())
                } else {
                    s.name.clone()
                },
                keys: tr::keycaps(&tr, &s.keys),
                group: s.group.clone(),
                specific: s.conditions.program_mode == declic_core::ProgramMode::OnlyIn,
            })
            .collect();
        entries.sort_by_key(|e| (e.group.to_lowercase(), !e.specific, e.name.to_lowercase()));
        let overlay = Overlay { tr, theme: style::make_theme(dark, accent), program, entries, focused: false };
        let close_later = Task::perform(
            async {
                std::thread::sleep(AUTO_CLOSE);
            },
            |_| Msg::Close,
        );
        (overlay, Task::batch([window::latest().and_then(window::gain_focus), close_later]))
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Close => iced::exit(),
            Msg::Focused => {
                self.focused = true;
                Task::none()
            }
            Msg::Unfocused if self.focused => iced::exit(),
            Msg::Unfocused => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        iced::event::listen_with(|event, _, _| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed { .. }) => Some(Msg::Close),
            iced::Event::Window(window::Event::Focused) => Some(Msg::Focused),
            iced::Event::Window(window::Event::Unfocused) => Some(Msg::Unfocused),
            _ => None,
        })
    }

    fn view(&self) -> Element<'_, Msg> {
        let tr = &self.tr;
        let title = if self.program.is_empty() {
            tr.get("cheat.title").to_string()
        } else {
            tr.fmt("cheat.title_in", &[("program", &self.program)])
        };
        let mut list = Column::new().spacing(4);
        let mut group: Option<&str> = None;
        for e in &self.entries {
            if group != Some(e.group.as_str()) {
                group = Some(e.group.as_str());
                let label = if e.group.is_empty() { tr.get("groups.none") } else { e.group.as_str() };
                list = list.push(
                    container(text(label).size(12).font(font_semibold()).style(style::text_secondary))
                        .padding(Padding::from([8.0, 0.0]).bottom(2.0)),
                );
            }
            list = list.push(
                row![
                    text(e.name.clone()).size(14).width(Length::Fill),
                    keycaps_view(e.keys.clone()),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            );
        }
        if self.entries.is_empty() {
            list = list.push(widgets::hint(tr.get("cheat.empty")).map(|_| Msg::Close));
        }
        let content = column![
            text(title).size(20).font(font_semibold()),
            text(tr.plural("main.count", self.entries.len() as i64, &[])).size(12).style(style::text_tertiary),
            scrollable(list.padding(Padding::ZERO.right(12.0))).height(Length::Fill).style(style::scroll),
            text(tr.get("cheat.close_hint")).size(12).style(style::text_tertiary),
        ]
        .spacing(10);
        widgets::directional(container(content).padding(22).width(Length::Fill).height(Length::Fill).style(|theme: &Theme| {
            let t = style::tokens(theme);
            container::Style {
                background: Some(style::panel_background(theme).into()),
                border: iced::Border { color: t.accent, width: 1.5, radius: 8.0.into() },
                text_color: Some(t.text),
                ..Default::default()
            }
        }))
    }
}

fn keycaps_view<'a>(labels: Vec<String>) -> Element<'a, Msg> {
    widgets::keycaps(labels, 12.5, false).map(|_| Msg::Close)
}
