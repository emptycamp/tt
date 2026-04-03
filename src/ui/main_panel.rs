use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::timer::{Timer, TimerState};

pub(super) fn draw_time_debt(frame: &mut Frame, app: &App, area: Rect) {
    let no_active_timer = app.active_timer().is_none();
    let all_paused =
        !app.timers.is_empty() && app.timers.iter().all(|t| t.state == TimerState::Paused);
    let any_overdue = app
        .timers
        .iter()
        .any(|t| t.is_overdue() && t.state != TimerState::Paused);
    let debt_active = no_active_timer || all_paused || any_overdue;

    let label = format!(" {} ", app.format_time_debt());
    let color = if debt_active {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", app.time_debt_label))
        .style(Style::default().fg(color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let para = Paragraph::new(Line::from(Span::styled(&label, Style::default().fg(color))))
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(para, inner);
}

pub(super) fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(timer) = app.active_timer() {
        draw_active_timer(frame, app, timer, area);
    } else {
        draw_empty_state(frame, area);
    }
}

fn draw_active_timer(
    frame: &mut Frame,
    app: &App,
    timer: &Timer,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} {} ", timer.state_icon(), timer.name));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let remaining = timer.format_remaining();
    let color = if timer.is_overdue() {
        Color::Red
    } else if timer.remaining_secs < 60.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    let time_paragraph = Paragraph::new(Line::from(Span::styled(&remaining, style)))
        .alignment(ratatui::layout::Alignment::Center);

    let v_center = if inner.height > 1 {
        inner.height / 2
    } else {
        0
    };
    let time_area = Rect {
        x: inner.x,
        y: inner.y + v_center,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(time_paragraph, time_area);

    if app.timers.len() > 1 {
        let active_position = app
            .timers
            .iter()
            .position(|t| Some(t.id) == app.active_id)
            .map_or(0, |i| i + 1);

        let indicator = format!(" {active_position}/{} timers ", app.timers.len());
        let indicator_para = Paragraph::new(Line::from(Span::styled(
            indicator,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(ratatui::layout::Alignment::Center);

        if inner.height > 2 {
            let ind_area = Rect {
                x: inner.x,
                y: inner.y + v_center + 1,
                width: inner.width,
                height: 1,
            };
            frame.render_widget(indicator_para, ind_area);
        }
    }
}

fn draw_empty_state(frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" tt ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = Paragraph::new(Line::from(Span::styled(
        "00:00:00",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(ratatui::layout::Alignment::Center);

    let v_center = if inner.height > 1 {
        inner.height / 2
    } else {
        0
    };
    let text_area = Rect {
        x: inner.x,
        y: inner.y + v_center,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(text, text_area);
}

pub(super) fn draw_command_bar(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.is_test_mode {
        " : TEST MODE "
    } else {
        " : "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(if app.is_test_mode {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cursor_display = format!("{}▏", app.input_buffer);
    let para = Paragraph::new(Line::from(Span::raw(&cursor_display)));
    frame.render_widget(para, inner);
}
