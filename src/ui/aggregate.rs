//! Aggregate view showing all printers in a grid layout.
//!
//! Displays a compact card for each printer with connection status,
//! print progress, WiFi signal, HMS status, and last update time.

use super::common::{
    extract_serial_suffix, format_compact_title, gcode_state_to_status, parse_dbm,
    WIFI_DEFAULT_DBM, WIFI_MEDIUM_THRESHOLD, WIFI_STRONG_THRESHOLD,
};
use super::{STALE_CRITICAL_SECS, STALE_WARNING_SECS};
use crate::app::App;
use crate::printer::PrinterState;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};
use std::borrow::Cow;
use std::time::Instant;

/// Height of each printer card in rows
const CARD_HEIGHT: u16 = 6;

/// Minimum width for a printer card
const MIN_CARD_WIDTH: u16 = 30;

/// Maximum cards per row
const MAX_CARDS_PER_ROW: usize = 4;

/// Renders the aggregate view showing all printers in a grid.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let printer_count = app.printer_count();

    if printer_count == 0 {
        return;
    }

    // Get all printer snapshots
    let snapshots = app.all_printer_snapshots();

    // Calculate grid layout
    let cards_per_row = calculate_cards_per_row(area.width, printer_count);
    let rows_needed = printer_count.div_ceil(cards_per_row);

    // Create row constraints
    let row_constraints: Vec<Constraint> = (0..rows_needed)
        .map(|_| Constraint::Length(CARD_HEIGHT))
        .chain(std::iter::once(Constraint::Min(0))) // Absorb remaining space
        .collect();

    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    // Render each row of cards
    for (row_idx, row_area) in row_areas.iter().take(rows_needed).enumerate() {
        let start_idx = row_idx * cards_per_row;
        let end_idx = (start_idx + cards_per_row).min(printer_count);
        let cards_in_row = end_idx - start_idx;

        // Create column constraints for this row
        let col_constraints: Vec<Constraint> = (0..cards_in_row)
            .map(|_| Constraint::Ratio(1, cards_in_row as u32))
            .collect();

        let card_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);

        for (col_idx, card_area) in card_areas.iter().enumerate() {
            let printer_idx = start_idx + col_idx;
            let is_connected = app.is_printer_connected(printer_idx);
            let is_selected = printer_idx == app.active_printer_index();
            let last_update = app.get_printer_last_update(printer_idx);

            render_printer_card(
                frame,
                &snapshots[printer_idx],
                printer_idx,
                is_connected,
                is_selected,
                last_update,
                *card_area,
            );
        }
    }
}

/// Calculate how many cards fit per row based on available width.
fn calculate_cards_per_row(width: u16, printer_count: usize) -> usize {
    let max_by_width = (width / MIN_CARD_WIDTH) as usize;
    max_by_width.clamp(1, MAX_CARDS_PER_ROW.min(printer_count))
}

/// Gets WiFi signal indicator (bars and color).
fn wifi_indicator(wifi_signal: &str) -> (Color, &'static str) {
    if wifi_signal.is_empty() {
        return (Color::DarkGray, "--");
    }

    let dbm = parse_dbm(wifi_signal).unwrap_or(WIFI_DEFAULT_DBM);

    if dbm > WIFI_STRONG_THRESHOLD {
        (Color::Green, "\u{2582}\u{2584}\u{2586}")
    } else if dbm > WIFI_MEDIUM_THRESHOLD {
        (Color::Yellow, "\u{2582}\u{2584}\u{2591}")
    } else {
        (Color::Red, "\u{2582}\u{2591}\u{2591}")
    }
}

/// Renders a single printer card.
fn render_printer_card(
    frame: &mut Frame,
    state: &PrinterState,
    index: usize,
    is_connected: bool,
    is_selected: bool,
    last_update: Option<Instant>,
    area: Rect,
) {
    // Check for HMS errors
    let has_errors = !state.hms_errors.is_empty();

    // Determine card border color: red if errors, cyan otherwise (gray if disconnected)
    let border_color = if has_errors {
        Color::Red
    } else if !is_connected {
        Color::DarkGray
    } else {
        Color::Cyan
    };

    let border_style = if is_selected {
        Style::new().fg(border_color).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(border_color)
    };

    // Build title: config name > "P1S ...0428" > "Bambu Printer"
    let display_name = if !state.printer_name.is_empty() {
        // Use config name
        Cow::Borrowed(state.printer_name.as_str())
    } else {
        // Use "P1S ...0428" format or fallback
        let model = if state.printer_model.is_empty() {
            "Bambu Printer"
        } else {
            &state.printer_model
        };
        let serial_suffix = extract_serial_suffix(&state.serial_suffix);
        format_compact_title(model, serial_suffix)
    };
    let title = format!(" {}. {} ", index + 1, display_name);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(title, border_style));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // Split inner area: status line, progress/empty, info line
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Status + WiFi
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // HMS + Last update
            Constraint::Min(0),    // Remaining space
        ])
        .split(inner);

    // Row 1: Status indicator + state + WiFi
    render_status_row(frame, state, is_connected, inner_chunks[0]);

    // Row 2: Progress bar (only if printing)
    if is_connected && state.print_status.is_active() {
        render_progress_bar(frame, state, inner_chunks[1]);
    }

    // Row 3: HMS status + Last updated
    render_info_row(frame, state, is_connected, last_update, inner_chunks[2]);
}

/// Renders the status row with connection indicator, state, and WiFi.
fn render_status_row(frame: &mut Frame, state: &PrinterState, is_connected: bool, area: Rect) {
    let (status_icon, status_color) = if is_connected {
        ("\u{25CF}", Color::Green) // Filled circle
    } else {
        ("\u{25CB}", Color::DarkGray) // Empty circle
    };

    let status_text = get_status_text(state, is_connected);
    let (wifi_color, wifi_bars) = wifi_indicator(&state.wifi_signal);

    // Left side: status
    let left = Line::from(vec![
        Span::raw(" "),
        Span::styled(status_icon, Style::new().fg(status_color)),
        Span::raw(" "),
        Span::styled(status_text, Style::new().fg(Color::White)),
    ]);

    // Right side: WiFi
    let right = Line::from(vec![
        Span::styled(wifi_bars, Style::new().fg(wifi_color)),
        Span::raw(" "),
    ]);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(right.width() as u16)])
        .split(area);

    frame.render_widget(Paragraph::new(left), chunks[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), chunks[1]);
}

/// Renders the progress bar.
fn render_progress_bar(frame: &mut Frame, state: &PrinterState, area: Rect) {
    let progress = state.print_status.progress;
    let progress_color = if progress >= 100 {
        Color::Green
    } else {
        Color::Cyan
    };

    let gauge = Gauge::default()
        .gauge_style(Style::new().fg(progress_color).bg(Color::DarkGray))
        .ratio(f64::from(progress.min(100)) / 100.0)
        .label(format!("{}%", progress));

    // Add small margin to progress bar
    let progress_area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );

    frame.render_widget(gauge, progress_area);
}

/// Renders the info row with HMS status and last update time.
fn render_info_row(
    frame: &mut Frame,
    state: &PrinterState,
    is_connected: bool,
    last_update: Option<Instant>,
    area: Rect,
) {
    // Show failure reason or HMS status
    let failure = state.print_status.failure_description();
    let (info_text, info_color): (Cow<'_, str>, Color) = if let Some(ref f) = failure {
        (Cow::Borrowed(f.as_ref()), Color::Red)
    } else if !is_connected || !state.hms_received {
        (Cow::Borrowed("--"), Color::DarkGray)
    } else if state.hms_errors.is_empty() {
        (Cow::Borrowed("OK"), Color::Green)
    } else {
        (Cow::Borrowed("ERR"), Color::Red)
    };

    // Last update time
    let update_text: Cow<'static, str> = match last_update {
        Some(t) => {
            let secs = t.elapsed().as_secs();
            Cow::Owned(format!("{}s", secs))
        }
        None => Cow::Borrowed("--"),
    };

    let update_color = match last_update {
        Some(t) if t.elapsed().as_secs() < STALE_WARNING_SECS => Color::DarkGray,
        Some(t) if t.elapsed().as_secs() < STALE_CRITICAL_SECS => Color::Yellow,
        Some(_) => Color::Red,
        None => Color::DarkGray,
    };

    // Left: HMS status or failure reason
    let left = if failure.is_some() {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(info_text.into_owned(), Style::new().fg(info_color)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" HMS:"),
            Span::styled(info_text.into_owned(), Style::new().fg(info_color)),
        ])
    };

    // Right: Last update
    let right = Line::from(vec![
        Span::styled(update_text, Style::new().fg(update_color)),
        Span::raw(" "),
    ]);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(right.width() as u16)])
        .split(area);

    frame.render_widget(Paragraph::new(left), chunks[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), chunks[1]);
}

/// Get status text for a printer.
fn get_status_text(state: &PrinterState, is_connected: bool) -> &'static str {
    if !is_connected {
        return "Disconnected";
    }

    gcode_state_to_status(&state.print_status.gcode_state)
}
