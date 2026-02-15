//! Temperature gauges panel rendering.
//!
//! Displays nozzle, bed, and chamber temperatures with visual gauges.
//! Includes fan speed indicators and smart chamber temperature ranges
//! based on the active filament type.

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

/// Temperature threshold above which the heater is considered active (in Celsius)
const ACTIVE_TEMP_THRESHOLD: f32 = 50.0;

/// Temperature difference threshold for considering temp "at target" (in Celsius)
const AT_TARGET_THRESHOLD: f32 = 5.0;

/// Converts a temperature from Celsius to Fahrenheit.
fn celsius_to_fahrenheit(celsius: f32) -> f32 {
    celsius * 9.0 / 5.0 + 32.0
}

/// Formats a temperature value with the appropriate unit symbol.
fn format_temp(celsius: f32, use_celsius: bool) -> String {
    if use_celsius {
        format!("{celsius:.0}°C")
    } else {
        format!("{:.0}°F", celsius_to_fahrenheit(celsius))
    }
}

/// Formats a temperature with target (e.g., "200°C / 210°C").
fn format_temp_with_target(current: f32, target: f32, use_celsius: bool) -> String {
    if use_celsius {
        format!("{current:.0}°C / {target:.0}°C")
    } else {
        format!(
            "{:.0}°F / {:.0}°F",
            celsius_to_fahrenheit(current),
            celsius_to_fahrenheit(target)
        )
    }
}

/// Safe chamber temperature range for a filament type.
struct ChamberRange {
    safe_low: f32,
    safe_high: f32,
}

/// Returns the safe chamber temperature range for a filament type.
///
/// Matches on material string prefix (case-insensitive).
/// Returns a default range for unknown filament types.
fn chamber_range_for_filament(material: &str) -> ChamberRange {
    /// Checks if `s` starts with `prefix` (ASCII case-insensitive).
    fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
        s.len() >= prefix.len()
            && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    }

    // Match on prefix to handle variants like "PLA-CF", "PETG HF", etc.
    if starts_with_ignore_case(material, "PLA") || starts_with_ignore_case(material, "PVA") {
        ChamberRange {
            safe_low: 25.0,
            safe_high: 40.0,
        }
    } else if starts_with_ignore_case(material, "PETG") {
        ChamberRange {
            safe_low: 30.0,
            safe_high: 50.0,
        }
    } else if starts_with_ignore_case(material, "ABS") || starts_with_ignore_case(material, "ASA") {
        ChamberRange {
            safe_low: 40.0,
            safe_high: 60.0,
        }
    } else if starts_with_ignore_case(material, "TPU") {
        ChamberRange {
            safe_low: 25.0,
            safe_high: 40.0,
        }
    } else if starts_with_ignore_case(material, "PA") || starts_with_ignore_case(material, "NYLON")
    {
        ChamberRange {
            safe_low: 45.0,
            safe_high: 65.0,
        }
    } else if starts_with_ignore_case(material, "PC") {
        ChamberRange {
            safe_low: 50.0,
            safe_high: 70.0,
        }
    } else {
        // Default range for unknown filaments
        ChamberRange {
            safe_low: 30.0,
            safe_high: 55.0,
        }
    }
}

/// Returns the required height for the temperatures panel based on printer capabilities.
///
/// Includes 2 for borders plus inner content rows.
/// When a chamber sensor is present and a tray is selected, an additional row is
/// needed for the smart chamber temperature gauge.
pub fn panel_height(has_chamber: bool, has_active_tray: bool) -> u16 {
    // Base: Fans, spacer, Nozzle text+gauge, spacer, Bed text+gauge, spacer = 8 rows
    // With chamber: +2 (text + spacer) or +3 (text + gauge + spacer)
    match (has_chamber, has_active_tray) {
        (true, true) => 13,  // 8 + 3 inner rows + 2 borders
        (true, false) => 12, // 8 + 2 inner rows + 2 borders
        (false, _) => 10,    // 8 inner rows + 2 borders
    }
}

/// Renders the temperatures panel with nozzle, bed, chamber temps and fan speeds.
pub fn render(frame: &mut Frame, printer_state: &PrinterState, use_celsius: bool, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(" Temperatures ", Style::new().fg(Color::Blue)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let has_chamber = printer_state.has_chamber_temp_sensor();
    let active_filament = printer_state.active_filament_type();

    // Build constraints: Fans, Nozzle, Bed, then Chamber at bottom (if present)
    // Max size: 8 base + 3 chamber = 11
    let mut constraints = Vec::with_capacity(11);
    constraints.extend([
        Constraint::Length(1), // Fans
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Nozzle text
        Constraint::Length(1), // Nozzle gauge
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Bed text
        Constraint::Length(1), // Bed gauge
        Constraint::Length(1), // Spacer
    ]);
    if has_chamber {
        constraints.push(Constraint::Length(1)); // Chamber text
        if active_filament.is_some() {
            constraints.push(Constraint::Length(1)); // Chamber gauge
        }
        constraints.push(Constraint::Length(1)); // Spacer
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let temps = &printer_state.temperatures;
    let speeds = &printer_state.speeds;

    // Fan speeds (always at top, conditionally show fans based on printer capabilities)
    let mut fan_spans = Vec::with_capacity(14);
    fan_spans.push(Span::raw(" "));
    fan_spans.push(Span::styled("Fans: ", Style::new().fg(Color::DarkGray)));
    fan_spans.push(Span::styled("Part ", Style::new().fg(Color::DarkGray)));
    fan_spans.push(Span::styled("◆ ", Style::new().fg(Color::DarkGray)));
    fan_spans.push(Span::styled(
        format!("{}%", speeds.fan_speed),
        Style::new().fg(Color::Cyan),
    ));
    if printer_state.has_heatbreak_fan() {
        fan_spans.push(Span::styled(
            "  Heatbreak ",
            Style::new().fg(Color::DarkGray),
        ));
        fan_spans.push(Span::styled("◆ ", Style::new().fg(Color::DarkGray)));
        fan_spans.push(Span::styled(
            format!("{}%", printer_state.heatbreak_fan_speed),
            Style::new().fg(Color::Cyan),
        ));
    }
    if printer_state.has_aux_fan() {
        fan_spans.push(Span::styled("  Aux ", Style::new().fg(Color::DarkGray)));
        fan_spans.push(Span::styled("◆ ", Style::new().fg(Color::DarkGray)));
        fan_spans.push(Span::styled(
            format!("{}%", speeds.aux_fan_speed),
            Style::new().fg(Color::Cyan),
        ));
    }
    if printer_state.has_chamber_fan() {
        fan_spans.push(Span::styled("  Chamber ", Style::new().fg(Color::DarkGray)));
        fan_spans.push(Span::styled("◆ ", Style::new().fg(Color::DarkGray)));
        fan_spans.push(Span::styled(
            format!("{}%", speeds.chamber_fan_speed),
            Style::new().fg(Color::Cyan),
        ));
    }
    let fan_line = Line::from(fan_spans);
    frame.render_widget(Paragraph::new(fan_line), chunks[0]);

    // Nozzle temperature (chunks 2-3)
    render_temp_gauge(
        frame,
        TempGaugeConfig {
            label: "Nozzle",
            current: temps.nozzle,
            target: temps.nozzle_target,
            max_temp: MAX_NOZZLE_TEMP,
        },
        use_celsius,
        chunks[2],
        chunks[3],
    );

    // Bed temperature (chunks 5-6)
    render_temp_gauge(
        frame,
        TempGaugeConfig {
            label: "Bed",
            current: temps.bed,
            target: temps.bed_target,
            max_temp: MAX_BED_TEMP,
        },
        use_celsius,
        chunks[5],
        chunks[6],
    );

    // Chamber temperature at bottom (if chamber sensor present)
    if has_chamber {
        render_chamber_display(
            frame,
            temps.chamber,
            active_filament,
            use_celsius,
            chunks[8], // Chamber text
            if active_filament.is_some() {
                Some(chunks[9]) // Chamber gauge
            } else {
                None
            },
        );
    }
}

/// Renders the chamber temperature display with optional smart gauge.
///
/// When a filament type is active, shows the safe range and a gauge indicating
/// whether the current temperature is within the safe range.
fn render_chamber_display(
    frame: &mut Frame,
    chamber_temp: f32,
    filament_type: Option<&str>,
    use_celsius: bool,
    text_area: Rect,
    gauge_area: Option<Rect>,
) {
    let unit = if use_celsius { "°C" } else { "°F" };
    let (text_spans, gauge_color) = if let Some(material) = filament_type {
        let range = chamber_range_for_filament(material);

        // Determine color based on temperature vs safe range
        let color = if chamber_temp < range.safe_low {
            Color::Cyan // Too cold
        } else if chamber_temp > range.safe_high {
            Color::Red // Too hot
        } else {
            Color::Green // In range
        };

        let (range_low, range_high) = if use_celsius {
            (range.safe_low, range.safe_high)
        } else {
            (
                celsius_to_fahrenheit(range.safe_low),
                celsius_to_fahrenheit(range.safe_high),
            )
        };

        let spans = vec![
            Span::raw(" "),
            Span::styled("Chamber: ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                format_temp(chamber_temp, use_celsius),
                Style::new().fg(color),
            ),
            Span::styled(
                format!(" ({material}: {range_low:.0}-{range_high:.0}{unit})"),
                Style::new().fg(Color::DarkGray),
            ),
        ];

        (spans, Some((color, range)))
    } else {
        // No active tray - simple display
        let spans = vec![
            Span::raw(" "),
            Span::styled("Chamber: ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                format_temp(chamber_temp, use_celsius),
                Style::new().fg(Color::Cyan),
            ),
        ];
        (spans, None)
    };

    frame.render_widget(Paragraph::new(Line::from(text_spans)), text_area);

    // Render gauge if we have an area and color
    if let (Some(area), Some((color, range))) = (gauge_area, gauge_color) {
        // Gauge is calibrated so the safe range spans 25-75%:
        // - 0-25%: too cold (cyan zone)
        // - 25-75%: safe range (green zone)
        // - 75-100%: too hot (red zone)
        let safe_span = range.safe_high - range.safe_low;
        let gauge_min = range.safe_low - 0.5 * safe_span;
        let gauge_max = range.safe_high + 0.5 * safe_span;

        let ratio = ((chamber_temp - gauge_min) / (gauge_max - gauge_min)).clamp(0.0, 1.0) as f64;

        let gauge = LineGauge::default()
            .filled_style(Style::new().fg(color))
            .unfilled_style(Style::new().fg(Color::DarkGray))
            .ratio(ratio)
            .label("");

        // Add right padding to match other gauges
        let padded_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        frame.render_widget(gauge, padded_area[0]);
    }
}

/// Configuration for rendering a temperature gauge.
#[derive(Clone, Copy)]
struct TempGaugeConfig {
    label: &'static str,
    current: f32,
    target: f32,
    /// Maximum temperature for gauge scaling when no target is set
    max_temp: f32,
}

/// Renders a temperature gauge with label and progress bar.
fn render_temp_gauge(
    frame: &mut Frame,
    config: TempGaugeConfig,
    use_celsius: bool,
    text_area: Rect,
    gauge_area: Rect,
) {
    let temp_color =
        if config.target > 0.0 && (config.current - config.target).abs() < AT_TARGET_THRESHOLD {
            Color::Green // At target temperature
        } else if config.target > 0.0 || config.current > ACTIVE_TEMP_THRESHOLD {
            Color::Yellow // Heating or hot
        } else {
            Color::DarkGray // Cold/idle
        };

    let temp_value = if config.target > 0.0 {
        format_temp_with_target(config.current, config.target, use_celsius)
    } else {
        format_temp(config.current, use_celsius)
    };

    let text_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!("{}: ", config.label),
            Style::new().fg(Color::DarkGray),
        ),
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

#[cfg(test)]
mod tests {
    use super::*;

    mod chamber_range_for_filament_tests {
        use super::*;

        #[test]
        fn returns_pla_range() {
            let range = chamber_range_for_filament("PLA");
            assert_eq!(range.safe_low, 25.0);
            assert_eq!(range.safe_high, 40.0);
        }

        #[test]
        fn handles_pla_variants() {
            let range = chamber_range_for_filament("PLA-CF");
            assert_eq!(range.safe_low, 25.0);
            assert_eq!(range.safe_high, 40.0);

            let range = chamber_range_for_filament("PLA Silk");
            assert_eq!(range.safe_low, 25.0);
            assert_eq!(range.safe_high, 40.0);
        }

        #[test]
        fn returns_petg_range() {
            let range = chamber_range_for_filament("PETG");
            assert_eq!(range.safe_low, 30.0);
            assert_eq!(range.safe_high, 50.0);
        }

        #[test]
        fn returns_abs_range() {
            let range = chamber_range_for_filament("ABS");
            assert_eq!(range.safe_low, 40.0);
            assert_eq!(range.safe_high, 60.0);
        }

        #[test]
        fn returns_asa_range() {
            let range = chamber_range_for_filament("ASA");
            assert_eq!(range.safe_low, 40.0);
            assert_eq!(range.safe_high, 60.0);
        }

        #[test]
        fn returns_tpu_range() {
            let range = chamber_range_for_filament("TPU");
            assert_eq!(range.safe_low, 25.0);
            assert_eq!(range.safe_high, 40.0);
        }

        #[test]
        fn returns_pa_range() {
            let range = chamber_range_for_filament("PA");
            assert_eq!(range.safe_low, 45.0);
            assert_eq!(range.safe_high, 65.0);

            let range = chamber_range_for_filament("PA-CF");
            assert_eq!(range.safe_low, 45.0);
            assert_eq!(range.safe_high, 65.0);
        }

        #[test]
        fn returns_nylon_range() {
            let range = chamber_range_for_filament("NYLON");
            assert_eq!(range.safe_low, 45.0);
            assert_eq!(range.safe_high, 65.0);
        }

        #[test]
        fn returns_pc_range() {
            let range = chamber_range_for_filament("PC");
            assert_eq!(range.safe_low, 50.0);
            assert_eq!(range.safe_high, 70.0);
        }

        #[test]
        fn returns_pva_range() {
            let range = chamber_range_for_filament("PVA");
            assert_eq!(range.safe_low, 25.0);
            assert_eq!(range.safe_high, 40.0);
        }

        #[test]
        fn returns_default_for_unknown() {
            let range = chamber_range_for_filament("UNKNOWN");
            assert_eq!(range.safe_low, 30.0);
            assert_eq!(range.safe_high, 55.0);
        }

        #[test]
        fn handles_case_insensitivity() {
            let range = chamber_range_for_filament("pla");
            assert_eq!(range.safe_low, 25.0);
            assert_eq!(range.safe_high, 40.0);

            let range = chamber_range_for_filament("Petg");
            assert_eq!(range.safe_low, 30.0);
            assert_eq!(range.safe_high, 50.0);
        }
    }

    mod panel_height_tests {
        use super::*;

        #[test]
        fn returns_correct_height_with_chamber_and_tray() {
            assert_eq!(panel_height(true, true), 13);
        }

        #[test]
        fn returns_correct_height_with_chamber_no_tray() {
            assert_eq!(panel_height(true, false), 12);
        }

        #[test]
        fn returns_correct_height_without_chamber() {
            assert_eq!(panel_height(false, false), 10);
            assert_eq!(panel_height(false, true), 10);
        }
    }
}
