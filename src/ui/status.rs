use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_ams(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(Span::styled(" AMS ", Style::default().fg(Color::Blue)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(ams) = &app.printer_state.ams {
        for unit in &ams.units {
            // Spacer above unit
            lines.push(Line::from(""));

            // Unit header
            lines.push(Line::from(Span::styled(
                format!(" Unit {}", unit.id + 1),
                Style::default().fg(Color::DarkGray),
            )));

            // Humidity line with grade widget
            // Bambu humidity scale: 5=Dry(A), 4(B), 3(C), 2(D), 1=Wet(E)
            let current_grade = match unit.humidity {
                5 => 'A',
                4 => 'B',
                3 => 'C',
                2 => 'D',
                1 => 'E',
                _ => '?',
            };

            let mut humidity_spans = vec![
                Span::styled("   Humidity: ", Style::default().fg(Color::DarkGray)),
                Span::styled("Dry ", Style::default().fg(Color::DarkGray)),
                Span::styled("◆ ", Style::default().fg(Color::DarkGray)),
            ];

            for (i, grade) in ['A', 'B', 'C', 'D', 'E'].iter().enumerate() {
                let grade_color = match grade {
                    'A' | 'B' => Color::Green,
                    'C' => Color::Yellow,
                    'D' => Color::Rgb(255, 165, 0), // Orange
                    'E' => Color::Red,
                    _ => Color::DarkGray,
                };
                let style = if *grade == current_grade {
                    Style::default().fg(grade_color).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                humidity_spans.push(Span::styled(grade.to_string(), style));
                if i < 4 {
                    humidity_spans.push(Span::styled("-", Style::default().fg(Color::DarkGray)));
                }
            }

            humidity_spans.push(Span::styled(" ◆", Style::default().fg(Color::DarkGray)));
            humidity_spans.push(Span::styled(" Wet", Style::default().fg(Color::DarkGray)));
            lines.push(Line::from(humidity_spans));

            // Filament header
            lines.push(Line::from(Span::styled(
                "   Filament:",
                Style::default().fg(Color::DarkGray),
            )));

            // Filament slots
            for tray in &unit.trays {
                let color = parse_hex_color(&tray.color);
                let is_active = ams.current_tray == Some(tray.id);
                let marker = if is_active { ">" } else { " " };

                // Show "?%" for unknown remaining (0 often means not reported)
                let remaining_text = if tray.remaining == 0 && !tray.material.is_empty() {
                    " ?%".to_string()
                } else if tray.material.is_empty() {
                    "".to_string()
                } else {
                    format!(" {}%", tray.remaining)
                };

                let remaining_color = match tray.remaining {
                    0 => Color::DarkGray,
                    1..=20 => Color::Yellow,
                    _ => Color::Green,
                };

                lines.push(Line::from(vec![
                    Span::styled(
                        format!("     {}[{}] ", marker, tray.id + 1),
                        if is_active {
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::styled("██", Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(
                        if tray.material.is_empty() {
                            "Empty".to_string()
                        } else {
                            tray.material.clone()
                        },
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(remaining_text, Style::default().fg(remaining_color)),
                ]));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No AMS detected",
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return Color::White;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);

    Color::Rgb(r, g, b)
}
