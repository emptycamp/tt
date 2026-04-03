use std::time::Instant;

use crate::command::Command;
use crate::timer::TimerState;

use super::{AddTimerError, App, Mode, UndoAction, UndoEntry, UNDO_WINDOW_SECS};

impl App {
    pub(super) fn execute_command(&mut self, cmd: Command) {
        match cmd {
            Command::Pause => self.toggle_pause(),
            Command::Reset => self.reset_active(),
            Command::Update(secs) => self.update_active(secs),
            Command::Revert => self.try_revert(),
            Command::Remove => self.remove_active(),
            Command::NewTimer(secs, name) => {
                if let Err(AddTimerError::DuplicateName(name)) = self.add_timer(secs, name) {
                    self.mode = Mode::NamePrompt(secs);
                    self.name_prompt_buffer = name;
                    self.name_prompt_error = Some(
                        "Timer with this name already exists. Enter a different name.".to_string(),
                    );
                }
            }
            Command::NamePrompt(secs) => {
                self.mode = Mode::NamePrompt(secs);
                self.name_prompt_buffer.clear();
                self.name_prompt_error = None;
            }
            Command::Switch => {
                if self.timers.len() > 1 {
                    self.open_selector();
                }
            }
            Command::TimerPrompt(name) => {
                self.mode = Mode::TimePrompt(name);
                self.time_prompt_buffer.clear();
                self.name_prompt_error = None;
            }
            Command::Quit => {
                self.should_quit = true;
            }
            Command::Unknown(_) => {}
        }
    }

    fn toggle_pause(&mut self) {
        if let Some(timer) = self.active_timer_mut() {
            if timer.state == TimerState::Paused {
                timer.resume();
            } else {
                timer.pause();
            }
            self.save();
        }
    }

    fn reset_active(&mut self) {
        if let Some(timer) = self.active_timer_mut() {
            timer.reset();
            self.save();
        }
    }

    fn update_active(&mut self, secs: f64) {
        let snapshot = self
            .active_timer()
            .map(|t| (t.id, t.remaining_secs, t.original_secs));

        let Some((id, old_remaining, old_original)) = snapshot else {
            return;
        };

        self.undo_stack.push(UndoEntry {
            timestamp: Instant::now(),
            action: UndoAction::TimeChanged {
                id,
                old_remaining,
                old_original,
            },
        });

        if let Some(timer) = self.active_timer_mut() {
            timer.remaining_secs = secs;
            timer.original_secs = secs;
            if timer.state == TimerState::Expired {
                timer.state = TimerState::Running;
            }
            timer.fib_alert_index = 0;
            timer.last_tick = Some(Instant::now());
        }

        self.save();
    }

    fn try_revert(&mut self) {
        let now = Instant::now();
        self.undo_stack
            .retain(|entry| now.duration_since(entry.timestamp).as_secs() < UNDO_WINDOW_SECS);

        let Some(entry) = self.undo_stack.pop() else {
            return;
        };

        match entry.action {
            UndoAction::TimerRemoved(timer) => {
                let id = timer.id;
                self.timers.push(timer);
                self.switch_to(id);
            }
            UndoAction::TimeChanged {
                id,
                old_remaining,
                old_original,
            } => {
                if let Some(timer) = self.timers.iter_mut().find(|t| t.id == id) {
                    timer.remaining_secs = old_remaining;
                    timer.original_secs = old_original;
                    timer.fib_alert_index = 0;
                    if old_remaining > 0.0 {
                        timer.state = TimerState::Running;
                        timer.last_tick = Some(Instant::now());
                    }
                }
            }
        }

        self.save();
    }
}
