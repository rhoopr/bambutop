use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(Span::styled(
            " Temperatures ",
            Style::default().fg(Color::Blue),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Chamber
            Constraint::Length(1), // Fans
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Nozzle text
            Constraint::Length(1), // Nozzle gauge
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Bed text
            Constraint::Length(1), // Bed gauge
            Constraint::Length(1), // Spacer
        ])
        .split(inner);

    let temps = &app.printer_state.temperatures;
    let speeds = &app.printer_state.speeds;

    // Chamber temperature (at top)
    let chamber_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Chamber: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.0}°C", temps.chamber),
            Style::default().fg(Color::Cyan),
        ),
    ]);
    frame.render_widget(Paragraph::new(chamber_line), chunks[0]);

    // Fan speeds
    let fan_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Fans: ", Style::default().fg(Color::DarkGray)),
        Span::styled("Part ", Style::default().fg(Color::DarkGray)),
        Span::styled("◆ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}%", speeds.fan_speed), Style::default().fg(Color::Cyan)),
        Span::styled("  Aux ", Style::default().fg(Color::DarkGray)),
        Span::styled("◆ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}%", speeds.aux_fan_speed), Style::default().fg(Color::Cyan)),
        Span::styled("  Chamber ", Style::default().fg(Color::DarkGray)),
        Span::styled("◆ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}%", speeds.chamber_fan_speed), Style::default().fg(Color::Cyan)),
    ]);
    frame.render_widget(Paragraph::new(fan_line), chunks[1]);

    // Nozzle temperature
    let nozzle_color = if temps.nozzle_target > 0.0 && (temps.nozzle - temps.nozzle_target).abs() < 5.0 {
        Color::Green
    } else if temps.nozzle > 50.0 {
        Color::Red
    } else {
        Color::DarkGray
    };

    let nozzle_text = if temps.nozzle_target > 0.0 {
        format!(" Nozzle: {:.0}°C / {:.0}°C", temps.nozzle, temps.nozzle_target)
    } else {
        format!(" Nozzle: {:.0}°C", temps.nozzle)
    };

    frame.render_widget(
        Paragraph::new(Span::styled(nozzle_text, Style::default().fg(nozzle_color))),
        chunks[3],
    );

    // Nozzle gauge
    let nozzle_ratio = if temps.nozzle_target > 0.0 {
        (temps.nozzle / temps.nozzle_target).min(1.0) as f64
    } else {
        (temps.nozzle / 300.0) as f64
    };
    let nozzle_gauge = Gauge::default()
        .gauge_style(Style::default().fg(nozzle_color).bg(Color::DarkGray))
        .ratio(nozzle_ratio)
        .label("");
    frame.render_widget(nozzle_gauge, chunks[4]);

    // Bed temperature
    render_temp_gauge(
        frame,
        chunks[6],
        chunks[7],
        "Bed",
        temps.bed,
        temps.bed_target,
        120.0,
        Color::Magenta,
    );
}

fn render_temp_gauge(
    frame: &mut Frame,
    text_area: Rect,
    gauge_area: Rect,
    label: &str,
    current: f32,
    target: f32,
    max: f32,
    color: Color,
) {
    let temp_text = if target > 0.0 {
        format!(" {}: {:.0}°C / {:.0}°C", label, current, target)
    } else {
        format!(" {}: {:.0}°C", label, current)
    };

    let temp_color = if target > 0.0 && (current - target).abs() < 5.0 {
        Color::Green
    } else if current > 50.0 {
        color
    } else {
        Color::DarkGray
    };

    frame.render_widget(
        Paragraph::new(Span::styled(temp_text, Style::default().fg(temp_color))),
        text_area,
    );

    let ratio = if target > 0.0 {
        (current / target).min(1.0) as f64
    } else {
        (current / max) as f64
    };

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(temp_color).bg(Color::DarkGray))
        .ratio(ratio)
        .label("");

    frame.render_widget(gauge, gauge_area);
}
