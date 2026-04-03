use std::time::Instant;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::{App, Mode};
use crate::timer::Timer;

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn ctrl_c() -> Event {
    Event::Key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn release(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    })
}

fn make_app() -> App {
    App {
        timers: vec![],
        active_id: None,
        mode: Mode::Normal,
        input_buffer: String::new(),
        selector_filter: String::new(),
        selector_index: 0,
        time_prompt_buffer: String::new(),
        name_prompt_buffer: String::new(),
        name_prompt_error: None,
        time_prompt_error: None,
        should_quit: false,
        time_debt_secs: 0.0,
        time_debt_label: "Test debt",
        next_id: 1,
        undo_stack: Vec::new(),
        all_paused_since: None,
        last_save: Instant::now(),
    }
}

fn make_app_with_timer(name: &str, secs: f64) -> App {
    let mut app = make_app();
    let timer = Timer::new(app.next_id, name.to_string(), secs);
    let id = timer.id;
    app.timers.push(timer);
    app.active_id = Some(id);
    app.next_id += 1;
    app
}

#[test]
fn empty_app_has_no_timers() {
    let app = make_app();
    assert!(app.timers.is_empty());
    assert_eq!(app.active_id, None);
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn app_with_timer_has_active_timer() {
    let app = make_app_with_timer("test", 60.0);
    assert_eq!(app.timers.len(), 1);
    assert!(app.active_id.is_some());
    let t = app.active_timer().expect("expected active timer");
    assert_eq!(t.name, "test");
    assert_eq!(t.original_secs, 60.0);
}

#[test]
fn typing_appends_to_input_buffer() {
    let mut app = make_app();
    app.handle_event(&press(KeyCode::Char('h')));
    app.handle_event(&press(KeyCode::Char('i')));
    assert_eq!(app.input_buffer, "hi");
}

#[test]
fn release_events_are_ignored() {
    let mut app = make_app();
    app.handle_event(&release(KeyCode::Char('h')));
    assert_eq!(app.input_buffer, "");
}

#[test]
fn backspace_removes_last_char() {
    let mut app = make_app();
    app.handle_event(&press(KeyCode::Char('a')));
    app.handle_event(&press(KeyCode::Char('b')));
    app.handle_event(&press(KeyCode::Backspace));
    assert_eq!(app.input_buffer, "a");
}

#[test]
fn esc_clears_input_buffer() {
    let mut app = make_app();
    app.input_buffer = "some text".into();
    app.handle_event(&press(KeyCode::Esc));
    assert_eq!(app.input_buffer, "");
}

#[test]
fn ctrl_c_sets_quit() {
    let mut app = make_app();
    app.handle_event(&ctrl_c());
    assert!(app.should_quit);
}

#[test]
fn quit_command_sets_should_quit() {
    let mut app = make_app();
    app.input_buffer = "q".into();
    app.handle_event(&press(KeyCode::Enter));
    assert!(app.should_quit);
}

#[test]
fn pause_command_toggles_timer_state() {
    let mut app = make_app_with_timer("test", 60.0);
    assert_eq!(
        app.active_timer().unwrap().state,
        crate::timer::TimerState::Running
    );

    app.input_buffer = "p".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(
        app.active_timer().unwrap().state,
        crate::timer::TimerState::Paused
    );

    app.input_buffer = "p".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(
        app.active_timer().unwrap().state,
        crate::timer::TimerState::Running
    );
}

#[test]
fn reset_command_restores_original_time() {
    let mut app = make_app_with_timer("test", 300.0);
    if let Some(timer) = app.active_timer_mut() {
        timer.remaining_secs = 100.0;
    }

    app.input_buffer = "reset".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.active_timer().unwrap().remaining_secs, 300.0);
}

#[test]
fn update_command_changes_time() {
    let mut app = make_app_with_timer("test", 300.0);

    app.input_buffer = "update 10m".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.active_timer().unwrap().remaining_secs, 600.0);
    assert_eq!(app.active_timer().unwrap().original_secs, 600.0);
}

#[test]
fn remove_command_removes_timer() {
    let mut app = make_app_with_timer("test", 60.0);
    assert_eq!(app.timers.len(), 1);

    app.input_buffer = "rm".into();
    app.handle_event(&press(KeyCode::Enter));
    assert!(app.timers.is_empty());
    assert_eq!(app.active_id, None);
}

#[test]
fn unknown_command_no_effect() {
    let mut app = make_app_with_timer("test", 60.0);
    let before = app.active_timer().unwrap().remaining_secs;

    app.input_buffer = "gibberish".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.active_timer().unwrap().remaining_secs, before);
}

#[test]
fn tt_new_timer_command() {
    let mut app = make_app();

    app.input_buffer = "tt 5m standup".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.active_timer().unwrap().name, "standup");
    assert_eq!(app.active_timer().unwrap().original_secs, 300.0);
}

#[test]
fn tt_duration_only_enters_name_prompt() {
    let mut app = make_app();

    app.input_buffer = "tt 5m".into();
    app.handle_event(&press(KeyCode::Enter));
    assert!(matches!(app.mode, Mode::NamePrompt(secs) if secs == 300.0));
}

#[test]
fn tt_name_only_enters_time_prompt() {
    let mut app = make_app();

    app.input_buffer = "tt meeting".into();
    app.handle_event(&press(KeyCode::Enter));
    assert!(matches!(app.mode, Mode::TimePrompt(ref n) if n == "meeting"));
}

#[test]
fn revert_restores_removed_timer() {
    let mut app = make_app_with_timer("test", 60.0);
    let id = app.active_id.unwrap();

    app.input_buffer = "rm".into();
    app.handle_event(&press(KeyCode::Enter));
    assert!(app.timers.is_empty());

    app.input_buffer = "revert".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.timers[0].id, id);
}

#[test]
fn revert_restores_updated_time() {
    let mut app = make_app_with_timer("test", 300.0);

    app.input_buffer = "update 10m".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.active_timer().unwrap().remaining_secs, 600.0);

    app.input_buffer = "revert".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.active_timer().unwrap().remaining_secs, 300.0);
}

#[test]
fn adding_second_timer_pauses_first() {
    let mut app = make_app_with_timer("first", 60.0);
    let first_id = app.active_id.unwrap();

    app.input_buffer = "tt 5m second".into();
    app.handle_event(&press(KeyCode::Enter));

    assert_eq!(app.timers.len(), 2);
    let first = app.timers.iter().find(|t| t.id == first_id).unwrap();
    assert_eq!(first.state, crate::timer::TimerState::Paused);
    assert_ne!(app.active_id.unwrap(), first_id);
    assert_eq!(
        app.active_timer().unwrap().state,
        crate::timer::TimerState::Running
    );
}

#[test]
fn switch_opens_selector() {
    let mut app = make_app_with_timer("first", 60.0);
    app.input_buffer = "tt 5m second".into();
    app.handle_event(&press(KeyCode::Enter));

    app.input_buffer = "tt".into();
    app.handle_event(&press(KeyCode::Enter));
    assert!(matches!(app.mode, Mode::Selector));
}

#[test]
fn switch_noop_with_zero_or_one_timer() {
    let mut app = make_app_with_timer("only", 60.0);
    app.input_buffer = "tt".into();
    app.handle_event(&press(KeyCode::Enter));
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn selector_esc_returns_to_normal() {
    let mut app = make_app();
    app.mode = Mode::Selector;
    app.handle_event(&press(KeyCode::Esc));
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn selector_filter_typing() {
    let mut app = make_app();
    app.mode = Mode::Selector;
    app.handle_event(&press(KeyCode::Char('a')));
    app.handle_event(&press(KeyCode::Char('b')));
    assert_eq!(app.selector_filter, "ab");
}

#[test]
fn selector_filter_backspace() {
    let mut app = make_app();
    app.mode = Mode::Selector;
    app.selector_filter = "abc".into();
    app.handle_event(&press(KeyCode::Backspace));
    assert_eq!(app.selector_filter, "ab");
}

#[test]
fn time_prompt_valid_input_creates_timer() {
    let mut app = make_app();
    app.mode = Mode::TimePrompt("meeting".into());

    app.handle_event(&press(KeyCode::Char('5')));
    app.handle_event(&press(KeyCode::Char('m')));
    app.handle_event(&press(KeyCode::Enter));

    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.active_timer().unwrap().name, "meeting");
    assert_eq!(app.active_timer().unwrap().original_secs, 300.0);
}

#[test]
fn time_prompt_invalid_input_shows_error() {
    let mut app = make_app();
    app.mode = Mode::TimePrompt("meeting".into());

    app.handle_event(&press(KeyCode::Char('x')));
    app.handle_event(&press(KeyCode::Enter));

    assert!(matches!(app.mode, Mode::TimePrompt(_)));
    assert!(app.time_prompt_error.is_some());
}

#[test]
fn time_prompt_esc_cancels() {
    let mut app = make_app();
    app.mode = Mode::TimePrompt("meeting".into());

    app.handle_event(&press(KeyCode::Esc));
    assert!(matches!(app.mode, Mode::Normal));
    assert!(app.time_prompt_error.is_none());
}

#[test]
fn name_prompt_valid_input_creates_timer() {
    let mut app = make_app();
    app.mode = Mode::NamePrompt(300.0);

    for c in "standup".chars() {
        app.handle_event(&press(KeyCode::Char(c)));
    }
    app.handle_event(&press(KeyCode::Enter));

    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.active_timer().unwrap().name, "standup");
    assert_eq!(app.active_timer().unwrap().original_secs, 300.0);
}

#[test]
fn name_prompt_empty_doesnt_create() {
    let mut app = make_app();
    app.mode = Mode::NamePrompt(300.0);

    app.handle_event(&press(KeyCode::Enter));
    assert!(app.timers.is_empty());
}

#[test]
fn name_prompt_esc_cancels() {
    let mut app = make_app();
    app.mode = Mode::NamePrompt(300.0);
    app.name_prompt_buffer = "partial".into();

    app.handle_event(&press(KeyCode::Esc));
    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.name_prompt_buffer, "");
}

#[test]
fn tick_accumulates_overdue_into_time_debt() {
    let mut app = make_app_with_timer("test", 0.01);
    std::thread::sleep(std::time::Duration::from_millis(30));
    let debt_before = app.time_debt_secs;
    app.tick();
    assert!(app.active_timer().unwrap().is_overdue());
    assert!(app.time_debt_secs > debt_before);
}

#[test]
fn tick_paused_timer_no_debt() {
    let mut app = make_app_with_timer("test", 60.0);
    if let Some(timer) = app.active_timer_mut() {
        timer.pause();
    }
    let debt_before = app.time_debt_secs;
    app.tick();
    assert_eq!(app.time_debt_secs, debt_before);
}

#[test]
fn filtered_timers_empty_filter_returns_all() {
    let mut app = make_app_with_timer("alpha", 60.0);
    app.input_buffer = "tt 5m beta".into();
    app.handle_event(&press(KeyCode::Enter));

    app.selector_filter.clear();
    assert_eq!(app.filtered_timers().len(), 2);
}

#[test]
fn filtered_timers_filters_by_name() {
    let mut app = make_app_with_timer("alpha", 60.0);
    app.input_buffer = "tt 5m beta".into();
    app.handle_event(&press(KeyCode::Enter));

    app.selector_filter = "alp".into();
    let filtered = app.filtered_timers();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "alpha");
}

#[test]
fn format_time_debt_zero() {
    let app = make_app();
    assert_eq!(app.format_time_debt(), "00:00:00");
}

#[test]
fn format_time_debt_with_value() {
    let mut app = make_app();
    app.time_debt_secs = 90.0;
    assert_eq!(app.format_time_debt(), "00:01:30");
}

#[test]
fn add_timer_removes_active_expired_timer() {
    let mut app = make_app_with_timer("expired", 60.0);
    if let Some(timer) = app.active_timer_mut() {
        timer.remaining_secs = -5.0;
        timer.state = crate::timer::TimerState::Expired;
        timer.last_tick = None;
    }

    app.add_timer(30.0, "fresh".to_string())
        .expect("expected timer to be added");

    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.active_timer().unwrap().name, "fresh");
}

#[test]
fn tt_new_timer_replaces_active_expired_timer() {
    let mut app = make_app_with_timer("expired", 60.0);
    if let Some(timer) = app.active_timer_mut() {
        timer.remaining_secs = -1.0;
        timer.state = crate::timer::TimerState::Expired;
        timer.last_tick = None;
    }

    app.input_buffer = "tt 10m test".into();
    app.handle_event(&press(KeyCode::Enter));

    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.active_timer().unwrap().name, "test");
}

#[test]
fn tt_duplicate_name_opens_name_prompt_with_error() {
    let mut app = make_app_with_timer("test", 60.0);

    app.input_buffer = "tt 10m test".into();
    app.handle_event(&press(KeyCode::Enter));

    assert_eq!(app.timers.len(), 1);
    assert!(matches!(app.mode, Mode::NamePrompt(secs) if secs == 600.0));
    assert_eq!(app.name_prompt_buffer, "test");
    assert!(app.name_prompt_error.is_some());
}

#[test]
fn name_prompt_duplicate_keeps_prompt_open() {
    let mut app = make_app_with_timer("standup", 60.0);
    app.mode = Mode::NamePrompt(300.0);
    app.name_prompt_buffer = "standup".into();

    app.handle_event(&press(KeyCode::Enter));

    assert!(matches!(app.mode, Mode::NamePrompt(secs) if secs == 300.0));
    assert_eq!(app.timers.len(), 1);
    assert!(app.name_prompt_error.is_some());
}

#[test]
fn no_active_timer_accumulates_time_debt() {
    let mut app = make_app();

    app.update_time_debt();
    std::thread::sleep(std::time::Duration::from_millis(20));
    app.update_time_debt();

    assert!(app.time_debt_secs > 0.0);
}
