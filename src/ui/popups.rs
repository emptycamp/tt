use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Mode};

use super::centered_rect;

pub(super) fn draw_selector(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Switch Timer (↑↓ Enter Esc) ")
        .style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let filter_text = format!("Filter: {}▏", app.selector_filter);
    let filter_para = Paragraph::new(Line::from(Span::raw(&filter_text)));
    frame.render_widget(filter_para, chunks[0]);

    let filtered = app.filtered_timers();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, timer)| {
            let icon = timer.state_icon();
            let remaining = timer.format_remaining();
            let text = format!("{icon} {:<20} {remaining}", timer.name);
            let style = if i == app.selector_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if Some(timer.id) == app.active_id {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(text, style)))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[1]);
}

pub(super) fn draw_time_prompt(frame: &mut Frame, app: &App) {
    let has_error = app.time_prompt_error.is_some();
    let height_pct = if has_error { 35 } else { 20 };
    let area = centered_rect(50, height_pct, frame.area());
    frame.render_widget(Clear, area);

    let name = match &app.mode {
        Mode::TimePrompt(name) => name.as_str(),
        _ => "",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Time for \"{name}\" "))
        .style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let input = format!("Duration: {}▏", app.time_prompt_buffer);
    let para = Paragraph::new(Line::from(Span::raw(&input)));
    frame.render_widget(para, chunks[0]);

    if let Some(err) = &app.time_prompt_error {
        let err_line = Paragraph::new(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )));
        frame.render_widget(err_line, chunks[1]);

        let examples = Paragraph::new(vec![
            Line::from(Span::styled(
                "Examples:",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  5      → 5 minutes",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  30s    → 30 seconds",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  12.5m  → 12m 30s",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  1.5h   → 1h 30m",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        frame.render_widget(examples, chunks[2]);
    }
}

pub(super) fn draw_name_prompt(frame: &mut Frame, app: &App) {
    let has_error = app.name_prompt_error.is_some();
    let height_pct = if has_error { 30 } else { 20 };
    let area = centered_rect(50, height_pct, frame.area());
    frame.render_widget(Clear, area);

    let duration_str = match &app.mode {
        Mode::NamePrompt(secs) => crate::duration::format_seconds(*secs),
        _ => String::new(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Name for timer ({duration_str}) "))
        .style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let input = format!("Name: {}▏", app.name_prompt_buffer);
    let para = Paragraph::new(Line::from(Span::raw(&input)));
    frame.render_widget(para, chunks[0]);

    if let Some(err) = &app.name_prompt_error {
        let err_line = Paragraph::new(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )));
        frame.render_widget(err_line, chunks[1]);
    }
}
