use crate::app::App;
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
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    // Split into two boxes side by side
    let boxes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(1)])
        .split(area);

    render_status_box(frame, app, boxes[0]);
    render_system_box(frame, app, boxes[1]);
}

fn render_status_box(frame: &mut Frame, app: &App, area: Rect) {
    let status = app.status_text();
    let status_color = match status {
        "Printing" => Color::Green,
        "Paused" => Color::Yellow,
        "Failed" | "Disconnected" => Color::Red,
        "Idle" => Color::Cyan,
        _ => Color::White,
    };

    let model = if app.printer_state.printer_model.is_empty() {
        "Bambu Printer"
    } else {
        &app.printer_state.printer_model
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

fn render_system_box(frame: &mut Frame, app: &App, area: Rect) {
    let has_errors = !app.printer_state.hms_errors.is_empty() || app.error_message.is_some();

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
    } else if !app.printer_state.hms_errors.is_empty() {
        for error in &app.printer_state.hms_errors {
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
    let wifi_spans = render_wifi_signal(&app.printer_state.wifi_signal);
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
        ];
    }

    // Parse dBm value from string (e.g., "-45dBm" or "-45")
    let dbm: i32 = wifi_signal
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect::<String>()
        .parse()
        .unwrap_or(WIFI_DEFAULT_DBM);

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
    ]
}
