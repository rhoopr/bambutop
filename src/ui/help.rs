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
    Shortcut {
        key: "r",
        description: "Refresh all printers",
    },
    Shortcut {
        key: "e",
        description: "Toggle error notifications",
    },
    Shortcut {
        key: "n",
        description: "Toggle completion notifications",
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
        label: "FLI",
        description: "First layer inspection",
    },
    Indicator {
        label: "REC",
        description: "Camera recording",
    },
    Indicator {
        label: "TL",
        description: "Timelapse enabled",
    },
    Indicator {
        label: "\u{25CF} green",
        description: "Detect only",
    },
    Indicator {
        label: "\u{25CF} yellow",
        description: "Detect + halt print",
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

#[cfg(test)]
mod tests {
    use super::*;

    mod centered_rect_tests {
        use super::*;

        #[test]
        fn centers_horizontally_and_vertically() {
            let area = Rect::new(0, 0, 100, 50);
            let result = centered_rect(40, 20, area);
            assert_eq!(result.x, 30);
            assert_eq!(result.y, 15);
            assert_eq!(result.width, 40);
            assert_eq!(result.height, 20);
        }

        #[test]
        fn clamps_when_overlay_wider_than_area() {
            let area = Rect::new(0, 0, 30, 50);
            let result = centered_rect(40, 20, area);
            assert_eq!(result.x, 0);
            assert_eq!(result.width, 30);
        }

        #[test]
        fn clamps_when_overlay_taller_than_area() {
            let area = Rect::new(0, 0, 100, 10);
            let result = centered_rect(40, 20, area);
            assert_eq!(result.y, 0);
            assert_eq!(result.height, 10);
        }

        #[test]
        fn respects_area_offset() {
            let area = Rect::new(10, 5, 100, 50);
            let result = centered_rect(40, 20, area);
            assert_eq!(result.x, 10 + 30);
            assert_eq!(result.y, 5 + 15);
        }

        #[test]
        fn zero_size_area() {
            let area = Rect::new(0, 0, 0, 0);
            let result = centered_rect(40, 20, area);
            assert_eq!(result.width, 0);
            assert_eq!(result.height, 0);
        }

        #[test]
        fn exact_fit() {
            let area = Rect::new(0, 0, 42, 30);
            let result = centered_rect(42, 30, area);
            assert_eq!(result.x, 0);
            assert_eq!(result.y, 0);
            assert_eq!(result.width, 42);
            assert_eq!(result.height, 30);
        }
    }

    mod shortcut_data_tests {
        use super::*;

        #[test]
        fn nav_shortcuts_are_not_empty() {
            assert!(!NAV_SHORTCUTS.is_empty());
        }

        #[test]
        fn control_shortcuts_are_not_empty() {
            assert!(!CONTROL_SHORTCUTS.is_empty());
        }

        #[test]
        fn indicators_are_not_empty() {
            assert!(!INDICATORS.is_empty());
        }

        #[test]
        fn all_nav_shortcuts_have_content() {
            for s in NAV_SHORTCUTS {
                assert!(!s.key.is_empty(), "Shortcut key should not be empty");
                assert!(
                    !s.description.is_empty(),
                    "Shortcut description should not be empty"
                );
            }
        }

        #[test]
        fn all_control_shortcuts_have_content() {
            for s in CONTROL_SHORTCUTS {
                assert!(!s.key.is_empty());
                assert!(!s.description.is_empty());
            }
        }

        #[test]
        fn all_indicators_have_content() {
            for i in INDICATORS {
                assert!(!i.label.is_empty());
                assert!(!i.description.is_empty());
            }
        }

        #[test]
        fn overlay_width_is_reasonable() {
            let w = OVERLAY_WIDTH;
            assert!(w > 20, "overlay too narrow: {w}");
            assert!(w < 100, "overlay too wide: {w}");
        }
    }

    mod line_builder_tests {
        use super::*;

        #[test]
        fn section_title_has_bold_cyan_text() {
            let line = section_title("Test Section");
            let spans: Vec<_> = line.spans.iter().collect();
            // First span is left padding
            assert_eq!(spans[0].content, LEFT_PAD);
            // Second span is the title
            assert_eq!(spans[1].content, "Test Section");
            assert_eq!(spans[1].style.fg, Some(Color::Cyan));
            assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
        }

        #[test]
        fn shortcut_line_has_yellow_key_and_white_desc() {
            let s = Shortcut {
                key: "q",
                description: "Quit",
            };
            let line = shortcut_line(&s);
            let spans: Vec<_> = line.spans.iter().collect();
            // Pad, key, space, description
            assert_eq!(spans[1].style.fg, Some(Color::Yellow));
            assert!(spans[1].content.contains('q'));
            assert_eq!(spans[3].content, "Quit");
            assert_eq!(spans[3].style.fg, Some(Color::White));
        }

        #[test]
        fn indicator_line_has_green_label() {
            let i = Indicator {
                label: "AI",
                description: "Spaghetti detection",
            };
            let line = indicator_line(&i);
            let spans: Vec<_> = line.spans.iter().collect();
            assert_eq!(spans[1].style.fg, Some(Color::Green));
            assert!(spans[1].content.contains("AI"));
            assert_eq!(spans[3].content, "Spaghetti detection");
        }
    }
}
