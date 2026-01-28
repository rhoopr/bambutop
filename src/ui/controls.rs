//! Printer controls panel rendering.
//!
//! Displays print speed, chamber light, and print job controls (pause/cancel)
//! in a clean two-line layout with keyboard shortcuts.

use crate::printer::{speed_level_to_name, speed_level_to_percent, PrinterState};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

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
    // Right side width depends on whether work light is available
    let has_work_light = printer_state.has_work_light();
    // "l Light: {light}" = 12, optionally + "  w Work: {work}" = 14
    let right1_width = if has_work_light { 26 } else { 12 };
    let padding1 = (inner.width as usize).saturating_sub(left1_width + right1_width);

    let mut line1_spans = vec![
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
    ];
    if has_work_light {
        line1_spans.push(Span::raw("  "));
        line1_spans.push(Span::styled("w", key_style));
        line1_spans.push(Span::styled(" Work: ", label_style));
        line1_spans.push(Span::styled(
            if printer_state.lights.work_light {
                "ON "
            } else {
                "OFF"
            },
            if printer_state.lights.work_light {
                Style::new().fg(Color::Yellow)
            } else {
                Style::new().fg(Color::DarkGray)
            },
        ));
    }
    let line1 = Line::from(line1_spans);

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
