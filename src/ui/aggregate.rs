//! Aggregate view rendering for multi-printer display.
//!
//! Displays all configured printers in a compact grid layout. Each printer
//! is shown as a card with connection status, job info, and progress.
//! The currently selected printer has a yellow border.

use crate::app::App;
use crate::printer::PrinterState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::borrow::Cow;

/// Minimum width for a printer card
const CARD_MIN_WIDTH: u16 = 40;

/// Height for a printer card (including borders)
const CARD_HEIGHT: u16 = 6;

/// Maximum display length for job names before truncation
const MAX_JOB_NAME_LEN: usize = 25;

/// Prefix used in full printer model names from Bambu
const MODEL_PREFIX: &str = "Bambu Lab ";

/// Number of serial number digits to show in compact title
const SERIAL_SUFFIX_LENGTH: usize = 4;

/// WiFi signal threshold for strong signal (dBm)
const WIFI_STRONG_THRESHOLD: i32 = -50;

/// WiFi signal threshold for medium signal (dBm)
const WIFI_MEDIUM_THRESHOLD: i32 = -70;

/// Default dBm value when signal cannot be parsed
const WIFI_DEFAULT_DBM: i32 = -100;

/// Renders the aggregate view showing all printers in a grid layout.
///
/// # Arguments
/// * `frame` - The ratatui frame to render to
/// * `app` - Application state containing printer information
/// * `area` - The rectangular area to render within
pub fn render_aggregate(frame: &mut Frame, app: &App, area: Rect) {
    let printer_count = app.printer_count();

    if printer_count == 0 {
        render_no_printers(frame, area);
        return;
    }

    // Calculate grid layout: 2 columns if >1 printer, single column otherwise
    let columns = if printer_count > 1 { 2 } else { 1 };
    let rows = (printer_count + columns - 1) / columns;

    // Create row constraints
    let row_constraints: Vec<Constraint> = (0..rows)
        .map(|_| Constraint::Length(CARD_HEIGHT))
        .chain(std::iter::once(Constraint::Min(0))) // Absorb remaining space
        .collect();

    let row_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    // Create column constraints
    let col_constraints: Vec<Constraint> = (0..columns)
        .map(|_| Constraint::Min(CARD_MIN_WIDTH))
        .collect();

    let active_index = app.active_printer_index();

    for row in 0..rows {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints.clone())
            .split(row_layout[row]);

        for col in 0..columns {
            let printer_index = row * columns + col;
            if printer_index >= printer_count {
                break;
            }

            let card_area = cols[col];
            let is_selected = printer_index == active_index;
            let is_connected = app.is_printer_connected(printer_index);

            if let Some(printer_state_arc) = app.get_printer(printer_index) {
                let printer_state = printer_state_arc
                    .lock()
                    .expect("printer state lock poisoned");
                render_printer_card(
                    frame,
                    &printer_state,
                    is_selected,
                    is_connected,
                    app.get_printer_last_update(printer_index),
                    card_area,
                );
            }
        }
    }
}

/// Renders a message when no printers are configured.
fn render_no_printers(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray))
        .title(Span::styled(
            " No Printers ",
            Style::new().fg(Color::DarkGray),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let message = Paragraph::new(Line::from(Span::styled(
        "No printers configured",
        Style::new().fg(Color::DarkGray),
    )));
    frame.render_widget(message, inner);
}

/// Renders a single printer card with status information.
///
/// # Arguments
/// * `frame` - The ratatui frame to render to
/// * `printer_state` - Snapshot of the printer's current state
/// * `is_selected` - Whether this printer is currently selected (yellow border)
/// * `is_connected` - Whether the printer is currently connected
/// * `last_update` - Timestamp of the last state update
/// * `area` - The rectangular area for this card
fn render_printer_card(
    frame: &mut Frame,
    printer_state: &PrinterState,
    is_selected: bool,
    is_connected: bool,
    last_update: Option<std::time::Instant>,
    area: Rect,
) {
    // Border color: yellow for selected, gray for unselected
    let border_color = if is_selected {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    // Format card title with model name
    let title = format_card_title(printer_state);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color))
        .title(Span::styled(
            format!(" {} ", title),
            Style::new().fg(border_color),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build card content lines
    let mut lines = Vec::with_capacity(4);

    // Line 1: Connection status dot, state, HMS status, WiFi
    lines.push(build_status_line(printer_state, is_connected));

    // Line 2: Last update time
    lines.push(build_update_line(last_update));

    // Line 3: Job name and progress
    lines.push(build_job_line(printer_state));

    // Line 4: Layer info and remaining time
    lines.push(build_progress_line(printer_state));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Formats the card title from printer model and serial suffix.
fn format_card_title(printer_state: &PrinterState) -> Cow<'static, str> {
    let model = if printer_state.printer_model.is_empty() {
        "Bambu Printer"
    } else {
        &printer_state.printer_model
    };

    // Strip "Bambu Lab " prefix for compactness
    let short_model = model.strip_prefix(MODEL_PREFIX).unwrap_or(model);

    // Extract serial suffix
    let serial_suffix = if printer_state.serial_suffix.len() >= SERIAL_SUFFIX_LENGTH {
        &printer_state.serial_suffix[printer_state.serial_suffix.len() - SERIAL_SUFFIX_LENGTH..]
    } else if !printer_state.serial_suffix.is_empty() {
        &printer_state.serial_suffix
    } else {
        ""
    };

    if serial_suffix.is_empty() {
        Cow::Owned(short_model.to_string())
    } else {
        Cow::Owned(format!("{} ...{}", short_model, serial_suffix))
    }
}

/// Builds the status line with connection dot, state, HMS, and WiFi.
fn build_status_line(printer_state: &PrinterState, is_connected: bool) -> Line<'static> {
    let mut spans = Vec::with_capacity(8);

    // Connection status dot
    let (dot_color, dot_char) = if is_connected {
        (Color::Green, "\u{25CF}") // Filled circle
    } else {
        (Color::Red, "\u{25CF}") // Filled circle (red)
    };
    spans.push(Span::styled(
        format!(" {} ", dot_char),
        Style::new().fg(dot_color),
    ));

    // Printer state
    let state_text = match printer_state.print_status.gcode_state.as_str() {
        "IDLE" => "Idle",
        "PREPARE" => "Preparing",
        "RUNNING" => "Printing",
        "PAUSE" => "Paused",
        "FINISH" => "Finished",
        "FAILED" => "Failed",
        "" => "Connecting",
        _ => "Unknown",
    };
    let state_color = match state_text {
        "Printing" => Color::Green,
        "Paused" => Color::Yellow,
        "Failed" => Color::Red,
        "Idle" | "Finished" => Color::Cyan,
        _ => Color::White,
    };
    spans.push(Span::styled(state_text, Style::new().fg(state_color)));

    // HMS status indicator
    if !printer_state.hms_errors.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "\u{26A0}", // Warning triangle
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    // WiFi signal
    spans.push(Span::raw("  "));
    spans.extend(render_wifi_compact(&printer_state.wifi_signal));

    Line::from(spans)
}

/// Renders a compact WiFi signal indicator.
fn render_wifi_compact(wifi_signal: &str) -> Vec<Span<'static>> {
    if wifi_signal.is_empty() {
        return vec![Span::styled("--", Style::new().fg(Color::DarkGray))];
    }

    let dbm = parse_dbm(wifi_signal).unwrap_or(WIFI_DEFAULT_DBM);
    let (color, bars) = if dbm > WIFI_STRONG_THRESHOLD {
        (Color::Green, "\u{2582}\u{2584}\u{2586}\u{2588}")
    } else if dbm > WIFI_MEDIUM_THRESHOLD {
        (Color::Yellow, "\u{2582}\u{2584}\u{2586} ")
    } else {
        (Color::Red, "\u{2582}\u{2584}  ")
    };

    vec![Span::styled(bars.to_string(), Style::new().fg(color))]
}

/// Parses dBm value from a string like "-45dBm" without allocation.
fn parse_dbm(s: &str) -> Option<i32> {
    let mut result: i32 = 0;
    let mut negative = false;
    let mut found_digit = false;

    for c in s.chars() {
        if c == '-' && !found_digit {
            negative = true;
        } else if c.is_ascii_digit() {
            found_digit = true;
            result = result
                .saturating_mul(10)
                .saturating_add((c as i32) - ('0' as i32));
        }
    }

    if found_digit {
        Some(if negative { -result } else { result })
    } else {
        None
    }
}

/// Builds the last update time line.
fn build_update_line(last_update: Option<std::time::Instant>) -> Line<'static> {
    let update_text = match last_update {
        Some(instant) => {
            let elapsed = instant.elapsed().as_secs();
            if elapsed < 60 {
                format!(" Updated {}s ago", elapsed)
            } else if elapsed < 3600 {
                format!(" Updated {}m ago", elapsed / 60)
            } else {
                format!(" Updated {}h ago", elapsed / 3600)
            }
        }
        None => " No data yet".to_string(),
    };

    Line::from(Span::styled(update_text, Style::new().fg(Color::DarkGray)))
}

/// Builds the job name and progress line.
fn build_job_line(printer_state: &PrinterState) -> Line<'static> {
    let print_status = &printer_state.print_status;

    // Get job display name
    let job_name = print_status.display_name();
    let job_display: Cow<'_, str> = if job_name.is_empty() {
        Cow::Borrowed("No job")
    } else {
        job_name
    };

    // Truncate if needed
    let truncated_name = truncate_str(&job_display, MAX_JOB_NAME_LEN);

    // Progress percentage
    let progress = print_status.progress;
    let progress_color = if progress >= 100 {
        Color::Green
    } else if progress > 0 {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    Line::from(vec![
        Span::raw(" "),
        Span::styled(truncated_name.into_owned(), Style::new().fg(Color::White)),
        Span::raw(" "),
        Span::styled(format!("{}%", progress), Style::new().fg(progress_color)),
    ])
}

/// Builds the layer info and remaining time line.
fn build_progress_line(printer_state: &PrinterState) -> Line<'static> {
    let print_status = &printer_state.print_status;

    // Layer info
    let layer_text = if print_status.total_layers > 0 {
        format!("L{}/{}", print_status.layer_num, print_status.total_layers)
    } else {
        "L-/-".to_string()
    };

    // Remaining time
    let time_text = format_time(print_status.remaining_time_mins);

    Line::from(vec![
        Span::raw(" "),
        Span::styled(layer_text, Style::new().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("ETA: ", Style::new().fg(Color::DarkGray)),
        Span::styled(time_text.into_owned(), Style::new().fg(Color::Cyan)),
    ])
}

/// Truncates a string to a maximum length, adding "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> Cow<'_, str> {
    if s.len() <= max_len {
        return Cow::Borrowed(s);
    }

    let target_len = max_len.saturating_sub(3);
    let mut end = target_len.min(s.len());

    // Find valid char boundary
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    if end == 0 {
        Cow::Borrowed("...")
    } else {
        Cow::Owned(format!("{}...", &s[..end]))
    }
}

/// Formats minutes into a human-readable time string.
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

#[cfg(test)]
mod tests {
    use super::*;

    mod truncate_str_tests {
        use super::*;

        #[test]
        fn returns_borrowed_when_short_enough() {
            let result = truncate_str("short", 20);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "short");
        }

        #[test]
        fn truncates_with_ellipsis() {
            let result = truncate_str("this is a very long string", 15);
            assert_eq!(result, "this is a ve...");
        }

        #[test]
        fn handles_exact_length() {
            let result = truncate_str("exact", 5);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "exact");
        }
    }

    mod format_time_tests {
        use super::*;

        #[test]
        fn returns_placeholder_for_zero() {
            let result = format_time(0);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "--:--");
        }

        #[test]
        fn formats_minutes_only() {
            let result = format_time(45);
            assert_eq!(result, "45m");
        }

        #[test]
        fn formats_hours_and_minutes() {
            let result = format_time(90);
            assert_eq!(result, "1h 30m");
        }
    }

    mod parse_dbm_tests {
        use super::*;

        #[test]
        fn parses_negative_with_suffix() {
            assert_eq!(parse_dbm("-45dBm"), Some(-45));
        }

        #[test]
        fn parses_negative_without_suffix() {
            assert_eq!(parse_dbm("-70"), Some(-70));
        }

        #[test]
        fn returns_none_for_empty() {
            assert_eq!(parse_dbm(""), None);
        }
    }
}
