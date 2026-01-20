mod controls;
mod header;
mod progress;
mod status;
mod temps;

use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

/// Maximum content width for the UI (characters)
const MAX_CONTENT_WIDTH: u16 = 100;

/// Renders the main application UI with header, progress, temps, AMS, and help bar.
pub fn render(frame: &mut Frame, app: &App) {
    // Take a snapshot of printer state once to avoid holding the lock during rendering
    let printer_state = app.printer_state_snapshot();

    // Limit width and center horizontally
    let area = frame.area();
    let content_area = if area.width > MAX_CONTENT_WIDTH {
        let padding = (area.width - MAX_CONTENT_WIDTH) / 2;
        Rect::new(area.x + padding, area.y, MAX_CONTENT_WIDTH, area.height)
    } else {
        area
    };

    // Calculate temps panel height based on whether printer has chamber sensor
    let temps_height = temps::panel_height(printer_state.has_chamber_temp_sensor());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),            // Header (status + system info)
            Constraint::Length(7),            // Progress (job, spacer, info, bar, spacer)
            Constraint::Length(temps_height), // Temps + AMS row (dynamic height)
            Constraint::Length(4),            // Printer Controls
            Constraint::Min(1),               // Spacer (absorbs extra space)
            Constraint::Length(1),            // Help bar
        ])
        .split(content_area);

    header::render(frame, app, &printer_state, chunks[0]);
    progress::render(frame, &printer_state, chunks[1]);

    // Middle row: temps on left (flexible), AMS on right (fixed width)
    // AMS width: 35 inner content + 2 borders = 37
    let middle_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(37)])
        .split(chunks[2]);

    temps::render(frame, &printer_state, middle_row[0]);
    status::render_ams(frame, &printer_state, middle_row[1]);

    controls::render(frame, &printer_state, chunks[3]);

    render_help_bar(frame, app, chunks[5]);
}

fn render_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;
    use std::borrow::Cow;

    let refresh_status = if app.auto_refresh {
        Span::styled(" ON ", Style::new().fg(Color::Black).bg(Color::Green))
    } else {
        Span::styled(" OFF ", Style::new().fg(Color::Black).bg(Color::Red))
    };

    let last_update: Cow<'static, str> = app
        .time_since_update()
        .map(|d| Cow::Owned(format!("  Updated {}s ago", d.as_secs())))
        .unwrap_or(Cow::Borrowed("  No data yet"));

    let help = Line::from(vec![
        Span::styled(
            " BAMBUTOP ",
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("q", Style::new().fg(Color::Yellow)),
        Span::raw(" Quit  "),
        Span::styled("r", Style::new().fg(Color::Yellow)),
        Span::raw(" Auto-Refresh "),
        refresh_status,
        Span::styled(
            last_update,
            Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ]);

    frame.render_widget(Paragraph::new(help), area);
}
