use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerState {
    Running,
    Paused,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timer {
    pub id: u32,
    pub name: String,
    pub original_secs: f64,
    pub remaining_secs: f64,
    pub state: TimerState,
    #[serde(skip)]
    pub last_tick: Option<Instant>,
    pub fib_alert_index: usize,
}

impl Timer {
    pub fn new(id: u32, name: String, duration_secs: f64) -> Self {
        Self {
            id,
            name,
            original_secs: duration_secs,
            remaining_secs: duration_secs,
            state: TimerState::Running,
            last_tick: Some(Instant::now()),
            fib_alert_index: 0,
        }
    }

    /// Advance the timer by elapsed time since last tick.
    /// Returns `true` if the timer just crossed zero (first expire).
    pub fn tick(&mut self) -> bool {
        if self.state == TimerState::Paused {
            return false;
        }

        let now = Instant::now();
        let Some(last) = self.last_tick else {
            self.last_tick = Some(now);
            return false;
        };

        let elapsed = now.duration_since(last).as_secs_f64();
        let was_positive = self.remaining_secs > 0.0;
        self.remaining_secs -= elapsed;
        self.last_tick = Some(now);

        let crossed_zero = was_positive && self.remaining_secs <= 0.0;
        if crossed_zero {
            self.state = TimerState::Expired;
        }
        crossed_zero
    }

    pub fn pause(&mut self) {
        if self.state == TimerState::Running || self.state == TimerState::Expired {
            self.state = TimerState::Paused;
            self.last_tick = None;
        }
    }

    pub fn resume(&mut self) {
        self.state = if self.remaining_secs <= 0.0 {
            TimerState::Expired
        } else {
            TimerState::Running
        };
        self.last_tick = Some(Instant::now());
    }

    pub fn reset(&mut self) {
        self.remaining_secs = self.original_secs;
        self.state = TimerState::Running;
        self.last_tick = Some(Instant::now());
        self.fib_alert_index = 0;
    }

    pub fn is_overdue(&self) -> bool {
        self.remaining_secs < 0.0
    }

    pub fn format_remaining(&self) -> String {
        duration::format_seconds(self.remaining_secs)
    }

    pub const fn state_icon(&self) -> &'static str {
        match self.state {
            TimerState::Paused => "⏸",
            TimerState::Running | TimerState::Expired => "▶",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn new_timer_is_running() {
        let t = Timer::new(1, "test".into(), 60.0);
        assert_eq!(t.state, TimerState::Running);
        assert_eq!(t.remaining_secs, 60.0);
        assert_eq!(t.original_secs, 60.0);
        assert!(t.last_tick.is_some());
    }

    #[test]
    fn pause_and_resume() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        t.pause();
        assert_eq!(t.state, TimerState::Paused);
        assert!(t.last_tick.is_none());
        t.resume();
        assert_eq!(t.state, TimerState::Running);
        assert!(t.last_tick.is_some());
    }

    #[test]
    fn pause_while_paused_is_noop() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        t.pause();
        t.pause();
        assert_eq!(t.state, TimerState::Paused);
    }

    #[test]
    fn tick_does_not_advance_when_paused() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        t.pause();
        thread::sleep(Duration::from_millis(20));
        assert!(!t.tick());
        assert_eq!(t.remaining_secs, 60.0);
    }

    #[test]
    fn tick_decrements() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        thread::sleep(Duration::from_millis(50));
        t.tick();
        assert!(t.remaining_secs < 60.0);
    }

    #[test]
    fn tick_crosses_zero() {
        let mut t = Timer::new(1, "test".into(), 0.01);
        thread::sleep(Duration::from_millis(20));
        assert!(t.tick());
        assert_eq!(t.state, TimerState::Expired);
        assert!(t.remaining_secs < 0.0);
    }

    #[test]
    fn expired_timer_keeps_ticking_negative() {
        let mut t = Timer::new(1, "test".into(), 0.01);
        thread::sleep(Duration::from_millis(20));
        t.tick();
        assert_eq!(t.state, TimerState::Expired);
        let before = t.remaining_secs;
        thread::sleep(Duration::from_millis(20));
        assert!(!t.tick());
        assert!(t.remaining_secs < before);
    }

    #[test]
    fn reset_restores() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        t.remaining_secs = 10.0;
        t.state = TimerState::Expired;
        t.reset();
        assert_eq!(t.remaining_secs, 60.0);
        assert_eq!(t.state, TimerState::Running);
        assert_eq!(t.fib_alert_index, 0);
    }

    #[test]
    fn format_negative() {
        let mut t = Timer::new(1, "test".into(), 0.0);
        t.remaining_secs = -90.0;
        assert_eq!(t.format_remaining(), "-00:01:30");
    }

    #[test]
    fn is_overdue() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        assert!(!t.is_overdue());
        t.remaining_secs = -1.0;
        assert!(t.is_overdue());
    }

    #[test]
    fn state_icons() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        assert_eq!(t.state_icon(), "▶");
        t.pause();
        assert_eq!(t.state_icon(), "⏸");
        t.state = TimerState::Expired;
        assert_eq!(t.state_icon(), "▶");
    }

    #[test]
    fn resume_overdue_timer_resumes_as_expired() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        t.remaining_secs = -10.0;
        t.state = TimerState::Paused;
        t.resume();
        assert_eq!(t.state, TimerState::Expired);
    }

    #[test]
    fn resume_positive_timer_resumes_as_running() {
        let mut t = Timer::new(1, "test".into(), 60.0);
        t.remaining_secs = 30.0;
        t.state = TimerState::Paused;
        t.resume();
        assert_eq!(t.state, TimerState::Running);
    }
}
