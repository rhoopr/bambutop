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
