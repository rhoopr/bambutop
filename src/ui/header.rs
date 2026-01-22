//! Header panel rendering with printer status and system info.
//!
//! Displays the printer model, connection status, HMS errors, and WiFi signal
//! strength with visual indicators and color coding.

use super::common::{
    extract_serial_suffix, format_compact_title, parse_dbm, WIFI_DEFAULT_DBM,
    WIFI_MEDIUM_THRESHOLD, WIFI_STRONG_THRESHOLD,
};
use crate::app::App;
use crate::printer::PrinterState;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use smallvec::SmallVec;
use std::time::Instant;

/// Renders the header panel with printer status and system info boxes.
pub fn render(frame: &mut Frame, app: &App, printer_state: &PrinterState, area: Rect) {
    // Split into two boxes side by side
    let boxes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(1)])
        .split(area);

    render_status_box(frame, app, printer_state, boxes[0]);
    render_system_box(frame, app, printer_state, boxes[1]);
}

fn render_status_box(frame: &mut Frame, app: &App, printer_state: &PrinterState, area: Rect) {
    let status = app.status_text();
    let status_color = match status {
        "Printing" => Color::Green,
        "Paused" => Color::Yellow,
        "Failed" | "Disconnected" => Color::Red,
        "Idle" => Color::Cyan,
        _ => Color::White,
    };

    // Format printer title: config name > "P1S ...0428" > "Bambu Printer"
    let title = if !printer_state.printer_name.is_empty() {
        // Use config name
        format!(" {} ", printer_state.printer_name)
    } else {
        // Use "P1S ...0428" format or fallback
        let model = if printer_state.printer_model.is_empty() {
            "Bambu Printer"
        } else {
            &printer_state.printer_model
        };
        let serial_suffix = extract_serial_suffix(&printer_state.serial_suffix);
        let compact_title = format_compact_title(model, serial_suffix);
        format!(" {} ", compact_title)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(status_color))
        .title(Span::styled(title, Style::new().fg(status_color)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let status_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(" {} ", status),
            Style::new()
                .fg(Color::Black)
                .bg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(status_line), inner);
}

fn render_system_box(frame: &mut Frame, app: &App, printer_state: &PrinterState, area: Rect) {
    let has_errors = !printer_state.hms_errors.is_empty() || app.error_message.is_some();

    let border_color = if has_errors { Color::Red } else { Color::Green };
    let title = if has_errors {
        " HMS Errors "
    } else {
        " HMS Status "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color))
        .title(Span::styled(title, Style::new().fg(border_color)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area: left for status, right for WiFi
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(22)])
        .split(inner);

    // Left side: status messages (pre-allocate for typical case)
    let mut lines: SmallVec<[Line; 4]> = SmallVec::new();

    if let Some(err) = &app.error_message {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(err.as_str(), Style::new().fg(Color::Red)),
        ]));
    } else if !printer_state.hms_errors.is_empty() {
        for error in &printer_state.hms_errors {
            let severity_color = match error.severity {
                0..=1 => Color::Yellow,
                2 => Color::LightRed,
                _ => Color::Red,
            };
            let relative_time = format_relative_time(error.received_at);
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(error.message.as_str(), Style::new().fg(severity_color)),
                Span::raw(" "),
                Span::styled(
                    format!("({})", relative_time),
                    Style::new().fg(Color::DarkGray),
                ),
            ]));
        }
    } else if !printer_state.hms_received {
        // No HMS data received yet - show placeholder
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("--", Style::new().fg(Color::DarkGray)),
        ]));
    } else {
        // HMS data received with no errors
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("All systems normal", Style::new().fg(Color::Green)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines.into_vec()), cols[0]);

    // Right side: WiFi indicator
    let wifi_spans = render_wifi_signal(&printer_state.wifi_signal);
    let wifi_line = Line::from(wifi_spans);
    frame.render_widget(
        Paragraph::new(wifi_line).alignment(Alignment::Right),
        cols[1],
    );
}

/// Renders WiFi signal with visual bars and color coding.
///
/// Signal strength thresholds:
/// - Strong: > -50dBm (green)
/// - Medium: -50 to -70dBm (yellow)
/// - Weak: < -70dBm (red)
///
/// Uses a lifetime parameter to borrow the wifi_signal string directly,
/// avoiding allocation on every render frame.
fn render_wifi_signal<'a>(wifi_signal: &'a str) -> Vec<Span<'a>> {
    /// Visual bars for strong WiFi signal
    const BARS_STRONG: &str = "\u{2582}\u{2584}\u{2586}\u{2588}";
    /// Visual bars for medium WiFi signal
    const BARS_MEDIUM: &str = "\u{2582}\u{2584}\u{2586} ";
    /// Visual bars for weak WiFi signal
    const BARS_WEAK: &str = "\u{2582}\u{2584}  ";

    if wifi_signal.is_empty() {
        return vec![
            Span::styled("WiFi: ", Style::new().fg(Color::DarkGray)),
            Span::styled("--", Style::new().fg(Color::DarkGray)),
            Span::raw(" "),
        ];
    }

    // Parse dBm value from string without allocation
    let dbm = parse_dbm(wifi_signal).unwrap_or(WIFI_DEFAULT_DBM);

    // Determine signal strength and color
    let (color, bars) = if dbm > WIFI_STRONG_THRESHOLD {
        (Color::Green, BARS_STRONG)
    } else if dbm > WIFI_MEDIUM_THRESHOLD {
        (Color::Yellow, BARS_MEDIUM)
    } else {
        (Color::Red, BARS_WEAK)
    };

    vec![
        Span::styled("WiFi: ", Style::new().fg(Color::DarkGray)),
        Span::styled(bars, Style::new().fg(color)),
        Span::raw(" "),
        Span::styled(wifi_signal, Style::new().fg(color)),
        Span::raw(" "),
    ]
}

/// Formats a relative time string from an Instant.
///
/// Returns human-readable strings like "2m ago", "1h ago", "3d ago".
/// For times under 60 seconds, returns "just now".
fn format_relative_time(instant: Instant) -> String {
    let elapsed = instant.elapsed();
    let secs = elapsed.as_secs();

    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
