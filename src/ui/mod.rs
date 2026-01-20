//! Terminal UI rendering for bambutop.
//!
//! This module provides the main layout and rendering logic for the TUI.
//! The UI is composed of several panels: header (status/WiFi), progress bar,
//! temperature gauges, AMS filament status, printer controls, and a help bar.

mod controls;
mod header;
mod progress;
mod status;
mod temps;

use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::borrow::Cow;

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

    // Calculate temps panel height based on chamber sensor and active tray
    let has_chamber = printer_state.has_chamber_temp_sensor();
    let has_active_tray = printer_state.active_filament_type().is_some();
    let temps_height = temps::panel_height(has_chamber, has_active_tray);

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

/// Application version from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn render_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let last_update: Cow<'static, str> = app
        .time_since_update()
        .map(|d| Cow::Owned(format!("Updated {}s ago ", d.as_secs())))
        .unwrap_or(Cow::Borrowed("No data yet "));

    // Left side: logo with version and quit hint
    let left = Line::from(vec![
        Span::styled(
            format!(" BAMBUTOP v{} ", VERSION),
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("q", Style::new().fg(Color::Yellow)),
        Span::raw(" Quit"),
    ]);

    // Right side: last update time
    let right = Line::from(vec![Span::styled(
        last_update,
        Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )]);

    // Split area for left and right alignment
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(right.width() as u16)])
        .split(area);

    frame.render_widget(Paragraph::new(left), chunks[0]);
    frame.render_widget(Paragraph::new(right), chunks[1]);
}
