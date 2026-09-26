//! Usage statistics (F-DAT-07): number of uses and last use per shortcut.
//!
//! Statistics live in their own file (`stats.toml`, next to the
//! configuration) so that counting a use never rewrites the configuration
//! or its backups. The background service is the only writer; the main
//! window reads it and asks the service to reset counters.

use crate::model::ShortcutId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const FILE_NAME: &str = "stats.toml";

/// Usage of one shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub count: u64,
    /// Seconds since 1970-01-01 UTC.
    pub last_used: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
struct StatsFile {
    #[serde(default)]
    usage: BTreeMap<String, Usage>,
}

/// Usage statistics of all shortcuts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stats {
    usage: BTreeMap<ShortcutId, Usage>,
}

/// Current time in seconds since the Unix epoch.
pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Stats {
    pub fn get(&self, id: ShortcutId) -> Usage {
        self.usage.get(&id).copied().unwrap_or_default()
    }

    /// Records one use at `now` (Unix seconds).
    pub fn record(&mut self, id: ShortcutId, now: u64) {
        let usage = self.usage.entry(id).or_default();
        usage.count += 1;
        usage.last_used = now;
    }

    /// Resets one shortcut, or all of them.
    pub fn reset(&mut self, id: Option<ShortcutId>) {
        match id {
            Some(id) => {
                self.usage.remove(&id);
            }
            None => self.usage.clear(),
        }
    }

    /// Forgets the statistics of shortcuts that no longer exist.
    pub fn retain(&mut self, exists: impl Fn(ShortcutId) -> bool) {
        self.usage.retain(|id, _| exists(*id));
    }

    /// Removes and returns the statistics of shortcuts that no longer exist,
    /// so that they can be put back if the deletion is undone.
    pub fn take_missing(&mut self, exists: impl Fn(ShortcutId) -> bool) -> Vec<(ShortcutId, Usage)> {
        let missing: Vec<ShortcutId> = self.usage.keys().copied().filter(|id| !exists(*id)).collect();
        missing.into_iter().filter_map(|id| self.usage.remove(&id).map(|u| (id, u))).collect()
    }

    /// Puts back statistics taken by [`Stats::take_missing`], unless the
    /// shortcut was used again in the meantime.
    pub fn restore(&mut self, id: ShortcutId, usage: Usage) {
        self.usage.entry(id).or_insert(usage);
    }

    pub fn parse(text: &str) -> Stats {
        let file: StatsFile = toml::from_str(text).unwrap_or_default();
        Stats { usage: file.usage.into_iter().filter_map(|(k, v)| k.parse().ok().map(|id| (id, v))).collect() }
    }

    pub fn to_toml(&self) -> String {
        let file = StatsFile { usage: self.usage.iter().map(|(k, v)| (k.to_string(), *v)).collect() };
        format!(
            "# Declic usage statistics (number of uses, last use in seconds since 1970).\n\n{}",
            toml::to_string(&file).unwrap_or_default()
        )
    }

    /// Loads the statistics; a missing or invalid file gives empty statistics.
    pub fn load(path: &Path) -> Stats {
        std::fs::read_to_string(path).map(|t| Stats::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        crate::config::write_atomic(path, self.to_toml().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_reset_and_round_trip() {
        let mut stats = Stats::default();
        stats.record(3, 100);
        stats.record(3, 200);
        stats.record(7, 150);
        assert_eq!(stats.get(3), Usage { count: 2, last_used: 200 });
        assert_eq!(stats.get(9), Usage::default());
        let parsed = Stats::parse(&stats.to_toml());
        assert_eq!(parsed, stats);
        stats.reset(Some(3));
        assert_eq!(stats.get(3).count, 0);
        stats.retain(|id| id != 7);
        assert_eq!(stats.get(7).count, 0);
        stats.record(1, 1);
        stats.reset(None);
        assert_eq!(stats, Stats::default());
    }

    #[test]
    fn missing_statistics_can_be_restored() {
        let mut stats = Stats::default();
        stats.record(1, 10);
        stats.record(2, 20);
        let taken = stats.take_missing(|id| id == 1);
        assert_eq!(taken, vec![(2, Usage { count: 1, last_used: 20 })]);
        assert_eq!(stats.get(2).count, 0);
        stats.restore(2, taken[0].1);
        assert_eq!(stats.get(2), Usage { count: 1, last_used: 20 });
        // A newer use is not overwritten.
        stats.record(1, 30);
        stats.restore(1, Usage { count: 9, last_used: 5 });
        assert_eq!(stats.get(1), Usage { count: 2, last_used: 30 });
    }

    #[test]
    fn invalid_file_gives_empty_stats() {
        assert_eq!(Stats::parse("not toml ["), Stats::default());
        assert!(now_unix() > 1_700_000_000);
    }
}
