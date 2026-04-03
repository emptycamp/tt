use std::time::Instant;

use crate::alert;
use crate::duration::format_seconds;
use crate::timer::{Timer, TimerState};

use super::{AddTimerError, App, Mode, UndoAction, UndoEntry};

impl App {
    pub(super) fn add_timer(&mut self, duration_secs: f64, name: String) -> Result<(), AddTimerError> {
        let active_id = self.active_id;
        let should_remove_active = active_id
            .and_then(|id| self.timers.iter().find(|t| t.id == id))
            .is_some_and(|t| t.state == TimerState::Expired || t.is_overdue());

        let excluded_id = if should_remove_active { active_id } else { None };
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
            let should_remove = self
                .timers
                .iter()
                .find(|t| t.id == current_id)
                .is_some_and(|t| t.state == TimerState::Expired || t.is_overdue());

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
        self.remove_active_impl(true);
    }

    pub(super) fn complete_active(&mut self) {
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
}
