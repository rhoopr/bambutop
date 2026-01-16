use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(Span::styled(
            " Print Progress ",
            Style::default().fg(Color::Blue),
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
    let job_name = {
        let name = print_status.display_name();
        if name.is_empty() {
            "No print job".to_string()
        } else {
            name
        }
    };

    let file_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Job: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate_str(&job_name, 70),
            Style::default().fg(Color::White),
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
        Span::styled("Speed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}%", speed_percent), Style::default().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled("Layer: ", Style::default().fg(Color::DarkGray)),
        Span::styled(layer_value, Style::default().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled("Remaining: ", Style::default().fg(Color::DarkGray)),
        Span::styled(time_remaining, Style::default().fg(Color::Cyan)),
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
        .gauge_style(Style::default().fg(progress_color).bg(Color::DarkGray))
        .ratio(progress)
        .label(Span::styled(
            format!("{}%", print_status.progress),
            Style::default().add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(gauge, chunks[3]);
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn format_time(mins: u32) -> String {
    if mins == 0 {
        "--:--".to_string()
    } else {
        let hours = mins / 60;
        let minutes = mins % 60;
        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    }
}

fn speed_level_to_percent(level: u8) -> u32 {
    // Bambu speed levels: 1=silent, 2=standard, 3=sport, 4=ludicrous
    match level {
        1 => 50,
        2 => 100,
        3 => 124,
        4 => 166,
        _ => 100,
    }
}
