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
    let bed_color = if temps.bed_target > 0.0 && (temps.bed - temps.bed_target).abs() < 5.0 {
        Color::Green
    } else if temps.bed > 50.0 {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    let bed_text = if temps.bed_target > 0.0 {
        format!(" Bed: {:.0}°C / {:.0}°C", temps.bed, temps.bed_target)
    } else {
        format!(" Bed: {:.0}°C", temps.bed)
    };

    frame.render_widget(
        Paragraph::new(Span::styled(bed_text, Style::default().fg(bed_color))),
        chunks[6],
    );

    // Bed gauge
    let bed_ratio = if temps.bed_target > 0.0 {
        (temps.bed / temps.bed_target).min(1.0) as f64
    } else {
        (temps.bed / 120.0) as f64
    };
    let bed_gauge = Gauge::default()
        .gauge_style(Style::default().fg(bed_color).bg(Color::DarkGray))
        .ratio(bed_ratio)
        .label("");
    frame.render_widget(bed_gauge, chunks[7]);
}
