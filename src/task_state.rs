//! Persistent overlay for task-backed timer countdowns, keyed by task id.
//!
//! Self-contained and separate from the daily timer store (`store.rs`), which is
//! left completely untouched. Task-backed timers persist their countdown here so
//! reopening tt resumes where it left off (mirroring how the daily store already
//! freezes ad-hoc `remaining_secs` across restarts). Writes are atomic. The file
//! is removed when there are no task-backed timers, so a tt that never sees a
//! task never leaves a stray file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::store;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayEntry {
    pub task_id: u32,
    pub remaining_secs: f64,
    /// Whether this was the single active/running timer when saved (at most one).
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub fib_alert_index: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct OverlayFile {
    #[serde(default)]
    entries: Vec<OverlayEntry>,
}

/// `<data_dir>/task-timers.json`, honoring `--test` via the existing
/// `store::is_test_mode()` flag (so it never touches store.rs).
fn overlay_path() -> Option<PathBuf> {
    let app_name = if store::is_test_mode() {
        "tt-test"
    } else {
        "tt"
    };
    ProjectDirs::from("", "", app_name).map(|p| p.data_dir().join("task-timers.json"))
}

/// Load the overlay (default location) as a map keyed by task id. Forgiving:
/// empty on any error.
pub fn load() -> HashMap<u32, OverlayEntry> {
    match overlay_path() {
        Some(path) => load_path(&path),
        None => HashMap::new(),
    }
}

/// Persist the overlay (default location). Removes the file when empty.
pub fn save(entries: &HashMap<u32, OverlayEntry>) {
    if let Some(path) = overlay_path() {
        save_path(&path, entries);
    }
}

fn load_path(path: &Path) -> HashMap<u32, OverlayEntry> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(file) = serde_json::from_str::<OverlayFile>(&contents) else {
        return HashMap::new();
    };
    file.entries.into_iter().map(|e| (e.task_id, e)).collect()
}

fn save_path(path: &Path, entries: &HashMap<u32, OverlayEntry>) {
    // No task-backed timers ⇒ don't leave a stray file around.
    if entries.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let mut list: Vec<OverlayEntry> = entries.values().cloned().collect();
    list.sort_by_key(|e| e.task_id);
    let Ok(json) = serde_json::to_string_pretty(&OverlayFile { entries: list }) else {
        return;
    };
    let _ = atomic_write(path, &json);
}

/// Write atomically: temp file in the same dir, then rename over the target.
fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp.{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("tt-overlay-test-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.push("task-timers.json");
        let _ = std::fs::remove_file(&dir);
        dir
    }

    #[test]
    fn entry_optional_fields_default() {
        let e: OverlayEntry =
            serde_json::from_str(r#"{ "task_id": 3, "remaining_secs": 12.5 }"#).unwrap();
        assert!(!e.running);
        assert_eq!(e.fib_alert_index, 0);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let path = temp_path("roundtrip");
        let mut map = HashMap::new();
        map.insert(
            1,
            OverlayEntry {
                task_id: 1,
                remaining_secs: 300.0,
                running: true,
                fib_alert_index: 2,
            },
        );
        map.insert(
            2,
            OverlayEntry {
                task_id: 2,
                remaining_secs: 60.0,
                running: false,
                fib_alert_index: 0,
            },
        );
        save_path(&path, &map);

        let loaded = load_path(&path);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.get(&1).unwrap().running);
        assert_eq!(loaded.get(&2).unwrap().remaining_secs, 60.0);
    }

    #[test]
    fn empty_map_removes_file() {
        let path = temp_path("empty");
        let mut map = HashMap::new();
        map.insert(
            1,
            OverlayEntry {
                task_id: 1,
                remaining_secs: 1.0,
                running: false,
                fib_alert_index: 0,
            },
        );
        save_path(&path, &map);
        assert!(path.exists());

        save_path(&path, &HashMap::new());
        assert!(!path.exists(), "empty overlay should remove the file");
    }
}
