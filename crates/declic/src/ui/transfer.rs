//! Import / export panel (F-DAT-05, F-DAT-06).

use super::icons::Icon;
use super::style::{self, Tone};
use super::widgets::{self, ButtonKind, font_semibold};
use super::{App, Message, Panel, blocking};
use crate::tr;
use declic_core::transfer::{self, DuplicatePolicy};
use declic_core::{ActionKind, Config, ShortcutId};
use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Alignment, Element, Length, Padding, Task};
use std::path::PathBuf;

/// A file read for import, waiting for confirmation.
pub struct ImportPreview {
    pub path: PathBuf,
    pub incoming: Config,
    pub duplicates: usize,
    pub policy: DuplicatePolicy,
}

#[derive(Default)]
pub struct TransferState {
    /// Export only the selected shortcuts.
    pub selection_only: bool,
    pub import: Option<ImportPreview>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Toml,
    Csv,
}

#[derive(Debug, Clone)]
pub enum TransferMsg {
    SelectionOnly(bool),
    Export(Format),
    ExportTo(Format, Option<PathBuf>),
    CopyTable,
    ImportChoose,
    ImportFile(Option<PathBuf>),
    ImportPolicy(DuplicatePolicy),
    ImportConfirm,
    ImportCancel,
}

fn tm(msg: TransferMsg) -> Message {
    Message::Transfer(msg)
}

fn scope_ids(app: &App) -> Option<Vec<ShortcutId>> {
    app.transfer.selection_only.then(|| app.selection.iter().copied().collect())
}

/// Rows of the readable export (header first).
pub fn table_rows(app: &App, ids: Option<&[ShortcutId]>) -> Vec<Vec<String>> {
    let tr = &app.tr;
    let mut rows = vec![
        [
            "table.name",
            "table.keys",
            "table.kind",
            "table.action",
            "table.conditions",
            "table.group",
            "table.enabled",
            "table.uses",
            "table.last_used",
        ]
        .iter()
        .map(|k| tr.get(k).to_string())
        .collect(),
    ];
    for s in app.config.shortcuts.iter().filter(|s| ids.is_none_or(|ids| ids.contains(&s.id))) {
        let usage = app.stats.get(s.id);
        let kind = tr.get(match s.action.kind() {
            ActionKind::Open => "main.filter_open",
            ActionKind::Text => "main.filter_text",
            ActionKind::Macro => "main.filter_macro",
        });
        let action = match &s.action {
            declic_core::Action::Open(open) => format!("{} {}", open.target, open.arguments).trim().to_string(),
            declic_core::Action::Macro(m) => match s.action.as_text() {
                Some((t, _)) => t.to_string(),
                None => m
                    .steps
                    .iter()
                    .filter(|st| st.enabled)
                    .map(|st| super::steps::step_title(tr, st.step.kind()).to_string())
                    .collect::<Vec<_>>()
                    .join(if widgets::is_rtl() { " ← " } else { " → " }),
            },
        };
        rows.push(vec![
            app.display_name(s),
            tr::hotkey_text(tr, &s.keys),
            kind.to_string(),
            action,
            super::editor::summary_conditions(tr, &s.conditions),
            s.group.clone(),
            tr.get(if s.enabled { "table.yes" } else { "table.no" }).to_string(),
            usage.count.to_string(),
            if usage.count > 0 { crate::tr::datetime(&app.tr, usage.last_used) } else { String::new() },
        ]);
    }
    rows
}

pub fn update(app: &mut App, msg: TransferMsg) -> Task<Message> {
    match msg {
        TransferMsg::SelectionOnly(on) => app.transfer.selection_only = on && !app.selection.is_empty(),
        TransferMsg::Export(format) => {
            let tr = &app.tr;
            let (title, name, label, ext) = match format {
                Format::Toml => (tr.get("transfer.export_title"), "declic-raccourcis.toml", tr.get("transfer.toml_files"), "toml"),
                Format::Csv => (tr.get("transfer.csv_title"), "declic-raccourcis.csv", tr.get("transfer.csv_files"), "csv"),
            };
            let (title, label) = (title.to_string(), label.to_string());
            return blocking(
                move || declic_win::dialogs::save(&title, name, &label, ext),
                move |path| tm(TransferMsg::ExportTo(format, path)),
            );
        }
        TransferMsg::ExportTo(format, Some(path)) => {
            let ids = scope_ids(app);
            let content = match format {
                Format::Toml => transfer::export_text(&app.config, ids.as_deref()),
                Format::Csv => {
                    // UTF-8 with a byte order mark so that spreadsheets detect the encoding.
                    let csv = transfer::to_csv(&table_rows(app, ids.as_deref()), declic_win::system::list_separator());
                    format!("\u{feff}{csv}")
                }
            };
            let count = ids.as_ref().map(|i| i.len()).unwrap_or(app.config.shortcuts.len());
            match declic_core::config::write_atomic(&path, content.as_bytes()) {
                Ok(()) => {
                    let msg = app.tr.plural("transfer.exported", count as i64, &[("file", &path.display().to_string())]);
                    app.set_toast(msg, Tone::Accent);
                }
                Err(e) => {
                    let msg = app.tr.fmt("transfer.failed", &[("error", &e.to_string())]);
                    app.set_toast(msg, Tone::Danger);
                }
            }
        }
        TransferMsg::ExportTo(_, None) => {}
        TransferMsg::CopyTable => {
            let ids = scope_ids(app);
            let tsv = transfer::to_tsv(&table_rows(app, ids.as_deref()));
            let msg = app.tr.get("transfer.copied").to_string();
            app.set_toast(msg, Tone::Accent);
            return iced::clipboard::write(tsv);
        }
        TransferMsg::ImportChoose => {
            let tr = &app.tr;
            let (title, label, all) =
                (tr.get("transfer.import_title").to_string(), tr.get("transfer.toml_files").to_string(), tr.get("dialog.all_files").to_string());
            return blocking(
                move || declic_win::dialogs::pick_filtered(&title, &label, "*.toml", &all),
                |path| tm(TransferMsg::ImportFile(path)),
            );
        }
        TransferMsg::ImportFile(Some(path)) => {
            app.panel = Panel::Transfer;
            match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| transfer::parse_import(&t).map_err(|e| e.to_string())) {
                Ok(incoming) => {
                    let duplicates = transfer::duplicates(&app.config, &incoming.shortcuts).len();
                    app.transfer.import = Some(ImportPreview { path, incoming, duplicates, policy: DuplicatePolicy::Merge });
                }
                Err(e) => {
                    let msg = app.tr.fmt("transfer.invalid", &[("error", &e)]);
                    app.set_toast(msg, Tone::Danger);
                }
            }
        }
        TransferMsg::ImportFile(None) => {}
        TransferMsg::ImportPolicy(policy) => {
            if let Some(preview) = app.transfer.import.as_mut() {
                preview.policy = policy;
            }
        }
        TransferMsg::ImportConfirm => {
            if app.read_only() {
                return Task::none();
            }
            if let Some(preview) = app.transfer.import.take() {
                let summary = transfer::import(&mut app.config, preview.incoming, preview.policy);
                let msg = app.tr.fmt(
                    "transfer.imported",
                    &[
                        ("added", &summary.added.to_string()),
                        ("merged", &summary.merged.to_string()),
                        ("replaced", &summary.replaced.to_string()),
                        ("skipped", &summary.skipped.to_string()),
                    ],
                );
                app.set_toast(msg, Tone::Accent);
                return app.persist();
            }
        }
        TransferMsg::ImportCancel => app.transfer.import = None,
    }
    Task::none()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let tr = &app.tr;
    let selected = app.selection.len();
    let scope = row![
        button(text(tr.plural("transfer.scope_all", app.config.shortcuts.len() as i64, &[])).size(13))
            .padding(Padding::from([6.0, 12.0]))
            .style(style::button_segment(!app.transfer.selection_only))
            .on_press(tm(TransferMsg::SelectionOnly(false))),
        button(text(tr.plural("transfer.scope_selection", selected as i64, &[])).size(13))
            .padding(Padding::from([6.0, 12.0]))
            .style(style::button_segment(app.transfer.selection_only))
            .on_press_maybe((selected > 0).then_some(tm(TransferMsg::SelectionOnly(true)))),
    ]
    .spacing(6);
    let export = column![
        widgets::section_title(tr.get("transfer.export")),
        scope,
        widgets::labeled_button(Icon::Export, tr.get("transfer.export_toml"), Some(tm(TransferMsg::Export(Format::Toml))), ButtonKind::Secondary),
        widgets::hint(tr.get("transfer.export_toml_hint")),
        widgets::labeled_button(Icon::Table, tr.get("transfer.export_csv"), Some(tm(TransferMsg::Export(Format::Csv))), ButtonKind::Secondary),
        widgets::labeled_button(Icon::Copy, tr.get("transfer.copy_table"), Some(tm(TransferMsg::CopyTable)), ButtonKind::Secondary),
        widgets::hint(tr.get("transfer.readable_hint")),
    ]
    .spacing(8);
    let mut import = column![
        widgets::section_title(tr.get("transfer.import")),
        widgets::labeled_button(Icon::Import, tr.get("transfer.import_choose"), (!app.read_only()).then_some(tm(TransferMsg::ImportChoose)), ButtonKind::Secondary),
        widgets::hint(tr.get("transfer.import_hint")),
    ]
    .spacing(8);
    if let Some(preview) = &app.transfer.import {
        let file = preview.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let mut card = column![
            text(file).size(13).font(font_semibold()),
            text(tr.plural("transfer.preview", preview.incoming.shortcuts.len() as i64, &[])).size(13),
        ]
        .spacing(8);
        if preview.duplicates > 0 {
            card = card.push(text(tr.plural("transfer.duplicates", preview.duplicates as i64, &[])).size(13).style(style::text_secondary));
            let mut policies = row![].spacing(6);
            for policy in DuplicatePolicy::ALL {
                let label = tr.get(match policy {
                    DuplicatePolicy::Merge => "transfer.merge",
                    DuplicatePolicy::Replace => "transfer.replace",
                    DuplicatePolicy::Ignore => "transfer.ignore",
                });
                policies = policies.push(
                    button(text(label).size(13))
                        .padding(Padding::from([6.0, 12.0]))
                        .style(style::button_segment(preview.policy == policy))
                        .on_press(tm(TransferMsg::ImportPolicy(policy))),
                );
            }
            card = card.push(policies);
            card = card.push(widgets::hint(tr.get(match preview.policy {
                DuplicatePolicy::Merge => "transfer.merge_hint",
                DuplicatePolicy::Replace => "transfer.replace_hint",
                DuplicatePolicy::Ignore => "transfer.ignore_hint",
            })));
        }
        card = card.push(
            row![
                widgets::text_button(tr.get("transfer.import_confirm"), Some(tm(TransferMsg::ImportConfirm)), style::button_primary),
                widgets::text_button(tr.get("editor.cancel"), Some(tm(TransferMsg::ImportCancel)), style::button_secondary),
            ]
            .spacing(6),
        );
        import = import.push(container(card).padding(12).style(style::infobar(Tone::Accent)));
    }
    let body = column![export, import].spacing(26).padding(Padding::from([4.0, 18.0]).bottom(18.0).right(22.0));
    let header = row![
        text(tr.get("transfer.title")).size(18).font(font_semibold()),
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
