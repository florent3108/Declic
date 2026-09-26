//! Main window: header, side panel with groups, list toolbar and rows.

use super::icons::{self, Icon};
use super::style::{self, Tone};
use super::widgets::{self, ButtonKind, Choice, font_semibold};
use super::{App, Bulk, CaptureTarget, ConfirmChoice, Example, GroupEdit, GroupFilter, Message, Panel, SortKey, editor};
use crate::tr;
use declic_core::{Action, ActionKind, Shortcut};
use iced::widget::{
    Column, Row, button, checkbox, column, container, image, pick_list, row, scrollable, space, text, text_input, toggler,
    tooltip,
};
use iced::{Alignment, Element, Length, Padding, Theme};

pub fn header(app: &App) -> Element<'_, Message> {
    let tr = &app.tr;
    let logo = container(icons::icon(Icon::Bolt, 18.0, Some(icons::on_accent))).padding(6).style(|theme: &Theme| {
        container::Style {
            background: Some(style::tokens(theme).accent.into()),
            border: iced::Border { radius: 8.0.into(), ..Default::default() },
            ..Default::default()
        }
    });
    let searching_keys = app.capture == Some(CaptureTarget::Search);
    let search_field: Element<'_, Message> = if searching_keys {
        text(tr.get("main.search_keys_active")).size(13).style(style::text_accent).into()
    } else {
        text_input(tr.get("main.search_placeholder"), &app.search)
            .id(super::search_id())
            .on_input(Message::Search)
            .size(13)
            .padding(Padding::from([7.0, 4.0]))
            .style(|theme, status| {
                let mut s = style::input(theme, status);
                s.border.width = 0.0;
                s.background = iced::Background::Color(iced::Color::TRANSPARENT);
                s
            })
            .into()
    };
    let search = container(
        row![icons::icon(Icon::Search, 16.0, Some(icons::secondary)), search_field].spacing(6).align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, 10.0]))
    .height(Length::Fixed(36.0))
    .align_y(Alignment::Center)
    .max_width(560)
    // Most of the free width goes to the search box (up to its maximum),
    // so that long placeholders in some languages are not cut.
    .width(Length::FillPortion(4))
    .style(move |theme: &Theme| {
        let t = style::tokens(theme);
        container::Style {
            background: Some(t.surface.into()),
            border: iced::Border {
                color: if searching_keys { t.accent } else { t.stroke_strong },
                width: 1.0,
                radius: style::RADIUS.into(),
            },
            ..Default::default()
        }
    });
    let search_keys_button = tooltip(
        button(icons::icon(Icon::Keyboard, 18.0, Some(if searching_keys { icons::accent } else { icons::secondary })))
            .padding(8)
            .style(style::button_ghost)
            .on_press(if searching_keys { Message::CancelCapture } else { Message::SearchByKeys }),
        container(text(tr.get("main.search_by_keys")).size(12)).padding(6).style(style::card),
        tooltip::Position::Bottom,
    );
    row![
        logo,
        text(tr.get("app.name")).size(20).font(font_semibold()),
        space::horizontal().width(Length::Fixed(12.0)),
        search,
        search_keys_button,
        space::horizontal().width(Length::FillPortion(1)),
        widgets::labeled_button(Icon::Plus, tr.get("main.new"), (!app.read_only()).then_some(Message::New), ButtonKind::Primary),
        widgets::icon_button(Icon::Import, tr.get("transfer.title"), Some(Message::OpenPanel(Panel::Transfer))),
        widgets::icon_button(Icon::Settings, tr.get("main.settings"), Some(Message::OpenPanel(Panel::Settings))),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .padding(Padding::from([14.0, 20.0]))
    .into()
}

fn side_item<'a>(
    icon: Icon,
    label: String,
    count: usize,
    selected: bool,
    message: Message,
) -> iced::widget::Button<'a, Message> {
    button(
        row![
            icons::icon(icon, 16.0, Some(if selected { icons::accent } else { icons::secondary })),
            super::fit::line(label, 13.0),
            text(count.to_string()).size(12).style(style::text_tertiary),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([7.0, 10.0]))
    .width(Length::Fill)
    .style(style::button_side(selected))
    .on_press(message)
}

pub fn sidebar(app: &App, width: f32) -> Element<'_, Message> {
    let tr = &app.tr;
    let all = app.config.shortcuts.len();
    let favorites = app.config.shortcuts.iter().filter(|s| s.favorite).count();
    let mut col = Column::new().spacing(2).width(Length::Fixed(width));
    col = col.push(side_item(Icon::Steps, tr.get("groups.all").into(), all, app.group_filter == GroupFilter::All, Message::GroupFilter(GroupFilter::All)));
    col = col.push(side_item(
        Icon::Star,
        tr.get("groups.favorites").into(),
        favorites,
        app.group_filter == GroupFilter::Favorites,
        Message::GroupFilter(GroupFilter::Favorites),
    ));
    col = col.push(space().height(10));
    col = col.push(
        row![
            text(tr.get("groups.title")).size(12).font(font_semibold()).style(style::text_secondary).width(Length::Fill),
            tooltip(
                button(icons::icon(Icon::Plus, 14.0, Some(icons::secondary)))
                    .padding(4)
                    .style(style::button_ghost)
                    .on_press_maybe((!app.read_only()).then_some(Message::GroupStartNew)),
                container(text(tr.get("groups.new")).size(12)).padding(6).style(style::card),
                tooltip::Position::Right,
            ),
        ]
        .align_y(Alignment::Center)
        .padding(Padding::from([0.0, 10.0])),
    );
    for group in &app.config.groups {
        let selected = app.group_filter == GroupFilter::Group(group.clone());
        let count = app.config.shortcuts.iter().filter(|s| &s.group == group).count();
        let editing = matches!(&app.group_edit, Some(GroupEdit::Rename { old, .. }) if old == group);
        if editing {
            if let Some(GroupEdit::Rename { name, .. }) = &app.group_edit {
                col = col.push(group_input(app, name));
            }
            continue;
        }
        col = col.push(side_item(Icon::Tag, group.clone(), count, selected, Message::GroupFilter(GroupFilter::Group(group.clone()))));
        if selected && !app.read_only() {
            if app.group_delete.as_deref() == Some(group.as_str()) {
                col = col.push(
                    container(
                        column![
                            text(tr.get("groups.delete_confirm")).size(12),
                            row![
                                widgets::text_button(tr.get("groups.delete"), Some(Message::GroupDeleteConfirmed(true)), style::button_danger),
                                widgets::text_button(tr.get("editor.cancel"), Some(Message::GroupDeleteConfirmed(false)), style::button_secondary),
                            ]
                            .spacing(6),
                        ]
                        .spacing(6),
                    )
                    .padding(8)
                    .style(style::infobar(Tone::Danger)),
                );
            } else {
                col = col.push(
                    row![
                        widgets::labeled_button(Icon::Edit, tr.get("groups.rename"), Some(Message::GroupStartRename(group.clone())), ButtonKind::Ghost),
                        widgets::labeled_button(Icon::Trash, tr.get("groups.delete"), Some(Message::GroupDelete(group.clone())), ButtonKind::Ghost)
                            .style(style::button_danger),
                    ]
                    .spacing(2)
                    .padding(Padding::ZERO.left(18.0))
                    // Long translations go onto a second line instead of being cut.
                    .wrap(),
                );
            }
        }
    }
    if let Some(GroupEdit::New(name)) = &app.group_edit {
        col = col.push(group_input(app, name));
    }
    if app.config.groups.is_empty() && app.group_edit.is_none() {
        col = col.push(container(widgets::hint(tr.get("groups.empty"))).padding(Padding::from([4.0, 10.0])));
    }
    col = col.push(space().height(10));
    col = col.push(text(tr.get("groups.types")).size(12).font(font_semibold()).style(style::text_secondary));
    let count = |k: ActionKind| app.config.shortcuts.iter().filter(|s| s.action.kind() == k).count();
    for (kind, icon, key) in [
        (ActionKind::Open, Icon::Open, "main.filter_open"),
        (ActionKind::Text, Icon::Text, "main.filter_text"),
        (ActionKind::Macro, Icon::Steps, "main.filter_macro"),
    ] {
        let selected = app.kind_filter == Some(kind);
        col = col.push(side_item(
            icon,
            tr.get(key).into(),
            count(kind),
            selected,
            Message::KindFilter(if selected { None } else { Some(kind) }),
        ));
    }
    scrollable(col.padding(Padding::ZERO.right(8.0))).height(Length::Fill).style(style::scroll).into()
}

fn group_input<'a>(app: &'a App, name: &'a str) -> Element<'a, Message> {
    let tr = &app.tr;
    row![
        text_input(tr.get("groups.name_placeholder"), name)
            .id(super::group_input_id())
            .on_input(Message::GroupInput)
            .on_submit(Message::GroupSubmit)
            .padding(Padding::from([5.0, 8.0]))
            .size(13)
            .style(style::input),
        button(icons::icon(Icon::Check, 14.0, Some(icons::accent))).padding(5).style(style::button_ghost).on_press(Message::GroupSubmit),
        button(icons::icon(Icon::Close, 14.0, Some(icons::secondary))).padding(5).style(style::button_ghost).on_press(Message::GroupCancel),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

pub fn info_bars(app: &App) -> Vec<Element<'_, Message>> {
    let tr = &app.tr;
    let mut bars = Vec::new();
    if !app.service_running {
        bars.push(widgets::infobar(
            Icon::Info,
            Tone::Warning,
            text(tr.get("main.daemon_stopped")).size(13),
            Some(widgets::text_button(tr.get("main.daemon_start"), Some(Message::StartService), style::button_secondary).into()),
        ));
    }
    if let Some(error) = &app.load_error {
        bars.push(widgets::infobar(
            Icon::Warning,
            Tone::Danger,
            text(tr.fmt("main.config_error", &[("error", error)])).size(13),
            Some(widgets::text_button(tr.get("main.reload"), Some(Message::Reload), style::button_secondary).into()),
        ));
    }
    if let Some(undo) = &app.undo {
        let label = if undo.items.len() == 1 {
            tr.fmt("main.deleted", &[("name", &app.display_name(&undo.items[0].1))])
        } else {
            tr.plural("main.deleted_many", undo.items.len() as i64, &[])
        };
        bars.push(widgets::infobar(
            Icon::Trash,
            Tone::Neutral,
            text(label).size(13),
            Some(
                row![
                    widgets::text_button(tr.get("main.undo"), Some(Message::Undo), style::button_secondary),
                    widgets::icon_button(Icon::Close, tr.get("main.close"), Some(Message::DismissUndo)),
                ]
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
            ),
        ));
    }
    if let Some((message, tone, _)) = &app.toast {
        bars.push(widgets::infobar(
            if *tone == Tone::Danger { Icon::Warning } else { Icon::Check },
            *tone,
            text(message.clone()).size(13),
            Some(widgets::icon_button(Icon::Close, tr.get("main.close"), Some(Message::DismissToast))),
        ));
    }
    if let Some((deadline, _)) = &app.test_countdown {
        let left = deadline.saturating_duration_since(std::time::Instant::now()).as_secs() + 1;
        bars.push(widgets::infobar(
            Icon::Play,
            Tone::Accent,
            text(tr.fmt("editor.test_banner", &[("n", &left.to_string())])).size(13),
            None,
        ));
    }
    bars
}

pub fn confirm_bar(app: &App) -> Element<'_, Message> {
    let tr = &app.tr;
    container(
        column![
            text(tr.get("editor.unsaved")).size(13).font(font_semibold()),
            row![
                widgets::text_button(tr.get("editor.save"), Some(Message::Confirm(ConfirmChoice::Save)), style::button_primary),
                widgets::text_button(tr.get("editor.discard"), Some(Message::Confirm(ConfirmChoice::Discard)), style::button_secondary),
                widgets::text_button(tr.get("editor.keep_editing"), Some(Message::Confirm(ConfirmChoice::Cancel)), style::button_ghost),
            ]
            .spacing(6)
            .wrap(),
        ]
        .spacing(8),
    )
    .padding(12)
    .width(Length::Fixed(app.editor_width()))
    .style(style::infobar(Tone::Warning))
    .into()
}

pub fn toolbar(app: &App) -> Element<'_, Message> {
    let tr = &app.tr;
    let visible = app.visible_shortcuts();
    let all_selected = !visible.is_empty() && visible.iter().all(|s| app.selection.contains(&s.id));
    let sorts: Vec<Choice<SortKey>> = SortKey::ALL
        .iter()
        .map(|&k| {
            Choice::new(k, tr.get(match k {
                SortKey::Created => "sort.created",
                SortKey::Name => "sort.name",
                SortKey::Keys => "sort.keys",
                SortKey::Kind => "sort.kind",
                SortKey::Uses => "sort.uses",
                SortKey::LastUsed => "sort.last_used",
            }))
        })
        .collect();
    let current_sort = sorts.iter().find(|c| c.value == app.sort).cloned();
    let mut r = row![
        // "Select all" is set apart from the sort control so that the two are not read as one.
        tooltip(
            checkbox(all_selected).on_toggle(Message::SelectAll).size(16).style(style::check),
            container(text(tr.get("bulk.select_all")).size(12)).padding(6).style(style::card),
            tooltip::Position::Bottom,
        ),
        space::horizontal().width(Length::Fixed(10.0)),
        text(tr.get("sort.label")).size(12).style(style::text_secondary),
        pick_list(sorts, current_sort, |c: Choice<SortKey>| Message::Sort(c.value))
            .text_size(13)
            .padding(Padding::from([4.0, 8.0]))
            .style(style::pick)
            .menu_style(style::menu),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if let Some(hk) = &app.search_keys {
        r = r.push(space::horizontal().width(Length::Fixed(10.0))).push(
            container(
                row![
                    icons::icon(Icon::Keyboard, 14.0, Some(icons::accent)),
                    widgets::keycaps(tr::keycaps(tr, hk), 12.0, true),
                    button(icons::icon(Icon::Close, 12.0, Some(icons::secondary)))
                        .padding(3)
                        .style(style::button_ghost)
                        .on_press(Message::ClearSearchKeys),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([3.0, 8.0]))
            .style(style::badge(Tone::Accent)),
        );
    }
    let total = tr.plural("main.count", visible.len() as i64, &[]);
    r = r.push(space::horizontal()).push(text(total).size(12).style(style::text_tertiary));
    if app.selection.is_empty() {
        return r.into();
    }
    // Actions on the selection.
    let mut groups = vec![Choice::new(String::new(), tr.get("groups.none"))];
    groups.extend(app.config.groups.iter().map(|g| Choice::new(g.clone(), g.clone())));
    let ro = app.read_only();
    let bulk = |icon: Icon, label: &'static str, action: Bulk| {
        widgets::labeled_button(icon, tr.get(label), (!ro).then_some(Message::Bulk(action)), ButtonKind::Ghost)
    };
    let bar = container(
        row![
            // The "clear" button stays next to the count: a filling space in
            // a wrapping row would push it onto a line of its own.
            widgets::icon_button(Icon::Close, tr.get("bulk.clear"), Some(Message::SelectAll(false))),
            text(tr.plural("bulk.selected", app.selection.len() as i64, &[])).size(13).font(font_semibold()),
            space::horizontal().width(8),
            bulk(Icon::Check, "bulk.enable", Bulk::Enable),
            bulk(Icon::Close, "bulk.disable", Bulk::Disable),
            bulk(Icon::Copy, "bulk.duplicate", Bulk::Duplicate),
            pick_list(groups, None::<Choice<String>>, |c: Choice<String>| Message::Bulk(Bulk::MoveTo(c.value)))
                .placeholder(tr.get("bulk.move"))
                .text_size(13)
                .padding(Padding::from([4.0, 8.0]))
                .style(style::pick)
                .menu_style(style::menu),
            bulk(Icon::Export, "bulk.export", Bulk::Export),
            bulk(Icon::Clock, "bulk.reset_stats", Bulk::ResetStats),
            bulk(Icon::Trash, "bulk.delete", Bulk::Delete).style(style::button_danger),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .wrap(),
    )
    .padding(Padding::from([4.0, 10.0]))
    .style(style::infobar(Tone::Accent));
    column![r, bar].spacing(8).into()
}

pub fn list(app: &App) -> Element<'_, Message> {
    let shortcuts = app.visible_shortcuts();
    if shortcuts.is_empty() {
        // An empty group or favourites view is not a failed search: say how to fill it.
        let filtered = !app.search.trim().is_empty() || app.search_keys.is_some() || app.kind_filter.is_some();
        let message = match &app.group_filter {
            GroupFilter::Group(_) if !filtered => "groups.empty_group",
            GroupFilter::Favorites if !filtered => "groups.no_favorites",
            _ => "main.no_results",
        };
        return container(text(app.tr.get(message)).size(14).style(style::text_secondary))
            .padding(40)
            .center_x(Length::Fill)
            .into();
    }
    let rows = shortcuts.into_iter().map(|s| row_view(app, s));
    scrollable(widgets::clip_text(Column::with_children(rows).spacing(6).padding(Padding::ZERO.right(12.0))))
        .height(Length::Fill)
        .style(style::scroll)
        .into()
}

fn kind_icon(s: &Shortcut) -> Icon {
    match &s.action {
        Action::Open(open) => {
            let t = open.target.to_ascii_lowercase();
            if t.starts_with("http://") || t.starts_with("https://") {
                Icon::Globe
            } else if t.starts_with("mailto:") {
                Icon::Mail
            } else if t.ends_with(".exe") || t.ends_with(".lnk") {
                Icon::Window
            } else if t.ends_with('\\') || !t.rsplit('\\').next().unwrap_or("").contains('.') {
                Icon::Folder
            } else {
                Icon::File
            }
        }
        Action::Macro(_) if s.action.kind() == ActionKind::Text => {
            if s.action.as_text().is_some_and(|(t, _)| t.contains("%DATE") || t.contains("%TIME")) {
                Icon::Calendar
            } else {
                Icon::Text
            }
        }
        Action::Macro(_) => Icon::Steps,
    }
}

fn summary(app: &App, s: &Shortcut) -> String {
    let text = match &s.action {
        Action::Open(open) => {
            if open.arguments.is_empty() { open.target.clone() } else { format!("{} {}", open.target, open.arguments) }
        }
        Action::Macro(m) => match s.action.as_text() {
            Some((t, _)) => declic_core::naming::excerpt(t).unwrap_or_default(),
            None => {
                let titles: Vec<&str> =
                    m.steps.iter().filter(|st| st.enabled).map(|st| super::steps::step_title(&app.tr, st.step.kind())).collect();
                titles.join(if widgets::is_rtl() { " ← " } else { " → " })
            }
        },
    };
    if text.chars().count() > 70 { text.chars().take(70).chain(['…']).collect() } else { text }
}

fn row_view<'a>(app: &'a App, s: &'a Shortcut) -> Element<'a, Message> {
    let tr = &app.tr;
    let program_icon = match &s.action {
        Action::Open(open) => app.row_icons.get(&open.target).cloned().flatten(),
        _ => None,
    };
    let icon_content: Element<'a, Message> = match program_icon {
        Some(handle) => image(handle).width(Length::Fixed(20.0)).height(Length::Fixed(20.0)).into(),
        None => icons::icon(kind_icon(s), 18.0, Some(if s.enabled { icons::accent } else { icons::secondary })).into(),
    };
    let icon_box = container(icon_content).padding(8).style(style::icon_circle);
    // Badges, most important first.
    let mut badges: Vec<(String, Tone)> = Vec::new();
    if app.conflicts.contains(&s.id) {
        badges.push((tr.get("row.conflict").to_string(), Tone::Danger));
    }
    let reserved = app.reserved.contains(&s.keys);
    if reserved {
        badges.push((tr.get("row.reserved").to_string(), Tone::Warning));
    }
    let conditions = editor::summary_conditions(tr, &s.conditions);
    if !conditions.is_empty() {
        badges.push((conditions, Tone::Accent));
    }
    if !s.group.is_empty() && !matches!(app.group_filter, GroupFilter::Group(_)) {
        badges.push((s.group.clone(), Tone::Neutral));
    }
    if !s.enabled {
        badges.push((tr.get("row.disabled").to_string(), Tone::Neutral));
    }
    let usage = app.stats.get(s.id);
    let name_style: fn(&Theme) -> iced::widget::text::Style =
        if s.enabled { |theme| iced::widget::text::Style { color: Some(style::tokens(theme).text) } } else { style::text_secondary };
    let selected = app.selection.contains(&s.id);
    let id = s.id;

    // When space runs out, the elements give way in this order: the badges
    // become a compact chip, then the usage count and finally the chip are
    // hidden. The name keeps at least ROW_NAME_MIN; the checkbox, star, icon,
    // keycaps and switch always keep their size.
    let mut children: Vec<Element<'a, Message>> = vec![
        checkbox(selected).on_toggle(move |v| Message::SelectOne(id, v)).size(15).style(style::check).into(),
        button(icons::icon(if s.favorite { Icon::StarFilled } else { Icon::Star }, 15.0, Some(if s.favorite { icons::accent } else { icons::secondary })))
            .padding(3)
            .style(style::button_ghost)
            .on_press_maybe((!app.read_only()).then_some(Message::Favorite(id, !s.favorite)))
            .into(),
        icon_box.into(),
        column![
            super::fit::line(app.display_name(s), 14.0).font(font_semibold()).style(name_style),
            super::fit::line(summary(app, s), 12.0).style(style::text_secondary),
        ]
        .spacing(2)
        .width(Length::Fill)
        .into(),
    ];
    const MAIN: usize = 3;
    let mut full_badges = None;
    let mut compact_badges = None;
    if !badges.is_empty() {
        let full = Row::with_children(badges.iter().map(|(label, tone)| {
            let badge = widgets::badge(label.clone(), *tone);
            if *tone == Tone::Warning && reserved {
                tooltip(
                    badge,
                    container(text(tr.get("editor.reserved")).size(12).width(Length::Fixed(260.0))).padding(6).style(style::card),
                    tooltip::Position::Top,
                )
                .into()
            } else {
                badge
            }
        }))
        .spacing(6)
        .align_y(Alignment::Center);
        children.push(full.into());
        full_badges = Some(children.len() - 1);
        children.push(compact_badges_chip(&badges));
        compact_badges = Some(children.len() - 1);
    }
    let mut uses = None;
    if usage.count > 0 {
        let when = crate::tr::datetime(&app.tr, usage.last_used);
        children.push(
            tooltip(
                text(tr.fmt("row.uses", &[("n", &tr.number(usage.count as i64))])).size(12).style(style::text_tertiary),
                container(text(tr.fmt("row.last_used", &[("date", &when)])).size(12)).padding(6).style(style::card),
                tooltip::Position::Top,
            )
            .into(),
        );
        uses = Some(children.len() - 1);
    }
    children.push(widgets::keycaps(tr::keycaps(tr, &s.keys), 12.5, false));
    children.push(
        tooltip(
            toggler(s.enabled)
                .on_toggle_maybe((!app.read_only()).then_some(move |v| Message::Toggle(id, v)))
                .size(18)
                .style(style::switch),
            container(text(tr.get("row.enabled_tip")).size(12)).padding(6).style(style::card),
            tooltip::Position::Left,
        )
        .into(),
    );
    let count = children.len();
    let level = |full: bool, compact: bool, with_uses: bool| -> Vec<bool> {
        (0..count)
            .map(|i| match Some(i) {
                i if i == full_badges => full,
                i if i == compact_badges => compact,
                i if i == uses => with_uses,
                _ => true,
            })
            .collect()
    };
    let levels = vec![level(true, false, true), level(false, true, true), level(false, true, false), level(false, false, false)];
    let content = super::fit::priority_row(children, MAIN, ROW_NAME_MIN, levels, 10.0);
    let highlighted = selected || (app.selected == Some(s.id) && app.editor.as_ref().is_some_and(|e| e.id == Some(s.id)));
    button(content)
        .padding(Padding::from([8.0, 12.0]))
        .width(Length::Fill)
        .style(style::button_row(highlighted))
        .on_press(Message::RowClicked(s.id))
        .into()
}

/// Minimum width kept for the name of a shortcut in the list.
const ROW_NAME_MIN: f32 = 120.0;

/// All the badges of a row as a single chip (tag icon and count); the
/// tooltip lists them.
fn compact_badges_chip<'a>(badges: &[(String, Tone)]) -> Element<'a, Message> {
    let tone = if badges.iter().any(|(_, t)| *t == Tone::Danger) {
        Tone::Danger
    } else if badges.iter().any(|(_, t)| *t == Tone::Warning) {
        Tone::Warning
    } else {
        Tone::Neutral
    };
    let color: fn(&Theme) -> iced::Color = match tone {
        Tone::Danger => icons::danger,
        Tone::Warning => icons::warning,
        _ => icons::secondary,
    };
    let chip = container(
        row![icons::icon(Icon::Tag, 12.0, Some(color)), text(badges.len().to_string()).size(11.5).font(font_semibold())]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .padding(Padding::from([2.0, 8.0]))
    .style(style::badge(tone));
    let list = Column::with_children(badges.iter().map(|(label, tone)| widgets::badge(label.clone(), *tone))).spacing(4);
    tooltip(chip, container(list).padding(6).style(style::card), tooltip::Position::Top).into()
}

pub fn empty_state(app: &App) -> Element<'_, Message> {
    let tr = &app.tr;
    let example = |icon: Icon, label: &'static str, ex: Example| {
        button(row![icons::icon(icon, 20.0, Some(icons::accent)), text(tr.get(label)).size(14)].spacing(12).align_y(Alignment::Center))
            .padding(Padding::from([12.0, 16.0]))
            .width(Length::Fixed(300.0))
            .style(style::button_row(false))
            .on_press_maybe((!app.read_only()).then_some(Message::Example(ex)))
    };
    let content = column![
        container(icons::icon(Icon::Keyboard, 40.0, Some(icons::accent))).padding(16).style(style::icon_circle),
        text(tr.get("empty.title")).size(22).font(font_semibold()),
        text(tr.get("empty.text")).size(14).style(style::text_secondary),
        column![
            example(Icon::Mail, "empty.example_email", Example::Email),
            example(Icon::Window, "empty.example_notepad", Example::Notepad),
            example(Icon::Calendar, "empty.example_date", Example::Date),
            example(Icon::Folder, "empty.example_documents", Example::Documents),
        ]
        .spacing(8),
        row![
            widgets::labeled_button(Icon::Plus, tr.get("main.new"), (!app.read_only()).then_some(Message::New), ButtonKind::Primary),
            widgets::labeled_button(Icon::Import, tr.get("transfer.import"), Some(Message::OpenPanel(Panel::Transfer)), ButtonKind::Secondary),
        ]
        .spacing(8),
    ]
    .spacing(14)
    .align_x(Alignment::Center);
    container(content).center(Length::Fill).into()
}
