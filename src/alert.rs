/// Cumulative Fibonacci minute thresholds for overdue alerts.
const FIB_THRESHOLDS: &[f64] = &[0.0, 1.0, 2.0, 4.0, 7.0, 12.0, 20.0, 33.0, 54.0, 88.0, 143.0];

fn side_effects_enabled() -> bool {
    // Unit tests are built with cfg(test), while integration tests run under
    // the Rust test harness and expose RUST_TEST_THREADS.
    !(cfg!(test) || std::env::var_os("RUST_TEST_THREADS").is_some())
}

/// Check if an alert should fire based on overdue seconds and the current
/// Fibonacci alert index. Returns `true` when the timer should advance to the
/// next alert stage.
pub fn should_alert(overdue_secs: f64, fib_index: usize) -> bool {
    if overdue_secs <= 0.0 {
        return false;
    }
    let overdue_mins = overdue_secs / 60.0;
    FIB_THRESHOLDS
        .get(fib_index)
        .is_some_and(|&threshold| overdue_mins >= threshold)
}

/// Play the Windows "tada" sound asynchronously.
#[cfg(windows)]
pub fn play_sound() {
    if !side_effects_enabled() {
        return;
    }

    use windows::core::w;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME};
    unsafe {
        let _ = PlaySoundW(
            w!("C:\\Windows\\Media\\tada.wav"),
            None,
            SND_FILENAME | SND_ASYNC,
        );
    }
}

#[cfg(not(windows))]
pub fn play_sound() {
    if !side_effects_enabled() {
        return;
    }

    // No-op on non-Windows platforms
}

/// Show a desktop notification when a timer expires.
pub fn show_toast(timer_name: &str) {
    if !side_effects_enabled() {
        return;
    }

    let _ = notify_rust::Notification::new()
        .summary("tt — Timer expired")
        .body(&format!("\"{timer_name}\" has finished!"))
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_alert_when_not_overdue() {
        assert!(!should_alert(0.0, 0));
        assert!(!should_alert(-1.0, 0));
    }

    #[test]
    fn first_alert_at_zero_threshold() {
        assert!(should_alert(0.1, 0));
    }

    #[test]
    fn alert_at_one_minute() {
        assert!(!should_alert(30.0, 1));
        assert!(should_alert(60.0, 1));
    }

    #[test]
    fn alert_at_two_minutes() {
        assert!(!should_alert(90.0, 2));
        assert!(should_alert(120.0, 2));
    }

    #[test]
    fn fibonacci_sequence_thresholds() {
        assert!(should_alert(4.0 * 60.0, 3));
        assert!(should_alert(7.0 * 60.0, 4));
        assert!(should_alert(12.0 * 60.0, 5));
        assert!(should_alert(54.0 * 60.0, 8));
    }

    #[test]
    fn no_alert_beyond_thresholds() {
        assert!(!should_alert(999.0 * 60.0, 11));
        assert!(!should_alert(999.0 * 60.0, 100));
    }

    #[test]
    fn alert_boundary_exact() {
        assert!(should_alert(20.0 * 60.0, 6));
        assert!(!should_alert(19.9 * 60.0, 6));
    }

    #[test]
    fn side_effects_are_disabled_in_unit_tests() {
        assert!(!side_effects_enabled());
    }
}
