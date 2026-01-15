mod header;
mod progress;
mod status;
mod temps;

use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App) {
    // Limit width to 100, center horizontally
    let max_width = 100u16;
    let area = frame.area();
    let content_area = if area.width > max_width {
        let padding = (area.width - max_width) / 2;
        Rect::new(area.x + padding, area.y, max_width, area.height)
    } else {
        area
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),   // Header (status + system info)
            Constraint::Length(7),   // Progress (job, spacer, info, bar, spacer)
            Constraint::Length(11),  // Temps + AMS row (fixed height)
            Constraint::Min(1),      // Spacer (absorbs extra space)
            Constraint::Length(1),   // Help bar
        ])
        .split(content_area);

    header::render(frame, app, chunks[0]);
    progress::render(frame, app, chunks[1]);

    // Middle row: temps on left, AMS on right
    let middle_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

    temps::render(frame, app, middle_row[0]);
    status::render_ams(frame, app, middle_row[1]);

    render_help_bar(frame, app, chunks[4]);
}

fn render_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let refresh_status = if app.auto_refresh {
        Span::styled(" ON ", Style::default().fg(Color::Black).bg(Color::Green))
    } else {
        Span::styled(" OFF ", Style::default().fg(Color::Black).bg(Color::Red))
    };

    let last_update = app
        .time_since_update()
        .map(|d| format!("  Updated {}s ago", d.as_secs()))
        .unwrap_or_else(|| "  No data yet".to_string());

    let help = Line::from(vec![
        Span::styled(
            " BAMBUTOP ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" Quit  "),
        Span::styled("r", Style::default().fg(Color::Yellow)),
        Span::raw(" Auto-Refresh "),
        refresh_status,
        Span::styled(
            last_update,
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        ),
    ]);

    frame.render_widget(Paragraph::new(help), area);
}
