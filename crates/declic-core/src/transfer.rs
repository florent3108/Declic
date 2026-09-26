//! Import and export of shortcuts (F-DAT-05) and readable exports (F-DAT-06).

use crate::config::{self, ConfigError};
use crate::conflict::conditions_overlap;
use crate::engine::Specificity;
use crate::model::{Config, Shortcut, ShortcutId};

/// What to do with an imported shortcut that duplicates an existing one
/// (same combination, overlapping conditions, same specificity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DuplicatePolicy {
    /// Keep both: the imported shortcut is added *disabled*, so that no
    /// silent conflict is created; the user chooses later which one to keep.
    #[default]
    Merge,
    /// The imported shortcut replaces the existing one (at its position).
    Replace,
    /// The imported shortcut is not imported.
    Ignore,
}

impl DuplicatePolicy {
    pub const ALL: [DuplicatePolicy; 3] = [DuplicatePolicy::Merge, DuplicatePolicy::Replace, DuplicatePolicy::Ignore];
}

/// Result of an import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportSummary {
    pub added: usize,
    pub merged: usize,
    pub replaced: usize,
    pub skipped: usize,
}

/// Configuration containing only the given shortcuts (all when `ids` is
/// `None`) and the groups they use; settings are not exported.
pub fn export(config: &Config, ids: Option<&[ShortcutId]>) -> Config {
    let mut out = Config::new();
    out.shortcuts = config.shortcuts.iter().filter(|s| ids.is_none_or(|ids| ids.contains(&s.id))).cloned().collect();
    out.groups = config.groups.iter().filter(|g| out.shortcuts.iter().any(|s| &s.group == *g)).cloned().collect();
    out
}

/// Text of an export file (same format as the configuration file).
pub fn export_text(config: &Config, ids: Option<&[ShortcutId]>) -> String {
    config::to_toml(&export(config, ids))
}

/// Reads an export (or configuration) file for import.
pub fn parse_import(text: &str) -> Result<Config, ConfigError> {
    config::parse(text.trim_start_matches('\u{feff}'))
}

fn is_duplicate(a: &Shortcut, b: &Shortcut) -> bool {
    a.keys.overlaps(&b.keys) && conditions_overlap(&a.conditions, &b.conditions) && Specificity::of(a) == Specificity::of(b)
}

/// For each incoming shortcut that duplicates an existing one: (index in
/// `incoming`, id of the existing shortcut).
pub fn duplicates(config: &Config, incoming: &[Shortcut]) -> Vec<(usize, ShortcutId)> {
    incoming
        .iter()
        .enumerate()
        .filter_map(|(i, s)| config.shortcuts.iter().find(|e| is_duplicate(e, s)).map(|e| (i, e.id)))
        .collect()
}

/// Imports shortcuts into `config`. Imported shortcuts get new identifiers;
/// their groups are created when needed.
pub fn import(config: &mut Config, incoming: Config, policy: DuplicatePolicy) -> ImportSummary {
    let mut summary = ImportSummary::default();
    for group in &incoming.groups {
        config.ensure_group(group);
    }
    for mut shortcut in incoming.shortcuts {
        if let Some(group) = config.ensure_group(&shortcut.group) {
            shortcut.group = group;
        }
        let duplicate = config.shortcuts.iter().position(|e| is_duplicate(e, &shortcut));
        shortcut.id = config.next_id();
        match (duplicate, policy) {
            (None, _) => {
                config.shortcuts.push(shortcut);
                summary.added += 1;
            }
            (Some(_), DuplicatePolicy::Ignore) => summary.skipped += 1,
            (Some(index), DuplicatePolicy::Replace) => {
                config.shortcuts[index] = shortcut;
                summary.replaced += 1;
            }
            (Some(_), DuplicatePolicy::Merge) => {
                shortcut.enabled = false;
                config.shortcuts.push(shortcut);
                summary.merged += 1;
            }
        }
    }
    summary
}

fn csv_field(field: &str, separator: char) -> String {
    if field.contains(separator) || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// CSV text (RFC 4180 quoting, CRLF line ends) with the given separator.
pub fn to_csv(rows: &[Vec<String>], separator: char) -> String {
    let mut out = String::new();
    for row in rows {
        let line: Vec<String> = row.iter().map(|f| csv_field(f, separator)).collect();
        out.push_str(&line.join(&separator.to_string()));
        out.push_str("\r\n");
    }
    out
}

/// Tab-separated text, pasted as a table by spreadsheets and word processors.
pub fn to_tsv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let line: Vec<String> = row.iter().map(|f| f.replace(['\t', '\r', '\n'], " ")).collect();
        out.push_str(&line.join("\t"));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn shortcut(id: u32, keys: &str, text: &str) -> Shortcut {
        Shortcut::new(id, keys.parse().unwrap(), Action::text(text, TextMode::Typing))
    }

    fn config_with(shortcuts: Vec<Shortcut>) -> Config {
        let mut c = Config::new();
        c.shortcuts = shortcuts;
        c
    }

    #[test]
    fn export_selection_with_groups() {
        let mut a = shortcut(1, "Ctrl+Alt+A", "a");
        a.group = "Travail".into();
        let b = shortcut(2, "Ctrl+Alt+B", "b");
        let mut config = config_with(vec![a, b]);
        config.groups = vec!["Travail".into(), "Perso".into()];
        let out = export(&config, Some(&[1]));
        assert_eq!(out.shortcuts.len(), 1);
        assert_eq!(out.groups, vec!["Travail".to_string()]);
        let parsed = parse_import(&export_text(&config, None)).unwrap();
        assert_eq!(parsed.shortcuts.len(), 2);
        assert_eq!(parsed.settings, Settings::default());
    }

    fn incoming() -> Config {
        let mut dup = shortcut(1, "Ctrl+Alt+M", "imported");
        dup.group = "Import".into();
        config_with(vec![dup, shortcut(2, "Ctrl+Alt+N", "new")])
    }

    #[test]
    fn duplicates_are_detected() {
        let config = config_with(vec![shortcut(5, "Ctrl+Alt+M", "existing")]);
        assert_eq!(duplicates(&config, &incoming().shortcuts), vec![(0, 5)]);
    }

    #[test]
    fn import_policies() {
        let base = config_with(vec![shortcut(5, "Ctrl+Alt+M", "existing")]);

        let mut c = base.clone();
        let s = import(&mut c, incoming(), DuplicatePolicy::Ignore);
        assert_eq!(s, ImportSummary { added: 1, skipped: 1, ..Default::default() });
        assert_eq!(c.shortcuts.len(), 2);
        assert_eq!(c.shortcuts[0].action.as_text().unwrap().0, "existing");

        let mut c = base.clone();
        let s = import(&mut c, incoming(), DuplicatePolicy::Replace);
        assert_eq!(s, ImportSummary { added: 1, replaced: 1, ..Default::default() });
        assert_eq!(c.shortcuts.len(), 2);
        assert_eq!(c.shortcuts[0].action.as_text().unwrap().0, "imported");
        assert_eq!(c.groups, vec!["Import".to_string()]);

        let mut c = base.clone();
        let s = import(&mut c, incoming(), DuplicatePolicy::Merge);
        assert_eq!(s, ImportSummary { added: 1, merged: 1, ..Default::default() });
        assert_eq!(c.shortcuts.len(), 3);
        let merged = c.shortcuts.iter().find(|s| s.action.as_text().unwrap().0 == "imported").unwrap();
        assert!(!merged.enabled, "merged duplicate is disabled");
        // Identifiers stay unique.
        let mut ids: Vec<_> = c.shortcuts.iter().map(|s| s.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn csv_and_tsv() {
        let rows = vec![
            vec!["Nom".to_string(), "Texte".to_string()],
            vec!["Signature".to_string(), "Marie; \"MD\"\nParis".to_string()],
        ];
        assert_eq!(to_csv(&rows, ';'), "Nom;Texte\r\nSignature;\"Marie; \"\"MD\"\"\nParis\"\r\n");
        assert_eq!(to_tsv(&rows), "Nom\tTexte\r\nSignature\tMarie; \"MD\" Paris\r\n");
    }
}
