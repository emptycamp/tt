use std::fs;
use std::path::PathBuf;

use chrono::Local;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::timer::{Timer, TimerState};

#[derive(Serialize, Deserialize)]
struct StoreData {
    date: String,
    active_id: Option<u32>,
    timers: Vec<TimerData>,
    #[serde(default, alias = "all_paused_secs")]
    time_debt_secs: f64,
}

#[derive(Serialize, Deserialize)]
struct TimerData {
    id: u32,
    name: String,
    original_secs: f64,
    remaining_secs: f64,
    fib_alert_index: usize,
}

fn data_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "tt").map(|p| p.data_dir().to_path_buf())
}

fn store_path() -> Option<PathBuf> {
    let date = Local::now().format("%Y-%m-%d").to_string();
    data_dir().map(|d| d.join(format!("timers-{date}.json")))
}

pub fn save(timers: &[Timer], active_id: Option<u32>, time_debt_secs: f64) {
    let Some(path) = store_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let data = StoreData {
        date: Local::now().format("%Y-%m-%d").to_string(),
        active_id,
        time_debt_secs,
        timers: timers
            .iter()
            .map(|t| TimerData {
                id: t.id,
                name: t.name.clone(),
                original_secs: t.original_secs,
                remaining_secs: t.remaining_secs,
                fib_alert_index: t.fib_alert_index,
            })
            .collect(),
    };

    if let Ok(json) = serde_json::to_string_pretty(&data) {
        let _ = fs::write(&path, json);
    }
}

pub fn load() -> (Vec<Timer>, Option<u32>, f64) {
    let Some(path) = store_path() else {
        return (vec![], None, 0.0);
    };

    let Ok(contents) = fs::read_to_string(&path) else {
        return (vec![], None, 0.0);
    };

    let Ok(data) = serde_json::from_str::<StoreData>(&contents) else {
        return (vec![], None, 0.0);
    };

    let today = Local::now().format("%Y-%m-%d").to_string();
    if data.date != today {
        let _ = fs::remove_file(&path);
        return (vec![], None, 0.0);
    }

    let time_debt_secs = data.time_debt_secs;

    let mut timers: Vec<Timer> = data
        .timers
        .into_iter()
        .map(|td| {
            let mut t = Timer::new(td.id, td.name, td.original_secs);
            t.remaining_secs = td.remaining_secs;
            t.fib_alert_index = td.fib_alert_index;
            t.state = TimerState::Paused;
            t.last_tick = None;
            t
        })
        .collect();

    if let Some(aid) = data.active_id {
        if let Some(t) = timers.iter_mut().find(|t| t.id == aid) {
            t.resume();
        }
    }

    (timers, data.active_id, time_debt_secs)
}

pub fn clear() {
    if let Some(dir) = data_dir() {
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_data_roundtrip() {
        let data = StoreData {
            date: "2025-01-15".into(),
            active_id: Some(2),
            time_debt_secs: 42.5,
            timers: vec![
                TimerData {
                    id: 1,
                    name: "meeting".into(),
                    original_secs: 300.0,
                    remaining_secs: 120.0,
                    fib_alert_index: 0,
                },
                TimerData {
                    id: 2,
                    name: "standup".into(),
                    original_secs: 600.0,
                    remaining_secs: -30.0,
                    fib_alert_index: 3,
                },
            ],
        };

        let json = serde_json::to_string(&data).unwrap();
        let parsed: StoreData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.date, "2025-01-15");
        assert_eq!(parsed.active_id, Some(2));
        assert!((parsed.time_debt_secs - 42.5).abs() < 0.001);
        assert_eq!(parsed.timers.len(), 2);
        assert_eq!(parsed.timers[0].name, "meeting");
        assert_eq!(parsed.timers[1].remaining_secs, -30.0);
        assert_eq!(parsed.timers[1].fib_alert_index, 3);
    }

    #[test]
    fn backward_compat_all_paused_secs_alias() {
        let json = r#"{
            "date": "2025-01-15",
            "active_id": null,
            "all_paused_secs": 99.9,
            "timers": []
        }"#;
        let parsed: StoreData = serde_json::from_str(json).unwrap();
        assert!((parsed.time_debt_secs - 99.9).abs() < 0.001);
    }

    #[test]
    fn time_debt_defaults_to_zero() {
        let json = r#"{
            "date": "2025-01-15",
            "active_id": null,
            "timers": []
        }"#;
        let parsed: StoreData = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.time_debt_secs, 0.0);
    }

    #[test]
    fn no_active_id() {
        let data = StoreData {
            date: "2025-01-15".into(),
            active_id: None,
            time_debt_secs: 0.0,
            timers: vec![],
        };
        let json = serde_json::to_string(&data).unwrap();
        let parsed: StoreData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.active_id, None);
        assert!(parsed.timers.is_empty());
    }

    #[test]
    fn timer_data_preserves_negative_remaining() {
        let td = TimerData {
            id: 1,
            name: "overdue".into(),
            original_secs: 60.0,
            remaining_secs: -120.5,
            fib_alert_index: 5,
        };
        let json = serde_json::to_string(&td).unwrap();
        let parsed: TimerData = serde_json::from_str(&json).unwrap();
        assert!((parsed.remaining_secs - (-120.5)).abs() < 0.001);
        assert_eq!(parsed.fib_alert_index, 5);
    }

    #[test]
    fn store_path_contains_date() {
        let path = store_path();
        assert!(path.is_some());
        let path = path.unwrap();
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(filename.starts_with("timers-"));
        assert!(filename.ends_with(".json"));
    }
}
