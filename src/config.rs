//! `tt` configuration: a small JSON settings file plus the `tt config` CLI.
//!
//! This module is intentionally self-contained and additive — it does not touch
//! the timer/store/idle logic. The only setting is `integrate_with_task`
//! (default `true`). The file lives next to the timer data and honors `--test`.

use std::fmt;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::store;

/// The only supported config key for now.
const KEY_INTEGRATE_WITH_TASK: &str = "integrate_with_task";
const VALID_KEYS: &[&str] = &[KEY_INTEGRATE_WITH_TASK];

/// `tt config` subcommands. Defined here (not in `cli.rs`) so the config logic
/// and its CLI surface live together; `cli.rs` references it as a subcommand.
#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// List all settings as `key=value`.
    #[command(visible_alias = "ls")]
    List,

    /// Update a setting (`key=value` or `key value`).
    #[command(long_about = "\
Update a setting. Accepts either `key=value` or `key value`; the key and value are
validated before anything is written.

Keys:
  integrate_with_task=<true|false>   integrate with the ttask tool (default true)")]
    Set {
        /// `key=value` (or `key value`), e.g. `integrate_with_task=false`.
        #[arg(value_name = "KEY=VALUE", required = true)]
        args: Vec<String>,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Config {
    #[serde(default = "default_true")]
    integrate_with_task: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            integrate_with_task: true,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    UnknownKey(String),
    InvalidValue { key: String, value: String },
    BadUsage(String),
    NoConfigDir,
    Io(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKey(k) => write!(
                f,
                "unknown config key '{k}' (valid keys: {})",
                VALID_KEYS.join(", ")
            ),
            Self::InvalidValue { key, value } => write!(
                f,
                "invalid value '{value}' for '{key}' (expected a boolean: true/false)"
            ),
            Self::BadUsage(msg) => write!(f, "{msg}"),
            Self::NoConfigDir => write!(f, "could not determine config directory"),
            Self::Io(e) => write!(f, "failed to write config: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Path to `config.json`, in the same data dir tt already uses (honoring `--test`
/// via the existing `store::is_test_mode()` flag).
fn config_path() -> Option<PathBuf> {
    let app_name = if store::is_test_mode() {
        "tt-test"
    } else {
        "tt"
    };
    ProjectDirs::from("", "", app_name).map(|p| p.data_dir().join("config.json"))
}

/// Parse a boolean config value, accepting common spellings (but not nonsense).
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl Config {
    /// Validate and apply a `key`/`value` pair. Validation happens here, before
    /// any save, so an invalid key/value never mutates the stored config.
    fn set(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key {
            KEY_INTEGRATE_WITH_TASK => {
                let parsed = parse_bool(value).ok_or_else(|| ConfigError::InvalidValue {
                    key: key.to_string(),
                    value: value.to_string(),
                })?;
                self.integrate_with_task = parsed;
                Ok(())
            }
            _ => Err(ConfigError::UnknownKey(key.to_string())),
        }
    }

    /// All settings as `(key, value)` pairs, for `tt config list`.
    fn pairs(&self) -> Vec<(String, String)> {
        vec![(
            KEY_INTEGRATE_WITH_TASK.to_string(),
            self.integrate_with_task.to_string(),
        )]
    }
}

/// Load a config from a file, falling back to defaults on any error.
fn load_path(path: &Path) -> Config {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Config::default();
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

/// Write a config to a file (creating the parent dir if needed).
fn save_path(path: &Path, cfg: &Config) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::Io(e.to_string()))?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| ConfigError::Io(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| ConfigError::Io(e.to_string()))
}

/// Split the args after `set` into a `(key, value)` pair, accepting both
/// `key=value` and `key value` forms.
fn parse_set(rest: &[String]) -> Result<(String, String), ConfigError> {
    let first = rest
        .first()
        .ok_or_else(|| ConfigError::BadUsage("usage: tt config set <key>=<value>".to_string()))?;

    if let Some((k, v)) = first.split_once('=') {
        return Ok((k.trim().to_string(), v.trim().to_string()));
    }
    if rest.len() >= 2 {
        let value = rest[1..].join(" ");
        return Ok((first.trim().to_string(), value.trim().to_string()));
    }
    Err(ConfigError::BadUsage(format!(
        "missing value for '{first}' (use `key=value` or `key value`)"
    )))
}

/// Whether `ttask` integration is enabled (loads the config; default `true`).
/// The single value the rest of tt needs from config.
pub fn integrate_with_task() -> bool {
    match config_path() {
        Some(path) => load_path(&path).integrate_with_task,
        None => Config::default().integrate_with_task,
    }
}

/// Run a parsed `tt config` subcommand. Returns the message to print on success,
/// or a `ConfigError` to report. (clap handles `--help` and unknown subcommands.)
pub fn run(action: &ConfigAction) -> Result<String, ConfigError> {
    let path = config_path().ok_or(ConfigError::NoConfigDir)?;
    run_at(action, &path)
}

/// `run` against an explicit config file (for tests / isolation).
fn run_at(action: &ConfigAction, path: &Path) -> Result<String, ConfigError> {
    match action {
        ConfigAction::List => {
            let cfg = load_path(path);
            Ok(cfg
                .pairs()
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        ConfigAction::Set { args } => {
            let (key, value) = parse_set(args)?;
            let mut cfg = load_path(path);
            cfg.set(&key, &value)?; // validates before saving
            save_path(path, &cfg)?;
            let normalized = cfg
                .pairs()
                .into_iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v)
                .unwrap_or(value);
            Ok(format!("{key}={normalized}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique, isolated config path under the system temp dir (no shared state,
    /// no touching the real tt data dir).
    fn temp_path(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("tt-config-test-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.push("config.json");
        let _ = std::fs::remove_file(&dir);
        dir
    }

    #[test]
    fn default_enables_integration() {
        assert!(Config::default().integrate_with_task);
    }

    #[test]
    fn deserialize_missing_field_defaults_to_true() {
        let cfg: Config = serde_json::from_str("{}").unwrap();
        assert!(cfg.integrate_with_task);
    }

    #[test]
    fn parse_bool_accepts_common_spellings() {
        for t in ["true", "TRUE", "1", "yes", "on"] {
            assert_eq!(parse_bool(t), Some(true), "{t}");
        }
        for fa in ["false", "False", "0", "no", "off"] {
            assert_eq!(parse_bool(fa), Some(false), "{fa}");
        }
        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(parse_bool(""), None);
    }

    #[test]
    fn set_rejects_unknown_key() {
        let mut cfg = Config::default();
        let err = cfg.set("nope", "true").unwrap_err();
        assert!(matches!(err, ConfigError::UnknownKey(k) if k == "nope"));
        assert!(cfg.integrate_with_task); // unchanged
    }

    #[test]
    fn set_rejects_invalid_value() {
        let mut cfg = Config::default();
        let err = cfg.set(KEY_INTEGRATE_WITH_TASK, "banana").unwrap_err();
        assert!(matches!(err, ConfigError::InvalidValue { .. }));
        assert!(cfg.integrate_with_task); // unchanged
    }

    #[test]
    fn parse_set_equals_and_space_forms() {
        let (k, v) = parse_set(&["integrate_with_task=false".to_string()]).unwrap();
        assert_eq!((k.as_str(), v.as_str()), ("integrate_with_task", "false"));
        let (k, v) = parse_set(&["integrate_with_task".to_string(), "true".to_string()]).unwrap();
        assert_eq!((k.as_str(), v.as_str()), ("integrate_with_task", "true"));
    }

    #[test]
    fn parse_set_missing_value_errors() {
        let err = parse_set(&["integrate_with_task".to_string()]).unwrap_err();
        assert!(matches!(err, ConfigError::BadUsage(_)));
    }

    fn set(tokens: &[&str]) -> ConfigAction {
        ConfigAction::Set {
            args: tokens.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn run_set_then_list_roundtrip() {
        let path = temp_path("roundtrip");
        assert_eq!(
            run_at(&set(&["integrate_with_task=false"]), &path).unwrap(),
            "integrate_with_task=false"
        );
        assert_eq!(
            run_at(&ConfigAction::List, &path).unwrap(),
            "integrate_with_task=false"
        );
        // space form, then read back
        run_at(&set(&["integrate_with_task", "true"]), &path).unwrap();
        assert_eq!(
            run_at(&ConfigAction::List, &path).unwrap(),
            "integrate_with_task=true"
        );
    }

    #[test]
    fn run_set_invalid_value_does_not_persist() {
        let path = temp_path("invalid");
        run_at(&set(&["integrate_with_task=true"]), &path).unwrap();
        let err = run_at(&set(&["integrate_with_task=banana"]), &path).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidValue { .. }));
        assert_eq!(
            run_at(&ConfigAction::List, &path).unwrap(),
            "integrate_with_task=true" // bad value never reached disk
        );
    }

    #[test]
    fn run_set_unknown_key_does_not_persist() {
        let path = temp_path("unknown");
        let err = run_at(&set(&["bogus=1"]), &path).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownKey(_)));
        // Nothing written ⇒ list shows the default.
        assert_eq!(
            run_at(&ConfigAction::List, &path).unwrap(),
            "integrate_with_task=true"
        );
    }
}
