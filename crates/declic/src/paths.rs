//! Locations of the configuration, log and translation files.

use declic_core::config;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    /// The configuration file.
    pub config: PathBuf,
    /// Folder containing the configuration file.
    pub dir: PathBuf,
    /// Configuration stored next to the executable.
    pub portable: bool,
}

impl Paths {
    pub fn resolve() -> Paths {
        let exe_dir = declic_win::system::exe_dir();
        let config = config::choose_config_path(exe_dir.as_deref(), declic_win::system::app_data_dir().as_deref());
        let dir = config.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let portable = exe_dir.is_some_and(|d| d == dir);
        Paths { config, dir, portable }
    }

    pub fn stats_file(&self) -> PathBuf {
        self.dir.join(declic_core::stats::FILE_NAME)
    }

    pub fn log_file(&self) -> PathBuf {
        self.dir.join("declic.log")
    }
}

/// Folder where external translation files may be placed (`lang` next to the executable).
pub fn lang_dir() -> Option<PathBuf> {
    declic_win::system::exe_dir().map(|d| d.join("lang"))
}
