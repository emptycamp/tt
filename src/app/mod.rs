use std::collections::HashMap;
use std::time::Instant;

use crossterm::event::{Event, KeyEvent, KeyEventKind};

use crate::config;
use crate::store;
use crate::task_integration::{CliTaskSource, TaskSource};
use crate::task_state::{self, OverlayEntry};
use crate::timer::{Timer, TimerState};

mod commands;
mod events;
mod runtime;
mod timers;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Selector,
    TimePrompt(String),
    NamePrompt(f64),
}

#[derive(Debug, Clone)]
enum UndoAction {
    TimerRemoved(Timer),
    TimeChanged {
        id: u32,
        old_remaining: f64,
        old_original: f64,
    },
}

#[derive(Debug, Clone)]
struct UndoEntry {
    timestamp: Instant,
    action: UndoAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AddTimerError {
    DuplicateName(String),
}

pub struct App {
    pub timers: Vec<Timer>,
    pub active_id: Option<u32>,
    pub is_test_mode: bool,
    pub mode: Mode,
    pub input_buffer: String,
    pub selector_filter: String,
    pub selector_index: usize,
    pub time_prompt_buffer: String,
    pub name_prompt_buffer: String,
    pub name_prompt_error: Option<String>,
    pub time_prompt_error: Option<String>,
    pub should_quit: bool,
    pub time_debt_secs: f64,
    pub time_debt_label: &'static str,
    next_id: u32,
    undo_stack: Vec<UndoEntry>,
    all_paused_since: Option<Instant>,
    last_save: Instant,
    /// Whether `ttask` integration is enabled (snapshot of config at startup).
    pub(super) integrate: bool,
    /// Bridge to the external `ttask` tool; `None` when integration is off.
    pub(super) task_source: Option<Box<dyn TaskSource>>,
    /// Persisted task-timer countdowns loaded at startup, keyed by task id. Used
    /// to resume a task-backed timer where it left off when it first appears.
    pub(super) task_overlay: HashMap<u32, OverlayEntry>,
    last_sync: Instant,
}

const UNDO_WINDOW_SECS: u64 = 20;
const AUTO_SAVE_INTERVAL_SECS: u64 = 5;
const EVENT_POLL_INTERVAL_MS: u64 = 100;
/// How often to reconcile timers against the `ttask` tool's active task list.
const TASK_SYNC_INTERVAL_SECS: u64 = 3;

const TIME_DEBT_LABELS: &[&str] = &[
    "Time laundering",
    "Side quests",
    "Void feeding",
    "Time decay",
    "Clock bleed",
    "Future debt",
    "Existence lag",
    "Progress rot",
    "Brainrot",
    "Entropy meter",
];

fn random_debt_label() -> &'static str {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    Instant::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);

    let len = TIME_DEBT_LABELS.len() as u64;
    let idx = (hasher.finish() % len) as usize;
    TIME_DEBT_LABELS[idx]
}

fn key_press(ev: &Event) -> Option<KeyEvent> {
    match ev {
        Event::Key(key) if key.kind == KeyEventKind::Press => Some(*key),
        _ => None,
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let (timers, active_id, time_debt_secs) = store::load();
        let is_test_mode = store::is_test_mode();
        let next_id = timers.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let has_active_timer = active_id
            .and_then(|id| timers.iter().find(|t| t.id == id))
            .is_some();
        let all_timers_paused =
            !timers.is_empty() && timers.iter().all(|t| t.state == TimerState::Paused);
        let should_track_idle_debt = !has_active_timer || all_timers_paused;

        // Integration is opt-out via config and only active when the `ttask` tool
        // is actually reachable. When off/unavailable, everything below is inert
        // and tt behaves exactly as before.
        let integrate = config::integrate_with_task();
        let task_source: Option<Box<dyn TaskSource>> = if integrate {
            Some(Box::new(CliTaskSource::new(is_test_mode)))
        } else {
            None
        };
        let task_overlay = if integrate {
            task_state::load()
        } else {
            HashMap::new()
        };

        let mut app = Self {
            timers,
            active_id,
            is_test_mode,
            mode: Mode::Normal,
            input_buffer: String::new(),
            selector_filter: String::new(),
            selector_index: 0,
            time_prompt_buffer: String::new(),
            name_prompt_buffer: String::new(),
            name_prompt_error: None,
            time_prompt_error: None,
            should_quit: false,
            time_debt_secs,
            time_debt_label: random_debt_label(),
            next_id,
            undo_stack: Vec::new(),
            all_paused_since: if should_track_idle_debt {
                Some(Instant::now())
            } else {
                None
            },
            last_save: Instant::now(),
            integrate,
            task_source,
            task_overlay,
            last_sync: Instant::now(),
        };

        // Surface existing tasks as timers immediately (no-op when integration is
        // off or `ttask` is unavailable), restoring any countdown in progress.
        app.sync_with_tasks(true);
        app
    }

    /// Construct the app for a bare `tt` invocation: load existing state and, if
    /// nothing is currently active but timers exist, auto-start the topmost one
    /// so the user lands on a running timer instead of an idle screen.
    pub fn resume() -> Self {
        let mut app = Self::new();
        app.auto_start_topmost();
        app
    }

    /// If no timer is currently active but at least one exists, start the
    /// topmost one (the first in the list, matching the selector order).
    fn auto_start_topmost(&mut self) {
        if self.active_timer().is_some() {
            return;
        }
        if let Some(first) = self.timers.first() {
            let id = first.id;
            self.switch_to(id);
        }
    }

    pub fn with_timer(duration_secs: f64, name: String) -> Self {
        let mut app = Self::new();
        if let Err(AddTimerError::DuplicateName(name)) = app.add_timer(duration_secs, name) {
            app.mode = Mode::NamePrompt(duration_secs);
            app.name_prompt_buffer = name;
            app.name_prompt_error =
                Some("Timer with this name already exists. Enter a different name.".to_string());
        }
        app
    }

    pub fn with_duration_prompt(duration_secs: f64) -> Self {
        let mut app = Self::new();
        app.mode = Mode::NamePrompt(duration_secs);
        app
    }

    pub fn with_name_prompt(name: String) -> Self {
        let mut app = Self::new();
        app.mode = Mode::TimePrompt(name);
        app
    }

    pub fn active_timer(&self) -> Option<&Timer> {
        let id = self.active_id?;
        self.timers.iter().find(|t| t.id == id)
    }

    fn active_timer_mut(&mut self) -> Option<&mut Timer> {
        let id = self.active_id?;
        self.timers.iter_mut().find(|t| t.id == id)
    }

    /// Timers matching the current selector filter, in the order the switch
    /// picker shows them. `self.timers` is kept in creation order (ascending
    /// id) at every mutation point — new timers append, reverts re-insert in
    /// place — so this is creation-time ascending.
    pub fn filtered_timers(&self) -> Vec<&Timer> {
        let filter = self.selector_filter.to_lowercase();
        self.timers
            .iter()
            .filter(|t| filter.is_empty() || t.name.to_lowercase().contains(&filter))
            .collect()
    }

    fn save(&self) {
        // Ad-hoc timers go to the daily store exactly as before. When there are
        // no task-backed timers this is identical to the original `save`: `adhoc`
        // == all timers and `daily_active` == `active_id`.
        let adhoc: Vec<Timer> = self
            .timers
            .iter()
            .filter(|t| !t.is_task_backed())
            .cloned()
            .collect();
        let daily_active = self.active_id.filter(|id| {
            self.timers
                .iter()
                .any(|t| t.id == *id && !t.is_task_backed())
        });
        store::save(&adhoc, daily_active, self.time_debt_secs);

        // Task-backed timer countdowns go to the separate overlay (which removes
        // its file when empty). Only relevant when integration is on.
        if self.integrate {
            self.save_task_overlay();
        }
    }
}
