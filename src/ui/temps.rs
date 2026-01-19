use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

/// Maximum nozzle temperature for gauge scaling (when no target is set)
const MAX_NOZZLE_TEMP: f32 = 300.0;

/// Maximum bed temperature for gauge scaling (when no target is set)
const MAX_BED_TEMP: f32 = 120.0;

/// Temperature threshold above which the heater is considered active
const ACTIVE_TEMP_THRESHOLD: f32 = 50.0;

/// Temperature difference threshold for considering temp "at target"
const AT_TARGET_THRESHOLD: f32 = 5.0;

/// Renders the temperatures panel with nozzle, bed, chamber temps and fan speeds.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(" Temperatures ", Style::new().fg(Color::Blue)));

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

    // Chamber temperature (at top) - only show if printer has a sensor
    if app.printer_state.has_chamber_temp_sensor() {
        let chamber_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("Chamber: ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}°C", temps.chamber),
                Style::new().fg(Color::Cyan),
            ),
        ]);
        frame.render_widget(Paragraph::new(chamber_line), chunks[0]);
    }

    // Fan speeds
    let fan_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Fans: ", Style::new().fg(Color::DarkGray)),
        Span::styled("Part ", Style::new().fg(Color::DarkGray)),
        Span::styled("◆ ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            format!("{}%", speeds.fan_speed),
            Style::new().fg(Color::Cyan),
        ),
        Span::styled("  Aux ", Style::new().fg(Color::DarkGray)),
        Span::styled("◆ ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            format!("{}%", speeds.aux_fan_speed),
            Style::new().fg(Color::Cyan),
        ),
        Span::styled("  Chamber ", Style::new().fg(Color::DarkGray)),
        Span::styled("◆ ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            format!("{}%", speeds.chamber_fan_speed),
            Style::new().fg(Color::Cyan),
        ),
    ]);
    frame.render_widget(Paragraph::new(fan_line), chunks[1]);

    // Nozzle temperature
    render_temp_gauge(
        frame,
        TempGaugeConfig {
            label: "Nozzle",
            current: temps.nozzle,
            target: temps.nozzle_target,
            max_temp: MAX_NOZZLE_TEMP,
            active_color: Color::Red,
        },
        chunks[3],
        chunks[4],
    );

    // Bed temperature
    render_temp_gauge(
        frame,
        TempGaugeConfig {
            label: "Bed",
            current: temps.bed,
            target: temps.bed_target,
            max_temp: MAX_BED_TEMP,
            active_color: Color::Magenta,
        },
        chunks[6],
        chunks[7],
    );
}

/// Configuration for rendering a temperature gauge.
struct TempGaugeConfig {
    label: &'static str,
    current: f32,
    target: f32,
    /// Maximum temperature for gauge scaling when no target is set
    max_temp: f32,
    /// Color to use when temperature is above 50°C but not at target
    active_color: Color,
}

/// Renders a temperature gauge with label and progress bar.
fn render_temp_gauge(
    frame: &mut Frame,
    config: TempGaugeConfig,
    text_area: Rect,
    gauge_area: Rect,
) {
    let color =
        if config.target > 0.0 && (config.current - config.target).abs() < AT_TARGET_THRESHOLD {
            Color::Green
        } else if config.current > ACTIVE_TEMP_THRESHOLD {
            config.active_color
        } else {
            Color::DarkGray
        };

    let text = if config.target > 0.0 {
        format!(
            " {}: {:.0}°C / {:.0}°C",
            config.label, config.current, config.target
        )
    } else {
        format!(" {}: {:.0}°C", config.label, config.current)
    };

    frame.render_widget(
        Paragraph::new(Span::styled(text, Style::new().fg(color))),
        text_area,
    );

    let ratio = if config.target > 0.0 {
        (config.current / config.target).min(1.0) as f64
    } else {
        (config.current / config.max_temp) as f64
    };

    let gauge = Gauge::default()
        .gauge_style(Style::new().fg(color).bg(Color::DarkGray))
        .ratio(ratio)
        .label("");
    frame.render_widget(gauge, gauge_area);
}
