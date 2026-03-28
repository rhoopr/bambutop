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
use std::borrow::Cow;
use std::time::Instant;

/// Seconds per minute for time formatting
const SECS_PER_MINUTE: u64 = 60;
/// Seconds per hour for time formatting
const SECS_PER_HOUR: u64 = 3600;
/// Seconds per day for time formatting
const SECS_PER_DAY: u64 = 86_400;

/// HMS severity level considered a warning (yellow)
const HMS_SEVERITY_WARNING: u8 = 1;
/// HMS severity level considered a serious error (light red)
const HMS_SEVERITY_ERROR: u8 = 2;

/// Renders the header panel as a single unified box.
///
/// Title shows "Printer Name — Status". Content has HMS/errors on the left
/// and WiFi, monitoring indicators, and firmware on the right.
pub fn render(frame: &mut Frame, app: &App, printer_state: &PrinterState, area: Rect) {
    let status = app.status_text();
    let status_color = match status {
        "Printing" => Color::Green,
        "Paused" => Color::Yellow,
        "Failed" | "Disconnected" => Color::Red,
        "Idle" => Color::Cyan,
        _ => Color::White,
    };

    let border_color = status_color;

    // Build title: "Printer Name — Status"
    let printer_name = if !printer_state.printer_name.is_empty() {
        Cow::Borrowed(printer_state.printer_name.as_str())
    } else {
        let model = if printer_state.printer_model.is_empty() {
            "Bambu Printer"
        } else {
            &printer_state.printer_model
        };
        let serial_suffix = extract_serial_suffix(&printer_state.serial_suffix);
        format_compact_title(model, serial_suffix)
    };

    let title = Line::from(vec![
        Span::styled(format!(" {printer_name} "), Style::new().fg(border_color)),
        Span::styled(
            format!(" {status} "),
            Style::new()
                .fg(Color::Black)
                .bg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner: left for HMS/status, right for system info
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(28)])
        .split(inner);

    // Left side: failure reason, HMS errors, or status
    let mut lines: Vec<Line> = Vec::with_capacity(4);

    if let Some(failure) = printer_state.print_status.failure_description() {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(failure.into_owned(), Style::new().fg(Color::Red)),
        ]));
    } else if let Some(err) = app.active_error_message() {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(err, Style::new().fg(Color::Red)),
        ]));
    } else if !printer_state.hms_errors.is_empty() {
        for error in &printer_state.hms_errors {
            let severity_color = match error.severity {
                0..=HMS_SEVERITY_WARNING => Color::Yellow,
                HMS_SEVERITY_ERROR => Color::LightRed,
                _ => Color::Red,
            };
            let relative_time = format_relative_time(error.received_at);
            let error_code = format!(
                "{:04X}_{:04X}",
                (error.code >> 16) & 0xFFFF,
                error.code & 0xFFFF,
            );
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(error_code, Style::new().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(&*error.message, Style::new().fg(severity_color)),
                Span::raw(" "),
                Span::styled(
                    format!("({relative_time})"),
                    Style::new().fg(Color::DarkGray),
                ),
            ]));
        }
    } else if !printer_state.hms_received {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("--", Style::new().fg(Color::DarkGray)),
        ]));
    } else if printer_state.print_status.gcode_state == crate::printer::GcodeState::Failed {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("Print failed", Style::new().fg(Color::Red)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("All systems normal", Style::new().fg(Color::Green)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), cols[0]);

    // Right side: WiFi, monitoring indicators, firmware
    let mut info_lines: Vec<Line> = Vec::with_capacity(4);

    // Line 1: WiFi signal
    let wifi_spans = render_wifi_signal(&printer_state.wifi_signal);
    info_lines.push(Line::from(wifi_spans));

    // Line 2: Monitoring indicators (AI, FLI, REC, TL)
    let has_indicators = printer_state.has_xcam() || printer_state.has_ipcam();
    if has_indicators {
        let dot = |on: bool, halt: bool| -> Span<'static> {
            let color = if !on {
                Color::DarkGray
            } else if halt {
                Color::Yellow
            } else {
                Color::Green
            };
            Span::styled("●", Style::new().fg(color))
        };
        let label = Style::new().fg(Color::DarkGray);
        let halt = printer_state.xcam.print_halt;
        let mut ind_spans: Vec<Span> = Vec::with_capacity(12);
        if printer_state.has_xcam() {
            ind_spans.push(Span::styled("AI", label));
            ind_spans.push(dot(printer_state.xcam.spaghetti_detector, halt));
            ind_spans.push(Span::raw(" "));
            ind_spans.push(Span::styled("FLI", label));
            ind_spans.push(dot(printer_state.xcam.first_layer_inspector, halt));
            ind_spans.push(Span::raw(" "));
        }
        if printer_state.has_ipcam() {
            ind_spans.push(Span::styled("REC", label));
            ind_spans.push(dot(printer_state.ipcam.recording, false));
            ind_spans.push(Span::raw(" "));
            ind_spans.push(Span::styled("TL", label));
            ind_spans.push(dot(printer_state.ipcam.timelapse, false));
            ind_spans.push(Span::raw(" "));
        }
        info_lines.push(Line::from(ind_spans));
    }

    // Line 3: Firmware version
    if !printer_state.firmware_version.is_empty() {
        info_lines.push(Line::from(vec![
            Span::styled("FW: ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                printer_state.firmware_version.as_str(),
                Style::new().fg(Color::DarkGray),
            ),
            Span::raw(" "),
        ]));
    }

    frame.render_widget(
        Paragraph::new(info_lines).alignment(Alignment::Right),
        cols[1],
    );
}

/// Renders WiFi signal with visual bars and color coding.
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

    let dbm = parse_dbm(wifi_signal).unwrap_or(WIFI_DEFAULT_DBM);

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
fn format_relative_time(instant: Instant) -> Cow<'static, str> {
    let elapsed = instant.elapsed();
    let secs = elapsed.as_secs();

    if secs < SECS_PER_MINUTE {
        Cow::Borrowed("just now")
    } else if secs < SECS_PER_HOUR {
        Cow::Owned(format!("{}m ago", secs / SECS_PER_MINUTE))
    } else if secs < SECS_PER_DAY {
        Cow::Owned(format!("{}h ago", secs / SECS_PER_HOUR))
    } else {
        Cow::Owned(format!("{}d ago", secs / SECS_PER_DAY))
    }
}
