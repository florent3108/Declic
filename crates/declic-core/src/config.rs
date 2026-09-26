//! Reading and writing the configuration file (TOML, versioned, atomic writes).

use crate::model::Config;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Current version of the configuration file format.
pub const FORMAT_VERSION: u32 = 2;

/// Name of the configuration file.
pub const FILE_NAME: &str = "config.toml";

/// Number of previous versions kept next to the configuration file.
pub const DEFAULT_BACKUPS: usize = 5;

const HEADER: &str = "\
# Declic configuration file.
# It can be edited by hand: Declic reloads it automatically when it changes.
# Key combinations use layout-independent names, e.g. \"Ctrl+Alt+M\", \"Win+N\",
# \"Ctrl+Num1\" (numeric keypad) or \"RightCtrl+P\" (side-specific modifier).

";

/// Errors that can occur while loading a configuration.
#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Parse(String),
    /// The file was written by a newer version of Declic.
    TooNew { found: u32, supported: u32 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "{e}"),
            ConfigError::Parse(e) => write!(f, "{e}"),
            ConfigError::TooNew { found, supported } => {
                write!(f, "format version {found} is newer than the supported version {supported}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        ConfigError::Io(e)
    }
}

/// Upgrades a raw document from an older format version to the current one.
/// Version 1 is the first format, so there is nothing to migrate yet; future
/// versions add their steps here.
fn migrate(mut doc: toml::Table, from: u32) -> Result<toml::Table, ConfigError> {
    if from > FORMAT_VERSION {
        return Err(ConfigError::TooNew { found: from, supported: FORMAT_VERSION });
    }
    // 0 → 1: files without a version field are treated as version 1.
    // 1 → 2: all macro steps, groups, favorites and new settings were added;
    //        every version-1 file is a valid version-2 file.
    doc.insert("version".into(), toml::Value::Integer(FORMAT_VERSION as i64));
    Ok(doc)
}

/// Parses the text of a configuration file.
pub fn parse(text: &str) -> Result<Config, ConfigError> {
    let doc: toml::Table = text.parse().map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))?;
    let version = match doc.get("version") {
        None => 0,
        Some(toml::Value::Integer(v)) if *v >= 0 => *v as u32,
        Some(_) => return Err(ConfigError::Parse("`version` must be a positive integer".into())),
    };
    let doc = migrate(doc, version)?;
    let mut config: Config = doc.try_into().map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))?;
    config.sanitize();
    Ok(config)
}

/// Serialises a configuration to TOML text (with an explanatory header).
pub fn to_toml(config: &Config) -> String {
    let mut config = config.clone();
    config.version = FORMAT_VERSION;
    let body = toml::to_string_pretty(&config).expect("configuration is always serialisable");
    format!("{HEADER}{body}")
}

/// Loads the configuration at `path`. A missing file yields `Ok(None)`.
pub fn load(path: &Path) -> Result<Option<Config>, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => parse(text.trim_start_matches('\u{feff}')).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

fn backup_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".bak{index}"));
    path.with_file_name(name)
}

/// Writes `contents` to `path` atomically: the data is written to a temporary
/// file in the same directory, flushed to disk, then renamed over the target.
/// A crash at any point leaves either the old or the new file, never a mix.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp_name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Saves the configuration atomically, keeping up to `backups` previous
/// versions (`config.toml.bak1` being the most recent).
pub fn save(path: &Path, config: &Config, backups: usize) -> io::Result<()> {
    let text = to_toml(config);
    if backups > 0 && path.exists() {
        // Rotation failures must never prevent saving.
        let _ = fs::remove_file(backup_path(path, backups));
        for i in (1..backups).rev() {
            let from = backup_path(path, i);
            if from.exists() {
                let _ = fs::rename(&from, backup_path(path, i + 1));
            }
        }
        let _ = fs::copy(path, backup_path(path, 1));
    }
    write_atomic(path, text.as_bytes())
}

/// Chooses where the configuration lives: next to the executable when a
/// configuration file is already there ("portable" mode), otherwise in the
/// per-user application data directory.
pub fn choose_config_path(exe_dir: Option<&Path>, user_data_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = exe_dir {
        let portable = dir.join(FILE_NAME);
        if portable.is_file() {
            return portable;
        }
    }
    match user_data_dir {
        Some(dir) => dir.join("Declic").join(FILE_NAME),
        None => PathBuf::from(FILE_NAME),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("declic-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> Config {
        let mut config = Config::new();
        let mut email = Shortcut::new(1, "Ctrl+Alt+M".parse().unwrap(), Action::text("prenom.nom@exemple.fr", TextMode::Typing));
        email.name = "Adresse e-mail".into();
        let mut excel = Shortcut::new(2, "Ctrl+Shift+S".parse().unwrap(), Action::open(r"%USERPROFILE%\Documents"));
        excel.conditions.program_mode = ProgramMode::OnlyIn;
        excel.conditions.programs = vec!["excel.exe".into()];
        excel.conditions.caps_lock = LockReq::Off;
        let mut folder = Shortcut::new(3, "Ctrl+Num1".parse().unwrap(), Action::Open(OpenAction {
            target: r"C:\Projets".into(),
            arguments: "--x".into(),
            working_dir: r"C:\".into(),
            window: WindowState::Maximized,
            run_as_admin: true,
        }));
        folder.enabled = false;
        config.shortcuts = vec![email, excel, folder];
        config.settings.language = "fr".into();
        config.settings.theme = ThemePref::Dark;
        config
    }

    #[test]
    fn round_trip() {
        let config = sample();
        let text = to_toml(&config);
        assert!(text.starts_with("# Declic configuration file."));
        assert!(text.contains("version = 2"));
        assert!(text.contains("keys = \"Ctrl+Alt+M\""));
        let parsed = parse(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn hand_written_file_with_defaults() {
        let text = r#"
            version = 1
            [[shortcut]]
            id = 7
            keys = "win+n"
            action = { type = "open", target = "notepad.exe" }

            [[shortcut]]
            id = 8
            keys = "Ctrl+Alt+M"
            [shortcut.action]
            type = "macro"
            steps = [{ step = "type_text", text = "hello" }]
        "#;
        let config = parse(text).unwrap();
        assert_eq!(config.shortcuts.len(), 2);
        let s = &config.shortcuts[0];
        assert!(s.enabled);
        assert_eq!(s.keys.to_string(), "Win+N");
        assert_eq!(s.conditions, Conditions::default());
        assert_eq!(config.shortcuts[1].action.as_text(), Some(("hello", TextMode::Typing)));
        assert_eq!(config.settings, Settings::default());
    }

    #[test]
    fn readme_examples_parse() {
        // The examples are read from the READMEs themselves so that they stay in sync.
        let readmes = [
            ("README.md", include_str!("../../../README.md"), "Work", "first.last@example.com"),
            ("README.fr.md", include_str!("../../../README.fr.md"), "Travail", "prenom.nom@exemple.fr"),
        ];
        for (file, readme, group, email) in readmes {
            let start = readme.find("```toml").unwrap_or_else(|| panic!("TOML example in {file}")) + "```toml".len();
            let end = start + readme[start..].find("```").unwrap_or_else(|| panic!("end of the TOML example in {file}"));
            let config = parse(&readme[start..end]).unwrap_or_else(|e| panic!("{file}: {e:?}"));
            assert_eq!(config.version, FORMAT_VERSION, "{file}");
            assert_eq!(config.groups, vec![group.to_string()], "{file}");
            assert_eq!(config.settings.stop_key.to_string(), "Escape", "{file}");
            assert!(config.settings.cheat_sheet_keys.is_some() && config.settings.open_window_keys.is_some(), "{file}");
            assert_eq!(config.shortcuts.len(), 3, "{file}");
            assert_eq!(config.shortcuts[0].action.as_text(), Some((email, TextMode::Typing)), "{file}");
            assert_eq!(config.shortcuts[1].conditions.program_mode, ProgramMode::Except, "{file}");
            assert_eq!(config.shortcuts[1].keys.key, crate::Key::Numpad1, "{file}");
            let report = &config.shortcuts[2];
            assert!(report.favorite, "{file}");
            assert_eq!(report.group, group, "{file}");
            match &report.action {
                Action::Macro(m) => {
                    assert_eq!(m.steps.len(), 5, "{file}");
                    assert!(!m.steps[3].enabled, "{file}");
                    assert!(m.steps.iter().all(|s| crate::steps::step_problems(&s.step).is_empty()), "{file}");
                }
                other => panic!("{file}: expected a macro, got {other:?}"),
            }
        }
    }

    #[test]
    fn macro_with_every_step_kind_round_trips() {
        use crate::steps::*;
        let mut steps: Vec<MacroStep> = StepKind::ALL.iter().map(|k| MacroStep::new(k.default_step())).collect();
        steps[1].enabled = false;
        steps.push(MacroStep::new(Step::PressKeys { keys: "Ctrl+V".parse().unwrap(), repeat: 3 }));
        steps.push(MacroStep::new(Step::MoveMouse { x: -20, y: 40, origin: MoveOrigin::Window }));
        steps.push(MacroStep::new(Step::ActivateOrLaunch {
            title: "*Bloc-notes".into(),
            program: "notepad.exe".into(),
            timeout_ms: 3000,
            if_missing: IfMissing::Continue,
            launch: OpenAction { target: "notepad.exe".into(), ..OpenAction::default() },
        }));
        let mut config = Config::new();
        let mut s = Shortcut::new(1, "Win+N".parse().unwrap(), Action::Macro(Macro { steps }));
        s.group = "Bureau".into();
        s.favorite = true;
        config.shortcuts.push(s);
        config.groups = vec!["Bureau".into(), "Vide".into()];
        config.settings.stop_key = "Ctrl+Escape".parse().unwrap();
        config.settings.open_window_keys = Some("Ctrl+Alt+D".parse().unwrap());
        config.settings.notifications = false;
        config.settings.default_text_mode = TextMode::Paste;
        let text = to_toml(&config);
        assert!(text.contains("step = \"activate_or_launch\""));
        assert!(text.contains("enabled = false"));
        let parsed = parse(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn version_1_files_still_load() {
        let text = "version = 1\n[[shortcut]]\nid = 1\nkeys = \"Ctrl+Alt+M\"\n[shortcut.action]\ntype = \"macro\"\n[[shortcut.action.steps]]\nstep = \"type_text\"\ntext = \"x\"\n";
        let config = parse(text).unwrap();
        assert_eq!(config.version, FORMAT_VERSION);
        assert_eq!(config.shortcuts[0].action.as_text(), Some(("x", TextMode::Typing)));
        assert!(config.shortcuts[0].group.is_empty());
    }

    #[test]
    fn groups_are_sanitized() {
        let text = "version = 2\ngroups = [\"Travail\", \"travail\", \" \"]\n[[shortcut]]\nid = 1\ngroup = \"Perso \"\nkeys = \"F9\"\naction = { type = \"open\", target = \"x\" }\n";
        let config = parse(text).unwrap();
        assert_eq!(config.groups, vec!["Travail".to_string(), "Perso".to_string()]);
        assert_eq!(config.shortcuts[0].group, "Perso");
    }

    #[test]
    fn missing_version_is_migrated() {
        let config = parse("").unwrap();
        assert_eq!(config.version, FORMAT_VERSION);
        assert!(config.shortcuts.is_empty());
    }

    #[test]
    fn newer_version_is_refused() {
        match parse("version = 99") {
            Err(ConfigError::TooNew { found: 99, .. }) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn invalid_files_are_reported() {
        assert!(matches!(parse("version = 1\n[[shortcut]]\nid = 1\nkeys = \"Ctrl+\"\naction = { type = \"open\", target = \"x\" }"), Err(ConfigError::Parse(_))));
        assert!(matches!(parse("this is not toml"), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn save_is_atomic_and_keeps_backups() {
        let dir = temp_dir("save");
        let path = dir.join(FILE_NAME);
        assert!(load(&path).unwrap().is_none());
        let mut config = sample();
        for i in 0..4 {
            config.settings.typing_delay_ms = i;
            save(&path, &config, 2).unwrap();
        }
        let loaded = load(&path).unwrap().unwrap();
        assert_eq!(loaded.settings.typing_delay_ms, 3);
        let bak1 = load(&backup_path(&path, 1)).unwrap().unwrap();
        let bak2 = load(&backup_path(&path, 2)).unwrap().unwrap();
        assert_eq!(bak1.settings.typing_delay_ms, 2);
        assert_eq!(bak2.settings.typing_delay_ms, 1);
        assert!(!backup_path(&path, 3).exists());
        // No temporary file is left behind.
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn portable_mode_detection() {
        let dir = temp_dir("portable");
        let appdata = dir.join("appdata");
        assert_eq!(choose_config_path(Some(&dir), Some(&appdata)), appdata.join("Declic").join(FILE_NAME));
        fs::write(dir.join(FILE_NAME), "version = 1").unwrap();
        assert_eq!(choose_config_path(Some(&dir), Some(&appdata)), dir.join(FILE_NAME));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn utf8_bom_is_accepted() {
        let dir = temp_dir("bom");
        let path = dir.join(FILE_NAME);
        fs::write(&path, "\u{feff}version = 1\n").unwrap();
        assert!(load(&path).unwrap().is_some());
        let _ = fs::remove_dir_all(&dir);
    }
}
