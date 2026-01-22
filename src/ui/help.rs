//! Help overlay component displaying keyboard shortcuts.
//!
//! This module renders a centered modal overlay showing all available
//! keyboard shortcuts for the application.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Keyboard shortcut definition
struct Shortcut {
    key: &'static str,
    description: &'static str,
}

/// All keyboard shortcuts displayed in the help overlay
const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        key: "q / Esc",
        description: "Quit",
    },
    Shortcut {
        key: "x",
        description: "Toggle controls lock",
    },
    Shortcut {
        key: "u",
        description: "Toggle Celsius/Fahrenheit",
    },
    Shortcut {
        key: "l",
        description: "Toggle chamber light",
    },
    Shortcut {
        key: "+ / -",
        description: "Adjust print speed",
    },
    Shortcut {
        key: "Space",
        description: "Pause/Resume print",
    },
    Shortcut {
        key: "c",
        description: "Cancel print",
    },
    Shortcut {
        key: "Tab",
        description: "Next printer",
    },
    Shortcut {
        key: "Shift+Tab",
        description: "Previous printer",
    },
    Shortcut {
        key: "1-9",
        description: "Select printer",
    },
    Shortcut {
        key: "? / h",
        description: "This help",
    },
];

/// Width of the help overlay (including borders)
const OVERLAY_WIDTH: u16 = 40;

/// Calculates the height needed for the help overlay
fn overlay_height() -> u16 {
    // Title (1) + blank line (1) + shortcuts + blank line (1) + footer (1) + borders (2)
    (SHORTCUTS.len() as u16) + 6
}

/// Renders the help overlay centered on the screen.
pub fn render(frame: &mut Frame, area: Rect) {
    let height = overlay_height();

    // Center the overlay
    let popup_area = centered_rect(OVERLAY_WIDTH, height, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Create the block with borders
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Cyan))
        .style(Style::new().bg(Color::Black));

    // Calculate inner area for content
    let inner_area = block.inner(popup_area);

    // Render the block
    frame.render_widget(block, popup_area);

    // Create layout for content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Blank line
            Constraint::Min(1),    // Shortcuts
            Constraint::Length(1), // Blank line
            Constraint::Length(1), // Footer
        ])
        .split(inner_area);

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        "Keyboard Shortcuts",
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Shortcuts list
    let shortcut_lines: Vec<Line> = SHORTCUTS
        .iter()
        .map(|s| {
            Line::from(vec![
                Span::styled(format!("{:>12}", s.key), Style::new().fg(Color::Yellow)),
                Span::raw("  "),
                Span::styled(s.description, Style::new().fg(Color::White)),
            ])
        })
        .collect();

    let shortcuts = Paragraph::new(shortcut_lines).alignment(Alignment::Center);
    frame.render_widget(shortcuts, chunks[2]);

    // Footer
    let footer = Paragraph::new(Line::from(vec![Span::styled(
        "Press any key to close",
        Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[4]);
}

/// Helper function to create a centered rectangle.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
