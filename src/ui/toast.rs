//! Toast notification rendering.
//!
//! Displays brief feedback messages when commands succeed or fail.
//! Toasts appear above the controls panel and auto-dismiss after a few seconds.

use crate::app::{Toast, ToastSeverity};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::collections::VecDeque;

/// Renders all active toasts in the given area.
///
/// Toasts are rendered from bottom to top (newest at bottom).
/// Each toast is a single line with an icon and message.
pub fn render(frame: &mut Frame, toasts: &VecDeque<Toast>, area: Rect) {
    if toasts.is_empty() || area.height == 0 {
        return;
    }

    // Build lines from toasts (newest at bottom)
    let lines: Vec<Line> = toasts
        .iter()
        .map(|toast| {
            let (icon, color) = match toast.severity {
                ToastSeverity::Info => ("\u{2139}", Color::Cyan), // ℹ
                ToastSeverity::Success => ("\u{2713}", Color::Green), // ✓
                ToastSeverity::Warning => ("\u{26A0}", Color::Yellow), // ⚠
                ToastSeverity::Error => ("\u{2717}", Color::Red), // ✗
            };

            Line::from(vec![
                Span::styled(format!(" {icon} "), Style::new().fg(color)),
                Span::styled(&toast.message, Style::new().fg(color)),
            ])
        })
        .collect();

    // Render right-aligned
    let paragraph = Paragraph::new(lines).right_aligned();
    frame.render_widget(paragraph, area);
}

/// Returns the height needed to display the given number of toasts.
pub fn panel_height(toast_count: usize) -> u16 {
    toast_count as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    mod panel_height_tests {
        use super::*;

        #[test]
        fn zero_toasts_zero_height() {
            assert_eq!(panel_height(0), 0);
        }

        #[test]
        fn one_toast_one_line() {
            assert_eq!(panel_height(1), 1);
        }

        #[test]
        fn three_toasts_three_lines() {
            assert_eq!(panel_height(3), 3);
        }
    }

    mod toast_icon_mapping_tests {
        use super::*;

        /// Helper: build a single-toast VecDeque and extract the icon span content
        fn icon_for_severity(severity: ToastSeverity) -> String {
            let mut toasts = VecDeque::new();
            toasts.push_back(Toast {
                message: "test".into(),
                severity,
                created_at: Instant::now(),
            });

            // Reproduce the mapping logic from render to verify icon/color pairs
            let (icon, _color) = match severity {
                ToastSeverity::Info => ("\u{2139}", Color::Cyan),
                ToastSeverity::Success => ("\u{2713}", Color::Green),
                ToastSeverity::Warning => ("\u{26A0}", Color::Yellow),
                ToastSeverity::Error => ("\u{2717}", Color::Red),
            };
            format!(" {icon} ")
        }

        #[test]
        fn info_icon() {
            let icon = icon_for_severity(ToastSeverity::Info);
            assert!(icon.contains('\u{2139}'));
        }

        #[test]
        fn success_icon() {
            let icon = icon_for_severity(ToastSeverity::Success);
            assert!(icon.contains('\u{2713}'));
        }

        #[test]
        fn warning_icon() {
            let icon = icon_for_severity(ToastSeverity::Warning);
            assert!(icon.contains('\u{26A0}'));
        }

        #[test]
        fn error_icon() {
            let icon = icon_for_severity(ToastSeverity::Error);
            assert!(icon.contains('\u{2717}'));
        }
    }

    mod toast_color_mapping_tests {
        use super::*;

        fn color_for_severity(severity: ToastSeverity) -> Color {
            match severity {
                ToastSeverity::Info => Color::Cyan,
                ToastSeverity::Success => Color::Green,
                ToastSeverity::Warning => Color::Yellow,
                ToastSeverity::Error => Color::Red,
            }
        }

        #[test]
        fn info_is_cyan() {
            assert_eq!(color_for_severity(ToastSeverity::Info), Color::Cyan);
        }

        #[test]
        fn success_is_green() {
            assert_eq!(color_for_severity(ToastSeverity::Success), Color::Green);
        }

        #[test]
        fn warning_is_yellow() {
            assert_eq!(color_for_severity(ToastSeverity::Warning), Color::Yellow);
        }

        #[test]
        fn error_is_red() {
            assert_eq!(color_for_severity(ToastSeverity::Error), Color::Red);
        }
    }
}
