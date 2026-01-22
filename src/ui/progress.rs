//! Print progress panel rendering.
//!
//! Displays the current print job name, progress percentage, layer count,
//! time remaining, and a visual progress bar.

use crate::printer::PrinterState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, LineGauge, Paragraph},
    Frame,
};
use std::borrow::Cow;

/// Maximum display length for job names before truncation
const MAX_JOB_NAME_DISPLAY_LEN: usize = 70;

/// Renders the print progress panel showing job name, progress, layer, time remaining, and progress bar.
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
            Constraint::Length(1), // Progress/Layer/Remaining
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // Spacer
        ])
        .split(inner);

    let print_status = &printer_state.print_status;

    // Job name
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
            truncate_str(&job_display, MAX_JOB_NAME_DISPLAY_LEN),
            Style::new().fg(Color::White),
        ),
    ]);
    frame.render_widget(Paragraph::new(file_line), chunks[0]);

    // Progress, Layer and time remaining
    let time_remaining = format_time(print_status.remaining_time_mins);

    let layer_value: Cow<'static, str> = if print_status.total_layers > 0 {
        Cow::Owned(format!(
            "{}/{}",
            print_status.layer_num, print_status.total_layers
        ))
    } else {
        Cow::Borrowed("-/-")
    };

    let info_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Progress: ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            format!("{}%", print_status.progress),
            Style::new().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::styled("Layer: ", Style::new().fg(Color::DarkGray)),
        Span::styled(layer_value, Style::new().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("Remaining: ", Style::new().fg(Color::DarkGray)),
        Span::styled(time_remaining, Style::new().fg(Color::Cyan)),
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

    let gauge = LineGauge::default()
        .filled_style(Style::new().fg(progress_color))
        .unfilled_style(Style::new().fg(Color::DarkGray))
        .ratio(progress)
        .label("");

    // Add right padding to the progress bar
    let progress_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[3]);
    frame.render_widget(gauge, progress_area[0]);
}

/// Truncates a string to a maximum length, adding "..." if truncated.
/// If the string appears to be a filename with an extension, truncates from the middle
/// to preserve the extension (e.g., "my_very_lo...model.3mf").
/// Returns `Cow::Borrowed` when no truncation is needed to avoid allocation.
fn truncate_str(s: &str, max_len: usize) -> Cow<'_, str> {
    if s.len() <= max_len {
        return Cow::Borrowed(s);
    }

    // Check for file extension (last '.' not at the start)
    if let Some(dot_pos) = s.rfind('.') {
        // Only treat as extension if it's not at the start and extension is reasonable length
        if dot_pos > 0 && s.len() - dot_pos <= 10 {
            let extension = &s[dot_pos..];
            let prefix_budget = max_len.saturating_sub(3).saturating_sub(extension.len());

            if prefix_budget > 0 {
                // Find a valid char boundary for the prefix
                let mut end = prefix_budget.min(s.len());
                while end > 0 && !s.is_char_boundary(end) {
                    end -= 1;
                }

                if end > 0 {
                    return Cow::Owned(format!("{}...{}", &s[..end], extension));
                }
            }
        }
    }

    // Fallback: truncate from end (no extension or not enough space)
    let target_len = max_len.saturating_sub(3);
    let mut end = target_len.min(s.len());

    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    if end == 0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    mod truncate_str_tests {
        use super::*;

        #[test]
        fn returns_borrowed_when_short_enough() {
            let result = truncate_str("short.txt", 20);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "short.txt");
        }

        #[test]
        fn truncates_from_middle_preserving_extension() {
            let result = truncate_str("my_very_long_filename_model.3mf", 25);
            // 25 - 3 (ellipsis) - 4 (.3mf) = 18 chars for prefix
            assert_eq!(result, "my_very_long_filen....3mf");
        }

        #[test]
        fn preserves_longer_extension() {
            let result = truncate_str("my_very_long_filename.gcode", 20);
            // 20 - 3 (ellipsis) - 6 (.gcode) = 11 chars for prefix
            assert_eq!(result, "my_very_lon....gcode");
        }

        #[test]
        fn handles_short_name_with_extension() {
            let result = truncate_str("test.3mf", 20);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "test.3mf");
        }

        #[test]
        fn truncates_from_end_when_no_extension() {
            let result = truncate_str("my_very_long_filename_without_ext", 20);
            assert_eq!(result, "my_very_long_file...");
        }

        #[test]
        fn handles_extension_at_start_as_no_extension() {
            let result = truncate_str(".hidden_very_long_file_name", 15);
            // Dot at start means no extension, truncate from end
            assert_eq!(result, ".hidden_very...");
        }

        #[test]
        fn handles_very_long_extension() {
            // Extensions longer than 10 chars are treated as not extensions
            let result = truncate_str("filename.verylongextension", 20);
            // 20 - 3 = 17 chars from start + ellipsis
            assert_eq!(result, "filename.verylong...");
        }

        #[test]
        fn handles_multiple_dots() {
            let result = truncate_str("my_model.v2.final.3mf", 18);
            // Should preserve ".3mf" as the extension
            // 18 - 3 (ellipsis) - 4 (.3mf) = 11 chars for prefix
            assert_eq!(result, "my_model.v2....3mf");
        }

        #[test]
        fn returns_ellipsis_for_very_short_max() {
            let result = truncate_str("test.3mf", 3);
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
}
