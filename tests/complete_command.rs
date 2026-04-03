use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use tt::app::App;
use tt::store;

struct TestStoreModeGuard;

impl TestStoreModeGuard {
    fn new() -> Self {
        store::set_test_mode(true);
        Self
    }
}

impl Drop for TestStoreModeGuard {
    fn drop(&mut self) {
        store::set_test_mode(false);
    }
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

#[test]
fn complete_command_respects_time_debt_rules() {
    let _guard = TestStoreModeGuard::new();

    store::clear();

    let mut positive = App::with_timer(900.0, "focus".to_string());
    positive.time_debt_secs = 0.0;
    positive.input_buffer = "c".to_string();
    positive.handle_event(&press(KeyCode::Enter));

    assert!(positive.timers.is_empty());
    assert_eq!(positive.active_id, None);
    assert_eq!(positive.time_debt_secs, -900.0);

    store::clear();

    let mut zero = App::with_timer(0.0, "done-now".to_string());
    zero.time_debt_secs = 50.0;
    zero.input_buffer = "done".to_string();
    zero.handle_event(&press(KeyCode::Enter));

    assert!(zero.timers.is_empty());
    assert_eq!(zero.active_id, None);
    assert_eq!(zero.time_debt_secs, 50.0);

    store::clear();
}
