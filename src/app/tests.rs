use std::time::Instant;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::{App, Mode};
use crate::task_integration::{FakeTaskSource, TaskRow, TaskSource};
use crate::timer::{Timer, TimerState};

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
    // Confine any `save()` these tests trigger to the isolated `tt-test` data dir,
    // so running the suite never writes to the user's real timer data.
    crate::store::set_test_mode(true);
    App {
        timers: vec![],
        active_id: None,
        is_test_mode: false,
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
        integrate: false,
        task_source: None,
        task_overlay: std::collections::HashMap::new(),
        last_sync: Instant::now(),
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
fn complete_command_removes_timer_and_reduces_time_debt_for_positive_remaining() {
    let mut app = make_app_with_timer("test", 900.0);
    app.time_debt_secs = 0.0;

    app.input_buffer = "complete".into();
    app.handle_event(&press(KeyCode::Enter));

    assert!(app.timers.is_empty());
    assert_eq!(app.active_id, None);
    assert_eq!(app.time_debt_secs, -900.0);
}

#[test]
fn complete_command_removes_timer_without_changing_debt_for_non_positive_remaining() {
    let mut app = make_app_with_timer("test", 60.0);
    app.time_debt_secs = 120.0;
    if let Some(timer) = app.active_timer_mut() {
        timer.remaining_secs = -15.0;
        timer.state = crate::timer::TimerState::Expired;
        timer.last_tick = None;
    }

    app.input_buffer = "done".into();
    app.handle_event(&press(KeyCode::Enter));

    assert!(app.timers.is_empty());
    assert_eq!(app.active_id, None);
    assert_eq!(app.time_debt_secs, 120.0);
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

fn add_paused_timer(app: &mut App, name: &str, secs: f64) -> u32 {
    let mut t = Timer::new(app.next_id, name.to_string(), secs);
    t.state = TimerState::Paused;
    t.last_tick = None;
    let id = t.id;
    app.timers.push(t);
    app.next_id += 1;
    id
}

#[test]
fn auto_start_topmost_starts_first_when_none_active() {
    let mut app = make_app();
    let first_id = add_paused_timer(&mut app, "first", 60.0);
    add_paused_timer(&mut app, "second", 60.0);
    app.active_id = None;

    app.auto_start_topmost();

    assert_eq!(app.active_id, Some(first_id));
    assert_eq!(app.timers[0].state, TimerState::Running);
    assert_eq!(app.timers[1].state, TimerState::Paused);
}

#[test]
fn auto_start_topmost_noop_when_active_exists() {
    let mut app = make_app_with_timer("active", 60.0);
    add_paused_timer(&mut app, "other", 60.0);
    let active_before = app.active_id;

    app.auto_start_topmost();

    // An already-active timer is left untouched; nothing else is started.
    assert_eq!(app.active_id, active_before);
    assert_eq!(app.timers[1].state, TimerState::Paused);
}

#[test]
fn auto_start_topmost_noop_when_no_timers() {
    let mut app = make_app();
    app.auto_start_topmost();
    assert_eq!(app.active_id, None);
    assert!(app.timers.is_empty());
}

#[test]
fn revert_keeps_picker_in_creation_order() {
    // The switch picker shows timers in creation order (ascending id). Reverting
    // a removed timer must restore it to its creation-order slot, not the end.
    let mut app = make_app();
    app.add_timer(60.0, "first".into()).unwrap(); // id 1
    app.add_timer(60.0, "second".into()).unwrap(); // id 2 (now active)

    let first_id = app.timers.iter().find(|t| t.name == "first").unwrap().id;
    app.switch_to(first_id);

    app.input_buffer = "rm".into();
    app.handle_event(&press(KeyCode::Enter));
    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.timers[0].name, "second");

    app.input_buffer = "revert".into();
    app.handle_event(&press(KeyCode::Enter));

    assert_eq!(app.timers.len(), 2);
    let ids: Vec<u32> = app.timers.iter().map(|t| t.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(
        ids, sorted,
        "picker must stay sorted by creation (id) ascending"
    );
    assert_eq!(app.timers[0].name, "first");
    assert_eq!(app.timers[1].name, "second");
}

#[test]
fn no_active_timer_accumulates_time_debt() {
    let mut app = make_app();

    app.update_time_debt();
    std::thread::sleep(std::time::Duration::from_millis(20));
    app.update_time_debt();

    assert!(app.time_debt_secs > 0.0);
}

// --- task integration ---------------------------------------------------------

fn task_row(id: u32, est_secs: i64, text: &str) -> TaskRow {
    TaskRow {
        id,
        est_secs,
        text: text.to_string(),
    }
}

/// An `App` with integration on, backed by the given fake. The fake is cloned so
/// the test keeps a handle to inspect writes. (make_app already set test mode, so
/// any save() goes to the isolated tt-test dir.)
fn make_integrated_app(fake: &FakeTaskSource) -> App {
    let mut app = make_app();
    app.integrate = true;
    app.task_source = Some(Box::new(fake.clone()) as Box<dyn TaskSource>);
    app
}

#[test]
fn sync_adds_paused_task_backed_timer_with_est_value() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 1800, "Write report")]);
    let mut app = make_integrated_app(&fake);

    app.sync_with_tasks(false);

    assert_eq!(app.timers.len(), 1);
    let t = &app.timers[0];
    assert_eq!(t.task_id, Some(1));
    assert_eq!(t.name, "Write report");
    assert_eq!(t.state, TimerState::Paused);
    assert_eq!(t.original_secs, 1800.0);
    assert_eq!(t.remaining_secs, 1800.0);
    assert_eq!(app.active_id, None); // adding task timers must not steal active
}

#[test]
fn initial_sync_starts_all_new_task_timers_paused() {
    // Regression: on first launch (no overlay), every task-backed timer must come
    // in paused — only one timer may run at a time, and nothing was running before.
    let fake = FakeTaskSource::with_rows(vec![
        task_row(1, 600, "a"),
        task_row(2, 600, "b"),
        task_row(3, 600, "c"),
    ]);
    let mut app = make_integrated_app(&fake);

    app.sync_with_tasks(true);

    assert_eq!(app.timers.len(), 3);
    assert!(
        app.timers.iter().all(|t| t.state == TimerState::Paused),
        "all task timers should be paused on first launch"
    );
    assert_eq!(app.active_id, None);
}

#[test]
fn integration_off_is_a_no_op() {
    // With integrate=false the source is never consulted and no timers appear.
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 600, "a")]);
    let mut app = make_app(); // integrate stays false
    app.task_source = Some(Box::new(fake.clone()) as Box<dyn TaskSource>);

    app.sync_with_tasks(true);

    assert!(app.timers.is_empty());
}

#[test]
fn sync_is_idempotent() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 600, "a"), task_row(2, 600, "b")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);
    app.sync_with_tasks(false);
    assert_eq!(app.timers.len(), 2);
}

#[test]
fn sync_removes_timer_when_task_no_longer_active() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 600, "a"), task_row(2, 600, "b")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);
    assert_eq!(app.timers.len(), 2);

    fake.set_rows(vec![task_row(2, 600, "b")]);
    app.sync_with_tasks(false);

    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.timers[0].task_id, Some(2));
}

#[test]
fn sync_leaves_ad_hoc_timers_untouched() {
    let fake = FakeTaskSource::with_rows(vec![]);
    let mut app = make_integrated_app(&fake);
    app.add_timer(60.0, "adhoc".to_string()).unwrap();

    app.sync_with_tasks(false);

    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.timers[0].task_id, None);
    assert_eq!(app.timers[0].name, "adhoc");
}

#[test]
fn unavailable_source_does_not_remove_task_timers() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 600, "keep")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);
    assert_eq!(app.timers.len(), 1);

    // `ttask` becomes unreachable (list_active -> None): must NOT be treated as
    // "no tasks" and wipe the timer.
    fake.set_available(false);
    app.sync_with_tasks(false);
    assert_eq!(app.timers.len(), 1);
}

#[test]
fn task_backed_timer_survives_expiry_and_switch() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 600, "one"), task_row(2, 600, "two")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);

    let id1 = app.timers.iter().find(|t| t.task_id == Some(1)).unwrap().id;
    let id2 = app.timers.iter().find(|t| t.task_id == Some(2)).unwrap().id;

    app.switch_to(id1);
    if let Some(t) = app.timers.iter_mut().find(|t| t.id == id1) {
        t.remaining_secs = -5.0;
        t.state = TimerState::Expired;
    }
    app.switch_to(id2);

    // The expired task-backed timer must NOT have been removed (unlike ad-hoc).
    assert!(app.timers.iter().any(|t| t.id == id1));
    assert_eq!(app.timers.len(), 2);
}

#[test]
fn duplicate_task_texts_get_unique_names() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 600, "dup"), task_row(2, 600, "dup")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);

    let mut names = std::collections::HashSet::new();
    for t in &app.timers {
        assert!(
            names.insert(t.name.clone()),
            "duplicate timer name: {}",
            t.name
        );
    }
    assert_eq!(app.timers.len(), 2);
}

#[test]
fn zero_estimate_makes_zero_length_paused_timer() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 0, "zero")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);

    assert_eq!(app.timers.len(), 1);
    assert_eq!(app.timers[0].state, TimerState::Paused);
    assert_eq!(app.timers[0].original_secs, 0.0);
}

#[test]
fn complete_writes_back_and_removes_task_timer() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 1800, "task one")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);
    let id = app.timers[0].id;
    app.switch_to(id);

    app.complete_active();

    assert!(app.timers.is_empty());
    assert_eq!(fake.completed(), vec![1]);
}

#[test]
fn remove_writes_back_soft_delete() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 1800, "task one")]);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);
    let id = app.timers[0].id;
    app.switch_to(id);

    app.remove_active();

    assert!(app.timers.is_empty());
    assert_eq!(fake.deleted(), vec![1]);
}

#[test]
fn write_back_failure_keeps_timer() {
    let fake = FakeTaskSource::with_rows(vec![task_row(1, 1800, "task one")]);
    fake.set_fail_writes(true);
    let mut app = make_integrated_app(&fake);
    app.sync_with_tasks(false);
    let id = app.timers[0].id;
    app.switch_to(id);

    app.complete_active();

    // Write failed and the task is still active → keep the timer so tt and ttask
    // don't diverge.
    assert_eq!(app.timers.len(), 1);
    assert!(fake.completed().is_empty());
}

#[test]
fn ad_hoc_complete_does_not_call_task() {
    let fake = FakeTaskSource::with_rows(vec![]);
    let mut app = make_integrated_app(&fake);
    app.add_timer(60.0, "adhoc".to_string()).unwrap();

    app.complete_active();

    assert!(app.timers.is_empty());
    assert!(fake.completed().is_empty());
}

#[test]
fn initial_sync_revives_one_running_timer_from_overlay() {
    use crate::task_state::OverlayEntry;

    let fake = FakeTaskSource::with_rows(vec![task_row(1, 600, "a"), task_row(2, 600, "b")]);
    let mut app = make_integrated_app(&fake);
    // Task 1 was paused; task 2 was running (with 120s left) before restart.
    app.task_overlay.insert(
        1,
        OverlayEntry {
            task_id: 1,
            remaining_secs: 300.0,
            running: false,
            fib_alert_index: 0,
        },
    );
    app.task_overlay.insert(
        2,
        OverlayEntry {
            task_id: 2,
            remaining_secs: 120.0,
            running: true,
            fib_alert_index: 0,
        },
    );

    app.sync_with_tasks(true);

    let running: Vec<_> = app
        .timers
        .iter()
        .filter(|t| t.state != TimerState::Paused)
        .collect();
    assert_eq!(running.len(), 1, "exactly one timer may run at a time");
    assert_eq!(running[0].task_id, Some(2));
    assert_eq!(running[0].remaining_secs, 120.0);
    assert_eq!(app.active_id, Some(running[0].id));
}
