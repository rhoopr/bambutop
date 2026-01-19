use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};
use std::borrow::Cow;

/// Speed level percentages for Bambu printers.
/// Levels: 1=silent, 2=standard, 3=sport, 4=ludicrous
const SPEED_SILENT: u32 = 50;
const SPEED_STANDARD: u32 = 100;
const SPEED_SPORT: u32 = 124;
const SPEED_LUDICROUS: u32 = 166;

/// Renders the print progress panel showing job name, speed, layer, time remaining, and progress bar.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(
            " Print Progress ",
            Style::new().fg(Color::Blue),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Job name
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Speed/Layer/Remaining
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // Spacer
        ])
        .split(inner);

    let print_status = &app.printer_state.print_status;

    // Print job name - use smart display_name() that prefers actual project names
    let job_name = print_status.display_name();
    let job_display: Cow<'_, str> = if job_name.is_empty() {
        Cow::Borrowed("No print job")
    } else {
        job_name
    };

    let file_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Job: ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            truncate_str(&job_display, 70).into_owned(),
            Style::new().fg(Color::White),
        ),
    ]);
    frame.render_widget(Paragraph::new(file_line), chunks[0]);

    // Speed, Layer and time remaining
    let speed_percent = speed_level_to_percent(app.printer_state.speeds.speed_level);
    let time_remaining = format_time(print_status.remaining_time_mins);

    let layer_value = if print_status.total_layers > 0 {
        format!("{}/{}", print_status.layer_num, print_status.total_layers)
    } else {
        "-/-".to_string()
    };

    let info_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Speed: ", Style::new().fg(Color::DarkGray)),
        Span::styled(format!("{}%", speed_percent), Style::new().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled("Layer: ", Style::new().fg(Color::DarkGray)),
        Span::styled(layer_value, Style::new().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled("Remaining: ", Style::new().fg(Color::DarkGray)),
        Span::styled(time_remaining.into_owned(), Style::new().fg(Color::Cyan)),
    ]);
    frame.render_widget(Paragraph::new(info_line), chunks[2]);

    // Progress bar
    let progress = print_status.progress as f64 / 100.0;
    let progress_color = if progress >= 1.0 {
        Color::Green
    } else if progress > 0.0 {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let gauge = Gauge::default()
        .gauge_style(Style::new().fg(progress_color).bg(Color::DarkGray))
        .ratio(progress)
        .label(Span::styled(
            format!("{}%", print_status.progress),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(gauge, chunks[3]);
}

/// Truncates a string to a maximum length, adding "..." if truncated.
/// Returns `Cow::Borrowed` when no truncation is needed to avoid allocation.
fn truncate_str(s: &str, max_len: usize) -> Cow<'_, str> {
    if s.len() <= max_len {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(format!("{}...", &s[..max_len - 3]))
    }
}

/// Formats minutes into a human-readable time string.
/// Returns `Cow::Borrowed` for the zero case to avoid allocation.
fn format_time(mins: u32) -> Cow<'static, str> {
    if mins == 0 {
        Cow::Borrowed("--:--")
    } else {
        let hours = mins / 60;
        let minutes = mins % 60;
        Cow::Owned(if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        })
    }
}

/// Converts Bambu speed level (1-4) to percentage.
fn speed_level_to_percent(level: u8) -> u32 {
    match level {
        1 => SPEED_SILENT,
        2 => SPEED_STANDARD,
        3 => SPEED_SPORT,
        4 => SPEED_LUDICROUS,
        _ => SPEED_STANDARD,
    }
}
