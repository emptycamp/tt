use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::app::{App, Mode};

mod main_panel;
mod popups;

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(frame.area());

    main_panel::draw_time_debt(frame, app, chunks[0]);
    main_panel::draw_main(frame, app, chunks[1]);
    main_panel::draw_command_bar(frame, app, chunks[2]);

    match &app.mode {
        Mode::Selector => popups::draw_selector(frame, app),
        Mode::TimePrompt(_) => popups::draw_time_prompt(frame, app),
        Mode::NamePrompt(_) => popups::draw_name_prompt(frame, app),
        Mode::Normal => {}
    }
}

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
