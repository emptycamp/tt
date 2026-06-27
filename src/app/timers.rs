use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::alert;
use crate::duration::format_seconds;
use crate::task_state::{self, OverlayEntry};
use crate::timer::{Timer, TimerState};

use super::{AddTimerError, App, Mode, UndoAction, UndoEntry};

/// Which lifecycle change to write back to the `ttask` tool.
enum TaskWriteBack {
    Complete,
    Delete,
}

impl App {
    pub(super) fn add_timer(
        &mut self,
        duration_secs: f64,
        name: String,
    ) -> Result<(), AddTimerError> {
        let active_id = self.active_id;
        // Task-backed timers are never auto-removed on expiry — only on explicit
        // complete/remove, or when the task leaves Active in the `ttask` tool.
        let should_remove_active = active_id
            .and_then(|id| self.timers.iter().find(|t| t.id == id))
            .is_some_and(|t| {
                (t.state == TimerState::Expired || t.is_overdue()) && !t.is_task_backed()
            });

        let excluded_id = if should_remove_active {
            active_id
        } else {
            None
        };
        if self.has_timer_with_name(&name, excluded_id) {
            return Err(AddTimerError::DuplicateName(name));
        }

        if let Some(id) = active_id {
            if should_remove_active {
                self.timers.retain(|t| t.id != id);
                self.active_id = None;
            } else if let Some(timer) = self.timers.iter_mut().find(|t| t.id == id) {
                timer.pause();
            }
        }

        let timer = Timer::new(self.next_id, name, duration_secs);
        let id = timer.id;
        self.timers.push(timer);
        self.active_id = Some(id);
        self.next_id += 1;
        self.save();
        Ok(())
    }

    fn has_timer_with_name(&self, name: &str, excluded_id: Option<u32>) -> bool {
        self.timers
            .iter()
            .any(|timer| timer.name == name && Some(timer.id) != excluded_id)
    }

    pub(super) fn switch_to(&mut self, id: u32) {
        if let Some(current_id) = self.active_id {
            // Task-backed timers stay put when switched away from, even if expired.
            let should_remove = self
                .timers
                .iter()
                .find(|t| t.id == current_id)
                .is_some_and(|t| {
                    (t.state == TimerState::Expired || t.is_overdue()) && !t.is_task_backed()
                });

            if should_remove && current_id != id {
                self.timers.retain(|t| t.id != current_id);
            } else if let Some(timer) = self.timers.iter_mut().find(|t| t.id == current_id) {
                timer.pause();
            }
        }

        if let Some(timer) = self.timers.iter_mut().find(|t| t.id == id) {
            timer.resume();
        }

        self.active_id = Some(id);
        self.save();
    }

    pub(super) fn remove_active(&mut self) {
        if let Some(task_id) = self.active_timer().and_then(|t| t.task_id) {
            // Reflect into `ttask` as a soft-delete. On a real failure, keep the
            // local timer so tt and ttask don't diverge. No tt-undo entry is
            // pushed for task-backed timers — recovery is via `ttask history`.
            if self.write_back(task_id, TaskWriteBack::Delete) {
                self.remove_active_impl(false);
            }
            return;
        }
        self.remove_active_impl(true);
    }

    pub(super) fn complete_active(&mut self) {
        if let Some(task_id) = self.active_timer().and_then(|t| t.task_id) {
            if !self.write_back(task_id, TaskWriteBack::Complete) {
                return; // keep the timer; the task is still active in `ttask`
            }
        }

        if let Some(remaining_secs) = self.active_timer().map(|t| t.remaining_secs) {
            if remaining_secs > 0.0 {
                self.time_debt_secs -= remaining_secs;
            }
        }

        self.remove_active_impl(false);
    }

    fn remove_active_impl(&mut self, with_undo: bool) {
        let Some(id) = self.active_id else { return };

        if with_undo {
            if let Some(timer) = self.timers.iter().find(|t| t.id == id).cloned() {
                self.undo_stack.push(UndoEntry {
                    timestamp: Instant::now(),
                    action: UndoAction::TimerRemoved(timer),
                });
            }
        }

        self.timers.retain(|t| t.id != id);

        if self.timers.is_empty() {
            self.active_id = None;
        } else if self.timers.len() == 1 {
            self.switch_to(self.timers[0].id);
        } else {
            self.active_id = None;
            self.open_selector();
            return;
        }

        self.save();
    }

    pub(super) fn open_selector(&mut self) {
        self.mode = Mode::Selector;
        self.selector_filter.clear();
        self.selector_index = 0;
    }

    /// Tick all running timers, handle alerts, and accumulate overdue time debt.
    pub fn tick(&mut self) {
        for timer in &mut self.timers {
            let was_overdue = timer.remaining_secs;
            let crossed_zero = timer.tick();

            if crossed_zero {
                alert::play_sound();
                alert::show_toast(&timer.name);
                timer.fib_alert_index = 1;
            } else if timer.is_overdue() && timer.state != TimerState::Paused {
                let overdue = -timer.remaining_secs;
                if alert::should_alert(overdue, timer.fib_alert_index) {
                    alert::play_sound();
                    timer.fib_alert_index += 1;
                }
            }

            if timer.is_overdue() && timer.state != TimerState::Paused {
                let previous = if was_overdue < 0.0 { -was_overdue } else { 0.0 };
                let current = -timer.remaining_secs;
                if current > previous {
                    self.time_debt_secs += current - previous;
                }
            }
        }
    }

    pub(super) fn update_time_debt(&mut self) {
        let no_active_timer = self.active_timer().is_none();
        let all_paused =
            !self.timers.is_empty() && self.timers.iter().all(|t| t.state == TimerState::Paused);
        let should_track_idle_debt = no_active_timer || all_paused;

        if should_track_idle_debt {
            if let Some(since) = self.all_paused_since {
                let now = Instant::now();
                self.time_debt_secs += now.duration_since(since).as_secs_f64();
                self.all_paused_since = Some(now);
            } else {
                self.all_paused_since = Some(Instant::now());
            }
        } else if let Some(since) = self.all_paused_since.take() {
            self.time_debt_secs += Instant::now().duration_since(since).as_secs_f64();
        }
    }

    pub fn format_time_debt(&self) -> String {
        let extra = self.all_paused_since.map_or(0.0, |since| {
            Instant::now().duration_since(since).as_secs_f64()
        });
        format_seconds(self.time_debt_secs + extra)
    }

    /// Reconcile timers against the `ttask` tool's active task list: add a paused
    /// timer for each Active task lacking one, drop task-backed timers whose task
    /// is gone, and (only on the startup sync) restore the one timer that was
    /// running before. Ad-hoc timers are never touched. No-op when integration is
    /// off or `ttask` is unreachable, so existing behavior is unchanged.
    ///
    /// `initial` is true only for the startup sync — the one allowed to promote a
    /// restored running timer to the active timer.
    pub fn sync_with_tasks(&mut self, initial: bool) {
        if !self.integrate {
            return;
        }
        // `None` ⇒ the `ttask` tool is unreachable; do nothing so we never mistake
        // "can't read" for "no tasks" and wrongly remove timers.
        let Some(rows) = self.task_source.as_ref().and_then(|s| s.list_active()) else {
            return;
        };
        let active_ids: HashSet<u32> = rows.iter().map(|r| r.id).collect();
        let mut changed = false;

        // (a) Drop task-backed timers whose task is no longer active. Skip while
        // the selector is open so the list doesn't shift under the cursor.
        if !matches!(self.mode, Mode::Selector) {
            let active_removed = self
                .active_timer()
                .and_then(|t| t.task_id)
                .is_some_and(|tid| !active_ids.contains(&tid));
            let before = self.timers.len();
            self.timers
                .retain(|t| t.task_id.is_none_or(|tid| active_ids.contains(&tid)));
            if self.timers.len() != before {
                changed = true;
            }
            if active_removed {
                self.active_id = None;
            }
        }

        // (b) Refresh display names for existing task-backed timers whose text
        // changed (countdown left intact).
        for row in &rows {
            let desired = self.unique_task_name(&row.text, row.id);
            if let Some(t) = self.timers.iter_mut().find(|t| t.task_id == Some(row.id)) {
                if t.name != desired {
                    t.name = desired;
                    changed = true;
                }
            }
        }

        // (c) Add a paused timer for each active task that doesn't have one yet.
        let existing: HashSet<u32> = self.timers.iter().filter_map(|t| t.task_id).collect();
        let mut restored_running: Option<u32> = None;
        for row in &rows {
            if existing.contains(&row.id) {
                continue;
            }
            let original = row.est_secs.max(0) as f64;
            let entry = self.task_overlay.get(&row.id).cloned();
            let name = self.unique_task_name(&row.text, row.id);
            let mut timer = Timer::new(self.next_id, name, original);
            timer.task_id = Some(row.id);
            let was_running = apply_overlay(&mut timer, entry.as_ref());
            // Task-backed timers always come in paused. On the startup sync, at
            // most ONE timer that was running before is revived as active; every
            // other timer stays paused (only one timer runs at a time).
            if initial && was_running && restored_running.is_none() {
                restored_running = Some(timer.id);
            } else {
                timer.pause();
            }
            self.timers.push(timer);
            self.next_id += 1;
            changed = true;
        }

        if let Some(id) = restored_running {
            self.switch_to(id); // makes it active (pauses any prior active), saves
        } else if changed {
            self.save();
        }
    }

    /// Project current task-backed timers into the overlay and persist it.
    pub(super) fn save_task_overlay(&self) {
        let entries: HashMap<u32, OverlayEntry> = self
            .timers
            .iter()
            .filter_map(|t| {
                t.task_id.map(|tid| {
                    (
                        tid,
                        OverlayEntry {
                            task_id: tid,
                            remaining_secs: t.remaining_secs,
                            running: t.state != TimerState::Paused,
                            fib_alert_index: t.fib_alert_index,
                        },
                    )
                })
            })
            .collect();
        task_state::save(&entries);
    }

    /// A stable, unique display name for the task-backed timer of `task_id`. tt
    /// requires unique timer names but task texts may collide; fall back to a
    /// `#<id>` suffix. Matching is always by `task_id`, never by name.
    fn unique_task_name(&self, text: &str, task_id: u32) -> String {
        let base = {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                format!("task #{task_id}")
            } else {
                trimmed.to_string()
            }
        };
        let collides = |name: &str| {
            self.timers
                .iter()
                .any(|t| t.name == name && t.task_id != Some(task_id))
        };
        if !collides(&base) {
            return base;
        }
        let suffixed = format!("{base} #{task_id}");
        if !collides(&suffixed) {
            return suffixed;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base} #{task_id} ({n})");
            if !collides(&candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Write a completion/deletion back to the `ttask` tool. Returns true if the
    /// local timer should be removed: either the write-back succeeded, or the task
    /// is already gone/terminal. Returns false to keep the timer (a real failure
    /// on a still-active task) so tt and ttask never diverge.
    fn write_back(&mut self, task_id: u32, op: TaskWriteBack) -> bool {
        let Some(source) = self.task_source.as_mut() else {
            return true; // integration unavailable: just drop locally
        };
        let result = match op {
            TaskWriteBack::Complete => source.complete(task_id),
            TaskWriteBack::Delete => source.delete(task_id),
        };
        match result {
            Ok(()) => true,
            Err(_) => match source.list_active() {
                Some(rows) => !rows.iter().any(|r| r.id == task_id),
                None => false,
            },
        }
    }
}

/// Restore a freshly-built task-backed timer's countdown from its overlay entry.
/// Returns whether the overlay says it was running. Without an entry the timer
/// keeps its fresh `original` value (the caller then pauses it).
fn apply_overlay(timer: &mut Timer, entry: Option<&OverlayEntry>) -> bool {
    let Some(e) = entry else {
        return false;
    };
    timer.remaining_secs = e.remaining_secs;
    timer.fib_alert_index = e.fib_alert_index;
    if e.running {
        // Resume from the stored remaining, mirroring how store::load resumes the
        // active ad-hoc timer across restarts (frozen while tt was closed).
        timer.state = if timer.remaining_secs <= 0.0 {
            TimerState::Expired
        } else {
            TimerState::Running
        };
        timer.last_tick = Some(Instant::now());
        true
    } else {
        timer.state = TimerState::Paused;
        timer.last_tick = None;
        false
    }
}
