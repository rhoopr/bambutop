use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

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
        "Failed" => Color::Red,
        "Disconnected" => Color::Red,
        "Idle" => Color::Cyan,
        _ => Color::White,
    };

    let model = if app.printer_state.printer_model.is_empty() {
        "Bambu Printer".to_string()
    } else {
        app.printer_state.printer_model.clone()
    };
    let title = format!(" {} ", model);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(status_color))
        .title(Span::styled(title, Style::default().fg(status_color)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let status_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(" {} ", status),
            Style::default()
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
    let title = if has_errors { " Errors " } else { " System " };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title, Style::default().fg(border_color)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(err) = &app.error_message {
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(err.clone(), Style::default().fg(Color::Red)),
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
                Span::styled(&error.message, Style::default().fg(severity_color)),
            ]));
        }
    } else {
        // Show WiFi signal and system status on the same line
        let wifi_spans = render_wifi_signal(&app.printer_state.wifi_signal);
        let mut line_spans = vec![Span::raw(" ")];
        line_spans.extend(wifi_spans);
        line_spans.push(Span::raw("  "));
        line_spans.push(Span::styled("All systems normal", Style::default().fg(Color::Green)));
        lines.push(Line::from(line_spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Renders WiFi signal with visual bars and color coding.
/// Good signal: > -50dBm (green), Medium: -50 to -70dBm (yellow), Weak: < -70dBm (red)
fn render_wifi_signal(wifi_signal: &str) -> Vec<Span<'static>> {
    if wifi_signal.is_empty() {
        return vec![
            Span::styled("WiFi: ", Style::default().fg(Color::Gray)),
            Span::styled("--", Style::default().fg(Color::Gray)),
        ];
    }

    // Parse dBm value from string (e.g., "-45dBm" or "-45")
    let dbm: i32 = wifi_signal
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect::<String>()
        .parse()
        .unwrap_or(-100);

    // Determine signal strength and color
    let (color, bars) = if dbm > -50 {
        // Strong signal
        (Color::Green, "\u{2582}\u{2584}\u{2586}\u{2588}")
    } else if dbm > -70 {
        // Medium signal
        (Color::Yellow, "\u{2582}\u{2584}\u{2586} ")
    } else {
        // Weak signal
        (Color::Red, "\u{2582}\u{2584}  ")
    };

    vec![
        Span::styled("WiFi: ", Style::default().fg(Color::Gray)),
        Span::styled(bars.to_string(), Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(wifi_signal.to_string(), Style::default().fg(color)),
    ]
}
