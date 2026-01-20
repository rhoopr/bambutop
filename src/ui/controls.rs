use crate::printer::PrinterState;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
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

/// Renders the printer controls panel showing speed settings.
pub fn render(frame: &mut Frame, printer_state: &PrinterState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(
            " Printer Controls ",
            Style::new().fg(Color::Blue),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let speed_percent = speed_level_to_percent(printer_state.speeds.speed_level);

    let speed_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Speed: ", Style::new().fg(Color::DarkGray)),
        Span::styled(format!("{}%", speed_percent), Style::new().fg(Color::Cyan)),
    ]);

    let lines = vec![speed_line, Line::from("")];
    frame.render_widget(Paragraph::new(lines), inner);
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
