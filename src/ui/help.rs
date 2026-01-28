//! Help overlay component displaying keyboard shortcuts and status indicators.
//!
//! This module renders a centered modal overlay showing all available
//! keyboard shortcuts and status indicator descriptions.

use ratatui::{
    layout::Rect,
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

/// Navigation shortcuts
const NAV_SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        key: "? / h",
        description: "This help",
    },
    Shortcut {
        key: "q / Esc",
        description: "Quit",
    },
    Shortcut {
        key: "u",
        description: "Toggle Celsius/Fahrenheit",
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
        key: "a",
        description: "Aggregate view",
    },
];

/// Printer control shortcuts (require unlock with x)
const CONTROL_SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        key: "x",
        description: "Toggle controls lock",
    },
    Shortcut {
        key: "l",
        description: "Toggle chamber light",
    },
    Shortcut {
        key: "w",
        description: "Toggle work light",
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
];

/// Status indicator definitions
struct Indicator {
    label: &'static str,
    description: &'static str,
}

/// Status indicators displayed in the help overlay
const INDICATORS: &[Indicator] = &[
    Indicator {
        label: "AI",
        description: "Spaghetti detection",
    },
    Indicator {
        label: "REC",
        description: "Camera recording",
    },
    Indicator {
        label: "TL",
        description: "Timelapse enabled",
    },
];

/// Width of the help overlay (including borders)
const OVERLAY_WIDTH: u16 = 42;

/// Renders the help overlay centered on the screen.
pub fn render(frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = Vec::with_capacity(32);

    // Section: Navigation
    lines.push(section_title("Navigation"));
    for s in NAV_SHORTCUTS {
        lines.push(shortcut_line(s));
    }

    lines.push(Line::raw(""));

    // Section: Printer Controls
    lines.push(section_title("Printer Controls"));
    for s in CONTROL_SHORTCUTS {
        lines.push(shortcut_line(s));
    }

    lines.push(Line::raw(""));

    // Section: Status Indicators
    lines.push(section_title("Status Indicators"));
    for i in INDICATORS {
        lines.push(indicator_line(i));
    }

    lines.push(Line::raw(""));

    // Footer
    lines.push(Line::from(vec![
        Span::raw("        "),
        Span::styled(
            "Press any key to close",
            Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ]));

    // borders (2) + content lines
    let height = lines.len() as u16 + 2;
    let popup_area = centered_rect(OVERLAY_WIDTH, height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Cyan))
        .style(Style::new().bg(Color::Black));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner_area);
}

/// Left padding for key/label column to center content within the overlay
const LEFT_PAD: &str = "  ";

/// Renders a section title line with padding to align with content.
fn section_title(title: &str) -> Line<'_> {
    Line::from(vec![
        Span::raw(LEFT_PAD),
        Span::styled(
            title,
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Renders a keyboard shortcut line.
fn shortcut_line(s: &Shortcut) -> Line<'static> {
    Line::from(vec![
        Span::raw(LEFT_PAD),
        Span::styled(format!("{:>10}", s.key), Style::new().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled(s.description, Style::new().fg(Color::White)),
    ])
}

/// Renders a status indicator line.
fn indicator_line(i: &Indicator) -> Line<'static> {
    Line::from(vec![
        Span::raw(LEFT_PAD),
        Span::styled(format!("{:>10}", i.label), Style::new().fg(Color::Green)),
        Span::raw("  "),
        Span::styled(i.description, Style::new().fg(Color::White)),
    ])
}

/// Helper function to create a centered rectangle.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
