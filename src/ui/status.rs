use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use smallvec::SmallVec;

/// Estimated line count for AMS display pre-allocation
const AMS_LINES_ESTIMATE: usize = 20;

/// Renders the AMS (Automatic Material System) status panel.
pub fn render_ams(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(" AMS ", Style::new().fg(Color::Blue)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: SmallVec<[Line; AMS_LINES_ESTIMATE]> = SmallVec::new();

    if let Some(ams) = &app.printer_state.ams {
        let num_units = ams.units.len();

        for unit in &ams.units {
            // Check if this unit is currently active
            let is_active_unit = ams.current_unit == Some(unit.id);

            // Separator line between units (only if multiple units)
            if unit.id > 0 && num_units > 1 {
                lines.push(Line::from(Span::styled(
                    "  ────────────────────────",
                    Style::new().fg(Color::DarkGray),
                )));
            }

            // Spacer above unit
            lines.push(Line::from(""));

            // Unit header with active indicator and Lite badge
            let unit_label = if unit.is_lite {
                format!(" Unit {} [Lite]", unit.id + 1)
            } else {
                format!(" Unit {}", unit.id + 1)
            };

            let unit_style = if is_active_unit {
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::DarkGray)
            };

            let mut header_spans: SmallVec<[Span; 2]> = SmallVec::new();
            if is_active_unit {
                header_spans.push(Span::styled("▶", Style::new().fg(Color::Green)));
            } else {
                header_spans.push(Span::styled(" ", Style::new()));
            }
            header_spans.push(Span::styled(unit_label, unit_style));

            lines.push(Line::from(header_spans.into_vec()));

            // Humidity line with grade widget (skip for AMS Lite which has no humidity sensor)
            if !unit.is_lite {
                // Bambu humidity scale: 5=Dry(A), 4(B), 3(C), 2(D), 1=Wet(E)
                let current_grade = match unit.humidity {
                    5 => 'A',
                    4 => 'B',
                    3 => 'C',
                    2 => 'D',
                    1 => 'E',
                    _ => '?',
                };

                let mut humidity_spans: SmallVec<[Span; 14]> = SmallVec::new();
                humidity_spans.push(Span::styled(
                    "   Humidity: ",
                    Style::new().fg(Color::DarkGray),
                ));
                humidity_spans.push(Span::styled("Dry ", Style::new().fg(Color::DarkGray)));
                humidity_spans.push(Span::styled("◆ ", Style::new().fg(Color::DarkGray)));

                for (i, grade) in ['A', 'B', 'C', 'D', 'E'].iter().enumerate() {
                    let grade_color = match grade {
                        'A' | 'B' => Color::Green,
                        'C' => Color::Yellow,
                        'D' => Color::Rgb(255, 165, 0), // Orange
                        'E' => Color::Red,
                        _ => Color::DarkGray,
                    };
                    let style = if *grade == current_grade {
                        Style::new().fg(grade_color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(Color::DarkGray)
                    };
                    humidity_spans.push(Span::styled(grade.to_string(), style));
                    if i < 4 {
                        humidity_spans.push(Span::styled("-", Style::new().fg(Color::DarkGray)));
                    }
                }

                humidity_spans.push(Span::styled(" ◆", Style::new().fg(Color::DarkGray)));
                humidity_spans.push(Span::styled(" Wet", Style::new().fg(Color::DarkGray)));
                lines.push(Line::from(humidity_spans.into_vec()));
            }

            // Filament header
            lines.push(Line::from(Span::styled(
                "   Filament:",
                Style::new().fg(Color::DarkGray),
            )));

            // Filament slots
            for tray in &unit.trays {
                // Use cached parsed color if available, otherwise fall back to white
                let color = tray
                    .parsed_color
                    .map(|(r, g, b)| Color::Rgb(r, g, b))
                    .unwrap_or(Color::White);
                // A tray is active if both the unit and tray slot match
                let is_active_tray = is_active_unit && ams.current_tray == Some(tray.id);
                let marker = if is_active_tray { "▶" } else { " " };

                // Show percentage if reported, hide if unknown (0 often means not reported)
                let remaining_text = if tray.remaining == 0 || tray.material.is_empty() {
                    String::new()
                } else {
                    format!(" {}%", tray.remaining)
                };

                let remaining_color = match tray.remaining {
                    0 => Color::DarkGray,
                    1..=20 => Color::Yellow,
                    _ => Color::Green,
                };

                let slot_style = if is_active_tray {
                    Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(Color::DarkGray)
                };

                let material_display = if tray.material.is_empty() {
                    "Empty"
                } else {
                    &tray.material
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("    {}[{}] ", marker, tray.id + 1), slot_style),
                    Span::styled("██", Style::new().fg(color)),
                    Span::raw(" "),
                    Span::styled(material_display, Style::new().fg(Color::White)),
                    Span::styled(remaining_text, Style::new().fg(remaining_color)),
                ]));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No AMS detected",
            Style::new().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(Paragraph::new(lines.into_vec()), inner);
}
