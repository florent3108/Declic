//! Settings panel (§ 9).

use super::icons::{self, Icon};
use super::widgets::{self, ButtonKind, Choice, font_semibold};
use super::{App, CaptureTarget, Message, Panel, SettingKey, style};
use crate::tr;
use declic_core::{Hotkey, TextMode, ThemePref};
use iced::widget::{button, column, container, pick_list, row, scrollable, space, text, toggler};
use iced::{Alignment, Element, Length, Padding};

const DELAYS: [u32; 5] = [0, 5, 10, 20, 50];

fn pick<'a, T: Clone + PartialEq + 'a>(
    choices: Vec<Choice<T>>,
    current: &T,
    on: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let selected = choices.iter().find(|c| c.value == *current).cloned();
    pick_list(choices, selected, move |c: Choice<T>| on(c.value))
        .text_size(13)
        .padding(Padding::from([6.0, 10.0]))
        .width(Length::Fill)
        .style(style::pick)
        .menu_style(style::menu)
        .into()
}

/// A setting holding a key combination: keycaps, record button, clear button.
fn combo_setting<'a>(app: &'a App, label: &'a str, value: Option<&Hotkey>, key: SettingKey, clear_label: &'a str) -> Element<'a, Message> {
    let tr = &app.tr;
    let target = CaptureTarget::Setting(key);
    let capturing = app.capture == Some(target);
    let content: Element<'a, Message> = if capturing {
        text(tr.get("editor.capture_prompt")).size(12.5).style(style::text_accent).into()
    } else {
        match value {
            Some(hk) => widgets::keycaps(tr::keycaps(tr, hk), 12.5, false),
            None => text(tr.get("settings.no_combination")).size(12.5).style(style::text_tertiary).into(),
        }
    };
    column![
        widgets::field_label(label),
        row![
            button(row![icons::icon(Icon::Keyboard, 15.0, Some(icons::secondary)), content].spacing(8).align_y(Alignment::Center))
                .padding(Padding::from([7.0, 10.0]))
                .width(Length::Fill)
                .style(style::capture_box(capturing))
                .on_press(if capturing { Message::CancelCapture } else { Message::StartCapture(target) }),
            widgets::text_button(clear_label, (value.is_some() && !capturing).then_some(Message::ClearSettingKeys(key)), style::button_ghost),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(6)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let tr = &app.tr;
    let s = &app.config.settings;
    let system_name = {
        let code = crate::tr::resolve_among("auto", &app.languages);
        app.languages.iter().find(|(c, _)| *c == code).map(|(_, n)| n.clone()).unwrap_or(code)
    };
    let mut languages = vec![Choice::new("auto".to_string(), tr.fmt("settings.language_auto", &[("name", &system_name)]))];
    languages.extend(app.languages.iter().map(|(code, name)| Choice::new(code.clone(), name.clone())));
    let themes: Vec<Choice<ThemePref>> = ThemePref::ALL
        .iter()
        .map(|&t| {
            Choice::new(t, tr.get(match t {
                ThemePref::System => "settings.theme_system",
                ThemePref::Light => "settings.theme_light",
                ThemePref::Dark => "settings.theme_dark",
            }))
        })
        .collect();
    let delays: Vec<Choice<u32>> = DELAYS
        .iter()
        .map(|&ms| {
            let label = if ms == 0 {
                tr.get("settings.typing_delay_none").to_string()
            } else {
                tr.fmt("settings.typing_delay_ms", &[("n", &ms.to_string())])
            };
            Choice::new(ms, label)
        })
        .collect();
    let modes = vec![
        Choice::new(TextMode::Typing, tr.get("steps.mode_typing")),
        Choice::new(TextMode::Paste, tr.get("steps.mode_paste")),
    ];

    let general = column![
        widgets::section_title(tr.get("settings.general")),
        column![widgets::field_label(tr.get("settings.language")), pick(languages, &s.language, Message::SetLanguage)].spacing(6),
        column![widgets::field_label(tr.get("settings.theme")), pick(themes, &s.theme, Message::SetTheme)].spacing(6),
        column![
            toggler(app.autostart).label(tr.get("settings.autostart")).on_toggle(Message::SetAutostart).size(18).text_size(13).style(style::switch),
            widgets::hint(tr.get("settings.autostart_hint")),
        ]
        .spacing(6),
        column![
            toggler(s.notifications)
                .label(tr.get("settings.notifications"))
                .on_toggle(Message::SetNotifications)
                .size(18)
                .text_size(13)
                .style(style::switch),
            widgets::hint(tr.get("settings.notifications_hint")),
        ]
        .spacing(6),
    ]
    .spacing(14);

    let combos = column![
        widgets::section_title(tr.get("settings.global")),
        combo_setting(app, tr.get("settings.open_window_keys"), s.open_window_keys.as_ref(), SettingKey::OpenWindow, tr.get("settings.clear")),
        combo_setting(app, tr.get("settings.cheat_sheet_keys"), s.cheat_sheet_keys.as_ref(), SettingKey::CheatSheet, tr.get("settings.clear")),
        widgets::hint(tr.get("settings.global_hint")),
    ]
    .spacing(14);

    let typing = column![
        widgets::section_title(tr.get("settings.typing")),
        column![widgets::field_label(tr.get("settings.default_text_mode")), pick(modes, &s.default_text_mode, Message::SetDefaultTextMode)].spacing(6),
        column![
            widgets::field_label(tr.get("settings.typing_delay")),
            pick(delays, &s.typing_delay_ms, Message::SetTypingDelay),
            widgets::hint(tr.get("settings.typing_delay_hint")),
        ]
        .spacing(6),
        combo_setting(app, tr.get("settings.stop_key"), Some(&s.stop_key), SettingKey::Stop, tr.get("settings.reset")),
        widgets::hint(tr.get("settings.stop_key_hint")),
    ]
    .spacing(14);

    let mut data = column![
        widgets::section_title(tr.get("settings.data")),
        widgets::field_label(tr.get("settings.config")),
        text(app.paths.config.display().to_string())
            .size(12)
            .style(style::text_secondary)
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
    ]
    .spacing(8);
    if app.paths.portable {
        data = data.push(widgets::hint(tr.get("settings.config_portable")));
    }
    data = data.push(
        row![
            widgets::labeled_button(Icon::Folder, tr.get("settings.open_folder"), Some(Message::OpenConfigFolder), ButtonKind::Secondary),
            widgets::labeled_button(Icon::File, tr.get("settings.open_log"), Some(Message::OpenLog), ButtonKind::Secondary),
        ]
        .spacing(6)
        .wrap(),
    );
    data = data.push(widgets::labeled_button(Icon::Trash, tr.get("settings.reset_all_stats"), Some(Message::ResetStats(None)), ButtonKind::Ghost));

    let about = column![
        widgets::hint(tr.fmt("settings.about", &[("version", env!("CARGO_PKG_VERSION"))])),
        widgets::hint(tr.get("settings.license")),
    ]
    .spacing(4);

    let body = column![general, combos, typing, data, about].spacing(28).padding(Padding::from([4.0, 18.0]).bottom(18.0).right(22.0));
    let header = row![
        text(tr.get("settings.title")).size(18).font(font_semibold()),
        space::horizontal(),
        widgets::icon_button(Icon::Close, tr.get("settings.close"), Some(Message::OpenPanel(Panel::None))),
    ]
    .align_y(Alignment::Center)
    .padding(Padding::from([14.0, 18.0]).bottom(6.0).right(12.0));
    container(column![header, scrollable(widgets::clip_text(body)).height(Length::Fill).style(style::scroll)])
        .width(Length::Fixed(app.panel_width()))
        .height(Length::Fill)
        .style(style::panel)
        .into()
}
