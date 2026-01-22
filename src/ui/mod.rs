//! Terminal UI rendering for bambutop.
//!
//! This module provides the main layout and rendering logic for the TUI.
//! The UI is composed of several panels: header (status/WiFi), progress bar,
//! temperature gauges, AMS filament status, printer controls, and a help bar.

mod controls;
mod header;
mod help;
mod progress;
mod status;
mod temps;
mod toast;

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

/// Seconds before data is considered slightly stale (yellow warning)
const STALE_WARNING_SECS: u64 = 5;

/// Seconds before data is considered critically stale (red warning)
const STALE_CRITICAL_SECS: u64 = 30;

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
            Constraint::Min(1),               // Spacer (absorbs extra space)
            Constraint::Length(4),            // Controls row (right-aligned)
            Constraint::Length(1),            // Help bar
        ])
        .split(content_area);

    header::render(frame, app, &printer_state, chunks[0]);
    progress::render(frame, &printer_state, app.timezone_offset_secs(), chunks[1]);

    // Middle row: temps on left (flexible), AMS on right (fixed width)
    // AMS width: 35 inner content + 2 borders = 37
    let middle_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(37)])
        .split(chunks[2]);

    temps::render(frame, &printer_state, app.use_celsius, middle_row[0]);
    status::render_ams(frame, &printer_state, middle_row[1]);

    // Toast notifications: render at bottom of spacer area, right-aligned
    let toast_count = app.toasts.len();
    if toast_count > 0 {
        let spacer = chunks[3];
        let toast_height = toast::panel_height(toast_count).min(spacer.height);
        if toast_height > 0 {
            let toast_area = Rect::new(
                spacer.x,
                spacer.y + spacer.height - toast_height,
                spacer.width,
                toast_height,
            );
            let toasts: Vec<_> = app.toasts.iter().cloned().collect();
            toast::render(frame, &toasts, toast_area);
        }
    }

    // Controls row: empty left half, controls on right half
    let controls_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[4]);

    controls::render(
        frame,
        &printer_state,
        app.controls_locked,
        app.cancel_pending,
        app.pause_pending,
        controls_row[1],
    );

    render_help_bar(frame, app, chunks[5]);

    // Render help overlay on top if visible
    if app.show_help {
        help::render(frame, content_area);
    }
}

/// Application version from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn render_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    // Determine update text and color based on staleness
    let (last_update, update_color): (Cow<'static, str>, Color) = app
        .time_since_update()
        .map(|d| {
            let secs = d.as_secs();
            let color = if secs >= STALE_CRITICAL_SECS {
                Color::Red
            } else if secs >= STALE_WARNING_SECS {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            let prefix = if secs >= STALE_WARNING_SECS {
                "\u{26A0} "
            } else {
                ""
            };
            (
                Cow::Owned(format!("{}Updated {}s ago ", prefix, secs)),
                color,
            )
        })
        .unwrap_or((Cow::Borrowed("No data yet "), Color::DarkGray));

    // Left side: logo with version, quit hint, and temp toggle
    let temp_unit = if app.use_celsius { "°C" } else { "°F" };
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
        Span::raw(" Quit  "),
        Span::styled("u", Style::new().fg(Color::Yellow)),
        Span::raw(format!(" {}", temp_unit)),
    ]);

    // Right side: last update time with staleness indicator
    let right = Line::from(vec![Span::styled(
        last_update,
        Style::new().fg(update_color),
    )]);

    // Split area for left and right alignment
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(right.width() as u16)])
        .split(area);

    frame.render_widget(Paragraph::new(left), chunks[0]);
    frame.render_widget(Paragraph::new(right), chunks[1]);
}
