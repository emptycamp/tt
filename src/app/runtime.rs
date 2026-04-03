use std::time::Instant;

use crossterm::event;
use crossterm::execute;
use crossterm::terminal::SetTitle;

use super::{App, AUTO_SAVE_INTERVAL_SECS, EVENT_POLL_INTERVAL_MS};

impl App {
    fn update_title(&self) {
        let mode_prefix = if self.is_test_mode { "[TEST] " } else { "" };
        let title = self.active_timer().map_or_else(
            || format!("{mode_prefix}tt"),
            |timer| {
                let remaining = timer.format_remaining();
                let icon = timer.state_icon();
                format!("{mode_prefix}{icon} {remaining} — {}", timer.name)
            },
        );
        let _ = execute!(std::io::stdout(), SetTitle(&title));
    }

    pub fn run(&mut self, terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>) {
        loop {
            self.tick();
            self.update_time_debt();
            self.update_title();

            let _ = terminal.draw(|frame| crate::ui::draw(frame, self));

            if self.last_save.elapsed().as_secs() >= AUTO_SAVE_INTERVAL_SECS {
                self.save();
                self.last_save = Instant::now();
            }

            if event::poll(std::time::Duration::from_millis(EVENT_POLL_INTERVAL_MS))
                .unwrap_or(false)
            {
                if let Ok(ev) = event::read() {
                    self.handle_event(&ev);
                }
            }

            if self.should_quit {
                self.save();
                break;
            }
        }
    }
}
