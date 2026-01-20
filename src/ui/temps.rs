use crate::printer::PrinterState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, LineGauge, Paragraph},
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

/// Returns the required height for the temperatures panel based on printer capabilities.
/// Includes 2 for borders plus inner content rows.
pub fn panel_height(has_chamber: bool) -> u16 {
    if has_chamber {
        11 // 9 inner rows + 2 borders
    } else {
        10 // 8 inner rows + 2 borders
    }
}

/// Renders the temperatures panel with nozzle, bed, chamber temps and fan speeds.
pub fn render(frame: &mut Frame, printer_state: &PrinterState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(" Temperatures ", Style::new().fg(Color::Blue)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let has_chamber = printer_state.has_chamber_temp_sensor();

    // Build constraints dynamically based on chamber sensor
    let mut constraints = vec![Constraint::Length(1)]; // Fans
    if has_chamber {
        constraints.push(Constraint::Length(1)); // Chamber
    }
    constraints.extend([
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Nozzle text
        Constraint::Length(1), // Nozzle gauge
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Bed text
        Constraint::Length(1), // Bed gauge
        Constraint::Length(1), // Spacer
    ]);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let temps = &printer_state.temperatures;
    let speeds = &printer_state.speeds;

    // Chunk offset: if chamber is present, indices shift by 1 after fans
    let offset = if has_chamber { 1 } else { 0 };

    // Fan speeds (always at top)
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
    frame.render_widget(Paragraph::new(fan_line), chunks[0]);

    // Chamber temperature (second line if present)
    if has_chamber {
        let chamber_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("Chamber: ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}°C", temps.chamber),
                Style::new().fg(Color::Cyan),
            ),
        ]);
        frame.render_widget(Paragraph::new(chamber_line), chunks[1]);
    }

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
        chunks[2 + offset],
        chunks[3 + offset],
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
        chunks[5 + offset],
        chunks[6 + offset],
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
    let temp_color =
        if config.target > 0.0 && (config.current - config.target).abs() < AT_TARGET_THRESHOLD {
            Color::Green
        } else if config.current > ACTIVE_TEMP_THRESHOLD {
            config.active_color
        } else {
            Color::DarkGray
        };

    let temp_value = if config.target > 0.0 {
        format!("{:.0}°C / {:.0}°C", config.current, config.target)
    } else {
        format!("{:.0}°C", config.current)
    };

    let text_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("{}: ", config.label), Style::new().fg(Color::DarkGray)),
        Span::styled(temp_value, Style::new().fg(temp_color)),
    ]);

    frame.render_widget(Paragraph::new(text_line), text_area);

    let ratio = if config.target > 0.0 {
        (config.current / config.target).min(1.0) as f64
    } else {
        (config.current / config.max_temp) as f64
    };

    let gauge = LineGauge::default()
        .filled_style(Style::new().fg(temp_color))
        .unfilled_style(Style::new().fg(Color::DarkGray))
        .ratio(ratio)
        .label("");

    // Add right padding
    let padded_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(gauge_area);
    frame.render_widget(gauge, padded_area[0]);
}
