//! Printer controls panel rendering.
//!
//! Displays print speed, chamber light, and print job controls (pause/cancel)
//! in a clean two-line layout with keyboard shortcuts.

use crate::printer::PrinterState;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Speed level percentages for Bambu printers.
/// Levels: 1=silent, 2=standard, 3=sport, 4=ludicrous
const SPEED_SILENT: u32 = 50;
const SPEED_STANDARD: u32 = 100;
const SPEED_SPORT: u32 = 124;
const SPEED_LUDICROUS: u32 = 166;

/// Print states where pause/resume/cancel actions are available.
const PRINT_STATE_RUNNING: &str = "RUNNING";
const PRINT_STATE_PAUSED: &str = "PAUSE";

/// Renders the printer controls panel.
///
/// Layout:
/// - Line 1: Speed and Light settings with their hotkeys
/// - Line 2: Print actions (Pause/Cancel) or lock indicator
pub fn render(
    frame: &mut Frame,
    printer_state: &PrinterState,
    controls_locked: bool,
    cancel_pending: bool,
    pause_pending: bool,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(
            " Printer Controls ",
            Style::new().fg(Color::Blue),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Gather state
    let speed_level = printer_state.speeds.speed_level;
    let speed_name = speed_level_to_name(speed_level);
    let speed_percent = speed_level_to_percent(speed_level);

    let light_on = printer_state.lights.chamber_light;
    let gcode_state = printer_state.print_status.gcode_state.as_str();
    let is_paused = gcode_state == PRINT_STATE_PAUSED;
    let has_active_job = gcode_state == PRINT_STATE_RUNNING || is_paused;

    // Colors based on state
    let key_style = if controls_locked {
        Style::new().fg(Color::DarkGray)
    } else {
        Style::new().fg(Color::Yellow)
    };
    let value_style = Style::new().fg(Color::Cyan);
    let label_style = Style::new().fg(Color::DarkGray);
    let light_style = if light_on {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::DarkGray)
    };

    // Line 1: Speed on left, Light on right
    // Calculate widths for right-alignment
    let speed_text = format!("{} ({}%)", speed_name, speed_percent);
    let light_text = if light_on { "ON " } else { "OFF" };
    // Left: "  +/- Speed: {speed}" = 2 + 3 + 8 + speed_text.len()
    let left1_width = 13 + speed_text.len();
    // Right: "l Light: {light}" = 1 + 8 + 3 = 12
    let right1_width = 12;
    let padding1 = (inner.width as usize).saturating_sub(left1_width + right1_width);

    let line1 = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "+",
            key_style.fg(if controls_locked {
                Color::DarkGray
            } else {
                Color::Green
            }),
        ),
        Span::styled("/", label_style),
        Span::styled(
            "-",
            key_style.fg(if controls_locked {
                Color::DarkGray
            } else {
                Color::Red
            }),
        ),
        Span::styled(" Speed: ", label_style),
        Span::styled(speed_text, value_style),
        Span::raw(" ".repeat(padding1)),
        Span::styled("l", key_style),
        Span::styled(" Light: ", label_style),
        Span::styled(light_text, light_style),
    ]);

    // Line 2: Print actions or lock/confirmation indicator
    let line2 = if controls_locked {
        Line::from(vec![
            Span::styled("  \u{1F512} Locked ", Style::new().fg(Color::DarkGray)),
            Span::styled("x", Style::new().fg(Color::Yellow)),
            Span::styled(" to unlock", label_style),
        ])
    } else if cancel_pending && has_active_job {
        // Cancel confirmation
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Cancel print job? ",
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("c", Style::new().fg(Color::Red)),
            Span::styled(" Yes  ", label_style),
            Span::styled("Esc", Style::new().fg(Color::Yellow)),
            Span::styled(" No", label_style),
        ])
    } else if pause_pending && has_active_job {
        // Pause/Resume confirmation
        let action = if is_paused { "Resume" } else { "Pause" };
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{} print job? ", action),
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled("␣", Style::new().fg(Color::Yellow)),
            Span::styled(" Yes  ", label_style),
            Span::styled("Esc", Style::new().fg(Color::Yellow)),
            Span::styled(" No", label_style),
        ])
    } else {
        // Print actions
        let action_key_style = if has_active_job {
            Style::new().fg(Color::Yellow)
        } else {
            label_style
        };
        let cancel_key_style = if has_active_job {
            Style::new().fg(Color::Red)
        } else {
            label_style
        };
        let pause_label = if is_paused {
            "Resume Print"
        } else {
            "Pause Print"
        };

        // Left: "  ␣ {pause_label}  c Cancel Print" = 2 + 1 + 1 + pause_label.len() + 2 + 1 + 13
        let left2_width = 20 + pause_label.len();
        // Right: "x Lock " = 7
        let right2_width = 7;
        let padding2 = (inner.width as usize).saturating_sub(left2_width + right2_width);

        Line::from(vec![
            Span::raw("  "),
            Span::styled("␣", action_key_style),
            Span::styled(format!(" {}  ", pause_label), label_style),
            Span::styled("c", cancel_key_style),
            Span::styled(" Cancel Print", label_style),
            Span::raw(" ".repeat(padding2)),
            Span::styled("x", Style::new().fg(Color::Yellow)),
            Span::styled(" Lock ", label_style),
        ])
    };

    let paragraph = Paragraph::new(vec![line1, line2]);
    frame.render_widget(paragraph, inner);
}

/// Converts Bambu speed level (1-4) to percentage.
fn speed_level_to_percent(level: u8) -> u32 {
    match level {
        1 => SPEED_SILENT,
        2 => SPEED_STANDARD,
        3 => SPEED_SPORT,
        4 => SPEED_LUDICROUS,
        _ => SPEED_STANDARD,
    }
}

/// Converts Bambu speed level (1-4) to its display name.
fn speed_level_to_name(level: u8) -> &'static str {
    match level {
        1 => "Silent",
        2 => "Standard",
        3 => "Sport",
        4 => "Ludicrous",
        _ => "Standard",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod speed_level_to_percent_tests {
        use super::*;

        #[test]
        fn converts_known_speed_levels() {
            assert_eq!(speed_level_to_percent(1), 50); // Silent
            assert_eq!(speed_level_to_percent(2), 100); // Standard
            assert_eq!(speed_level_to_percent(3), 124); // Sport
            assert_eq!(speed_level_to_percent(4), 166); // Ludicrous
        }

        #[test]
        fn defaults_unknown_levels_to_standard() {
            assert_eq!(speed_level_to_percent(0), 100);
            assert_eq!(speed_level_to_percent(5), 100);
            assert_eq!(speed_level_to_percent(255), 100);
        }
    }

    mod speed_level_to_name_tests {
        use super::*;

        #[test]
        fn converts_known_speed_levels() {
            assert_eq!(speed_level_to_name(1), "Silent");
            assert_eq!(speed_level_to_name(2), "Standard");
            assert_eq!(speed_level_to_name(3), "Sport");
            assert_eq!(speed_level_to_name(4), "Ludicrous");
        }

        #[test]
        fn defaults_unknown_levels_to_standard() {
            assert_eq!(speed_level_to_name(0), "Standard");
            assert_eq!(speed_level_to_name(5), "Standard");
            assert_eq!(speed_level_to_name(255), "Standard");
        }
    }
}
