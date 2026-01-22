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
///
/// # Arguments
/// * `frame` - The ratatui frame to render to
/// * `printer_state` - Current printer state snapshot
/// * `timezone_offset_secs` - Local timezone offset from UTC in seconds (for ETA clock display)
/// * `area` - The rectangular area to render within
pub fn render(
    frame: &mut Frame,
    printer_state: &PrinterState,
    timezone_offset_secs: i32,
    area: Rect,
) {
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
            Constraint::Length(1), // Phase (or spacer if no phase)
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

    // Print phase (only shown when job is active)
    if let Some(phase) = print_status.print_phase(&printer_state.temperatures) {
        let phase_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("Phase: ", Style::new().fg(Color::DarkGray)),
            Span::styled(phase, Style::new().fg(Color::Gray)),
        ]);
        frame.render_widget(Paragraph::new(phase_line), chunks[1]);
    }

    // Progress, Layer and time remaining
    let time_remaining = format_time(print_status.remaining_time_mins);
    let eta_clock = format_eta_clock(print_status.remaining_time_mins, timezone_offset_secs);

    // Build remaining time display with ETA clock if available
    let remaining_display: Cow<'_, str> = if print_status.remaining_time_mins == 0 {
        time_remaining
    } else {
        Cow::Owned(format!("{} (ETA {})", time_remaining, eta_clock))
    };

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
        Span::styled(remaining_display, Style::new().fg(Color::Cyan)),
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

/// Number of seconds in an hour
const SECS_PER_HOUR: i64 = 3600;
/// Number of seconds in a minute
const SECS_PER_MINUTE: i64 = 60;
/// Number of seconds in a day (for wrapping calculations)
const SECS_PER_DAY: i64 = 86400;

/// Formats the estimated completion time as a 12-hour clock string (e.g., "2:45 PM").
///
/// # Arguments
/// * `remaining_mins` - Minutes remaining until completion
/// * `timezone_offset_secs` - Local timezone offset from UTC in seconds
///
/// # Returns
/// A formatted string like "2:45 PM" or "--:--" if remaining time is 0.
fn format_eta_clock(remaining_mins: u32, timezone_offset_secs: i32) -> Cow<'static, str> {
    if remaining_mins == 0 {
        return Cow::Borrowed("--:--");
    }

    // Get current UTC timestamp
    let now_utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Calculate ETA in UTC seconds
    let remaining_secs = i64::from(remaining_mins) * SECS_PER_MINUTE;
    let eta_utc = now_utc + remaining_secs;

    // Convert to local time
    let eta_local = eta_utc + i64::from(timezone_offset_secs);

    // Extract time of day (seconds since midnight, handling negative values)
    let secs_since_midnight = eta_local.rem_euclid(SECS_PER_DAY);

    let hour_24 = (secs_since_midnight / SECS_PER_HOUR) as u32;
    let minute = ((secs_since_midnight % SECS_PER_HOUR) / SECS_PER_MINUTE) as u32;

    // Convert to 12-hour format
    let (hour_12, am_pm) = match hour_24 {
        0 => (12, "AM"),
        1..=11 => (hour_24, "AM"),
        12 => (12, "PM"),
        _ => (hour_24 - 12, "PM"),
    };

    Cow::Owned(format!("{}:{:02} {}", hour_12, minute, am_pm))
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

    mod format_eta_clock_tests {
        use super::*;

        #[test]
        fn returns_borrowed_for_zero_remaining() {
            let result = format_eta_clock(0, 0);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "--:--");
        }

        #[test]
        fn formats_12_hour_with_am_pm() {
            // We can't test exact times since they depend on current time,
            // but we can verify the format is correct (contains AM or PM)
            let result = format_eta_clock(60, 0);
            assert!(
                result.ends_with("AM") || result.ends_with("PM"),
                "Expected AM/PM suffix, got: {}",
                result
            );
        }

        #[test]
        fn format_contains_colon() {
            let result = format_eta_clock(30, 0);
            assert!(
                result.contains(':'),
                "Expected colon in time format, got: {}",
                result
            );
        }

        #[test]
        fn handles_positive_timezone_offset() {
            // UTC+1 (3600 seconds)
            let result = format_eta_clock(60, 3600);
            assert!(
                result.ends_with("AM") || result.ends_with("PM"),
                "Expected valid time format with positive offset, got: {}",
                result
            );
        }

        #[test]
        fn handles_negative_timezone_offset() {
            // UTC-5 (-18000 seconds)
            let result = format_eta_clock(60, -18000);
            assert!(
                result.ends_with("AM") || result.ends_with("PM"),
                "Expected valid time format with negative offset, got: {}",
                result
            );
        }

        #[test]
        fn handles_very_long_estimates() {
            // 48 hours (2880 minutes) - should still produce valid time
            let result = format_eta_clock(2880, 0);
            assert!(
                result.ends_with("AM") || result.ends_with("PM"),
                "Expected valid time format for long estimate, got: {}",
                result
            );
        }

        #[test]
        fn hour_is_in_valid_12_hour_range() {
            // Test that the hour is between 1-12 (not 0 or 13+)
            let result = format_eta_clock(60, 0);
            // Parse the hour from the result (format is "H:MM AM" or "HH:MM AM")
            let hour_str: String = result.chars().take_while(|c| *c != ':').collect();
            let hour: u32 = hour_str.parse().expect("Failed to parse hour");
            assert!(
                (1..=12).contains(&hour),
                "Hour {} is not in valid 12-hour range (1-12)",
                hour
            );
        }
    }
}
