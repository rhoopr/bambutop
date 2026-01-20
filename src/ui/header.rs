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

/// WiFi signal threshold for strong signal (dBm)
const WIFI_STRONG_THRESHOLD: i32 = -50;

/// WiFi signal threshold for medium signal (dBm)
const WIFI_MEDIUM_THRESHOLD: i32 = -70;

/// Default dBm value when signal cannot be parsed
const WIFI_DEFAULT_DBM: i32 = -100;

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

    let model = if printer_state.printer_model.is_empty() {
        "Bambu Printer"
    } else {
        &printer_state.printer_model
    };
    let title = format!(" {} ", model);
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
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(error.message.as_str(), Style::new().fg(severity_color)),
            ]));
        }
    } else {
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
fn render_wifi_signal(wifi_signal: &str) -> Vec<Span<'static>> {
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
        Span::styled(wifi_signal.to_string(), Style::new().fg(color)),
        Span::raw(" "),
    ]
}

/// Parses dBm value from a string like "-45dBm" or "-45" without allocation.
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

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_dbm_tests {
        use super::*;

        #[test]
        fn parses_negative_with_suffix() {
            assert_eq!(parse_dbm("-45dBm"), Some(-45));
            assert_eq!(parse_dbm("-70dBm"), Some(-70));
        }

        #[test]
        fn parses_negative_without_suffix() {
            assert_eq!(parse_dbm("-45"), Some(-45));
            assert_eq!(parse_dbm("-100"), Some(-100));
        }

        #[test]
        fn parses_positive_values() {
            assert_eq!(parse_dbm("45"), Some(45));
            assert_eq!(parse_dbm("0"), Some(0));
        }

        #[test]
        fn returns_none_for_empty() {
            assert_eq!(parse_dbm(""), None);
        }

        #[test]
        fn returns_none_for_no_digits() {
            assert_eq!(parse_dbm("dBm"), None);
            assert_eq!(parse_dbm("-"), None);
            assert_eq!(parse_dbm("abc"), None);
        }

        #[test]
        fn handles_whitespace_in_value() {
            // Digits are extracted regardless of surrounding text
            assert_eq!(parse_dbm("Signal: -45 dBm"), Some(-45));
        }

        #[test]
        fn saturates_on_overflow() {
            // Very large numbers saturate instead of overflowing
            let result = parse_dbm("99999999999999999999");
            assert!(result.is_some());
            // Should be saturated to i32::MAX
            assert_eq!(result, Some(i32::MAX));
        }

        #[test]
        fn handles_multiple_minus_signs() {
            // Only the first minus before digits is used
            assert_eq!(parse_dbm("--45"), Some(-45));
        }

        #[test]
        fn concatenates_all_digit_sequences() {
            // Documents behavior: all digits in the string are concatenated
            // This matches the actual implementation behavior
            assert_eq!(parse_dbm("-45abc67"), Some(-4567));
        }

        #[test]
        fn minus_after_digits_is_ignored() {
            // Minus sign only counts if before any digits
            assert_eq!(parse_dbm("45-67"), Some(4567));
        }
    }
}
