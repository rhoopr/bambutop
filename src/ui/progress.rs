use crate::printer::PrinterState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};
use std::borrow::Cow;

/// Speed level percentages for Bambu printers.
/// Levels: 1=silent, 2=standard, 3=sport, 4=ludicrous
const SPEED_SILENT: u32 = 50;
const SPEED_STANDARD: u32 = 100;
const SPEED_SPORT: u32 = 124;
const SPEED_LUDICROUS: u32 = 166;

/// Renders the print progress panel showing job name, speed, layer, time remaining, and progress bar.
pub fn render(frame: &mut Frame, printer_state: &PrinterState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(
            " Print Progress ",
            Style::new().fg(Color::Blue),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Job name
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Speed/Layer/Remaining
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // Spacer
        ])
        .split(inner);

    let print_status = &printer_state.print_status;

    // Print job name - use smart display_name() that prefers actual project names
    let job_name = print_status.display_name();
    let job_display: Cow<'_, str> = if job_name.is_empty() {
        Cow::Borrowed("No print job")
    } else {
        job_name
    };

    let file_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Job: ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            truncate_str(&job_display, 70).into_owned(),
            Style::new().fg(Color::White),
        ),
    ]);
    frame.render_widget(Paragraph::new(file_line), chunks[0]);

    // Speed, Layer and time remaining
    let speed_percent = speed_level_to_percent(printer_state.speeds.speed_level);
    let time_remaining = format_time(print_status.remaining_time_mins);

    let layer_value = if print_status.total_layers > 0 {
        format!("{}/{}", print_status.layer_num, print_status.total_layers)
    } else {
        "-/-".to_string()
    };

    let info_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Speed: ", Style::new().fg(Color::DarkGray)),
        Span::styled(format!("{}%", speed_percent), Style::new().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled("Layer: ", Style::new().fg(Color::DarkGray)),
        Span::styled(layer_value, Style::new().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled("Remaining: ", Style::new().fg(Color::DarkGray)),
        Span::styled(time_remaining.into_owned(), Style::new().fg(Color::Cyan)),
    ]);
    frame.render_widget(Paragraph::new(info_line), chunks[2]);

    // Progress bar
    let progress = print_status.progress as f64 / 100.0;
    let progress_color = if progress >= 1.0 {
        Color::Green
    } else if progress > 0.0 {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let gauge = Gauge::default()
        .gauge_style(Style::new().fg(progress_color).bg(Color::DarkGray))
        .ratio(progress)
        .label(Span::styled(
            format!("{}%", print_status.progress),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(gauge, chunks[3]);
}

/// Truncates a string to a maximum length, adding "..." if truncated.
/// Returns `Cow::Borrowed` when no truncation is needed to avoid allocation.
/// Handles UTF-8 safely by finding valid character boundaries.
fn truncate_str(s: &str, max_len: usize) -> Cow<'_, str> {
    if s.len() <= max_len {
        return Cow::Borrowed(s);
    }

    // Find a safe truncation point that doesn't split a UTF-8 character
    let target_len = max_len.saturating_sub(3);
    let mut end = target_len.min(s.len());

    // Walk backwards to find a valid UTF-8 character boundary
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    if end == 0 {
        // Edge case: couldn't find a valid boundary, just return ellipsis
        Cow::Borrowed("...")
    } else {
        Cow::Owned(format!("{}...", &s[..end]))
    }
}

/// Formats minutes into a human-readable time string.
/// Returns `Cow::Borrowed` for the zero case to avoid allocation.
fn format_time(mins: u32) -> Cow<'static, str> {
    if mins == 0 {
        Cow::Borrowed("--:--")
    } else {
        let hours = mins / 60;
        let minutes = mins % 60;
        Cow::Owned(if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    mod truncate_str_tests {
        use super::*;

        #[test]
        fn returns_borrowed_when_no_truncation_needed() {
            let result = truncate_str("short", 10);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "short");
        }

        #[test]
        fn truncates_long_strings() {
            let result = truncate_str("this is a very long string", 10);
            assert!(matches!(result, Cow::Owned(_)));
            assert_eq!(result, "this is...");
        }

        #[test]
        fn handles_exact_length() {
            let result = truncate_str("exactly10!", 10);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "exactly10!");
        }

        #[test]
        fn handles_utf8_boundaries() {
            // "héllo world" - 'é' is 2 bytes
            // Bytes: h(1) + é(2) + l(1) + l(1) + o(1) + space(1) + world(5) = 12 bytes
            // With max_len=8 and "..." taking 3, we have 5 bytes for content
            // "héll" is 5 bytes: h(1) + é(2) + l(1) + l(1)
            let result = truncate_str("héllo world", 8);
            assert_eq!(result, "héll...");
        }

        #[test]
        fn handles_emoji() {
            // "Hello 🎉 World!" - 🎉 is 4 bytes at position 6-9
            // With max_len=10, target_len=7 lands inside emoji
            // Should walk back to byte 6 (start of emoji) then to byte 5 (space before)
            // Actually byte 6 IS a char boundary (start of emoji), so we get "Hello "
            let result = truncate_str("Hello 🎉 World!", 10);
            assert_eq!(result, "Hello ...");
        }

        #[test]
        fn handles_max_len_zero() {
            let result = truncate_str("hello", 0);
            assert_eq!(result, "...");
        }

        #[test]
        fn handles_max_len_less_than_ellipsis() {
            // max_len=2, target_len would be negative (saturates to 0)
            let result = truncate_str("hello", 2);
            assert_eq!(result, "...");
        }

        #[test]
        fn handles_empty_string() {
            let result = truncate_str("", 10);
            assert_eq!(result, "");
        }

        #[test]
        fn handles_very_short_max_len() {
            let result = truncate_str("hello", 3);
            assert_eq!(result, "...");
        }
    }

    mod format_time_tests {
        use super::*;

        #[test]
        fn returns_borrowed_for_zero() {
            let result = format_time(0);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "--:--");
        }

        #[test]
        fn formats_minutes_only() {
            let result = format_time(45);
            assert_eq!(result, "45m");
        }

        #[test]
        fn formats_hours_and_minutes() {
            let result = format_time(90);
            assert_eq!(result, "1h 30m");
        }

        #[test]
        fn formats_exact_hours() {
            let result = format_time(120);
            assert_eq!(result, "2h 0m");
        }

        #[test]
        fn formats_large_values() {
            let result = format_time(1500); // 25 hours
            assert_eq!(result, "25h 0m");
        }
    }

    mod speed_level_to_percent_tests {
        use super::*;

        #[test]
        fn converts_known_levels() {
            assert_eq!(speed_level_to_percent(1), 50); // Silent
            assert_eq!(speed_level_to_percent(2), 100); // Standard
            assert_eq!(speed_level_to_percent(3), 124); // Sport
            assert_eq!(speed_level_to_percent(4), 166); // Ludicrous
        }

        #[test]
        fn defaults_to_standard_for_invalid() {
            assert_eq!(speed_level_to_percent(0), 100);
            assert_eq!(speed_level_to_percent(5), 100);
            assert_eq!(speed_level_to_percent(255), 100);
        }
    }
}
