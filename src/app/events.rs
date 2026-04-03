use crossterm::event::{Event, KeyCode, KeyModifiers};

use crate::command::Command;
use crate::duration::parse_duration;

use super::{key_press, AddTimerError, App, Mode};

impl App {
    pub fn handle_event(&mut self, ev: &Event) {
        match &self.mode {
            Mode::Normal => self.handle_normal_event(ev),
            Mode::Selector => self.handle_selector_event(ev),
            Mode::TimePrompt(_) => self.handle_time_prompt_event(ev),
            Mode::NamePrompt(_) => self.handle_name_prompt_event(ev),
        }
    }

    fn handle_normal_event(&mut self, ev: &Event) {
        let Some(key) = key_press(ev) else { return };

        match key.code {
            KeyCode::Enter => {
                let input = std::mem::take(&mut self.input_buffer);
                let cmd = Command::parse(&input);
                self.execute_command(cmd);
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Esc => {
                self.input_buffer.clear();
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }
    }

    fn handle_selector_event(&mut self, ev: &Event) {
        let Some(key) = key_press(ev) else { return };

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let selected_id = self.filtered_timers().get(self.selector_index).map(|t| t.id);
                if let Some(id) = selected_id {
                    self.mode = Mode::Normal;
                    self.switch_to(id);
                }
            }
            KeyCode::Up => {
                self.selector_index = self.selector_index.saturating_sub(1);
            }
            KeyCode::Down => {
                let max_index = self.filtered_timers().len().saturating_sub(1);
                if self.selector_index < max_index {
                    self.selector_index += 1;
                }
            }
            KeyCode::Backspace => {
                self.selector_filter.pop();
                self.selector_index = 0;
            }
            KeyCode::Char(c) => {
                self.selector_filter.push(c);
                self.selector_index = 0;
            }
            _ => {}
        }
    }

    fn handle_time_prompt_event(&mut self, ev: &Event) {
        let Some(key) = key_press(ev) else { return };

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.time_prompt_buffer.clear();
                self.time_prompt_error = None;
            }
            KeyCode::Enter => {
                let input = self.time_prompt_buffer.clone();
                match parse_duration(&input) {
                    Ok(secs) => {
                        self.time_prompt_buffer.clear();
                        self.time_prompt_error = None;
                        let name = match std::mem::replace(&mut self.mode, Mode::Normal) {
                            Mode::TimePrompt(name) => name,
                            _ => "Timer".to_string(),
                        };
                        if let Err(AddTimerError::DuplicateName(name)) = self.add_timer(secs, name) {
                            self.mode = Mode::NamePrompt(secs);
                            self.name_prompt_buffer = name;
                            self.name_prompt_error = Some(
                                "Timer with this name already exists. Enter a different name.".to_string(),
                            );
                        }
                    }
                    Err(e) => {
                        self.time_prompt_error = Some(format!("Invalid: {e}"));
                        self.time_prompt_buffer.clear();
                    }
                }
            }
            KeyCode::Backspace => {
                self.time_prompt_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.time_prompt_buffer.push(c);
            }
            _ => {}
        }
    }

    fn handle_name_prompt_event(&mut self, ev: &Event) {
        let Some(key) = key_press(ev) else { return };

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.name_prompt_buffer.clear();
                self.name_prompt_error = None;
            }
            KeyCode::Enter => {
                let secs = match &self.mode {
                    Mode::NamePrompt(secs) => *secs,
                    _ => 0.0,
                };
                let name = std::mem::take(&mut self.name_prompt_buffer);
                let trimmed_name = name.trim().to_string();

                if !trimmed_name.is_empty() {
                    if let Err(AddTimerError::DuplicateName(name)) = self.add_timer(secs, trimmed_name) {
                        self.name_prompt_buffer = name;
                        self.name_prompt_error = Some(
                            "Timer with this name already exists. Enter a different name.".to_string(),
                        );
                        return;
                    }
                }

                self.mode = Mode::Normal;
                self.name_prompt_error = None;
            }
            KeyCode::Backspace => {
                self.name_prompt_buffer.pop();
                self.name_prompt_error = None;
            }
            KeyCode::Char(c) => {
                self.name_prompt_buffer.push(c);
                self.name_prompt_error = None;
            }
            _ => {}
        }
    }
}
