mod app;
mod config;
mod demo;
mod mqtt;
mod printer;
mod ui;
mod wizard;

use anyhow::{Context, Result};
use app::{App, ViewMode};
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;

/// Maximum number of printers that can be navigated via number keys (1-9)
const MAX_PRINTER_HOTKEYS: usize = 9;
use mqtt::MqttClient;
use printer::{speed_level_to_name, speed_level_to_percent, GcodeState};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Flag to track whether terminal is in raw mode (for panic hook)
static TERMINAL_IN_RAW_MODE: AtomicBool = AtomicBool::new(false);

/// UI refresh rate - how often to poll for events and redraw
const UI_TICK_RATE: Duration = Duration::from_millis(250);

/// Interval between periodic full status requests to all printers.
/// Acts as a safety net: if individual MQTT pushes are silently lost
/// (QoS 0 offers no delivery guarantee), this ensures state is refreshed.
const STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// MQTT event channel capacity per printer
const CHANNEL_CAPACITY_PER_PRINTER: usize = 100;

#[derive(Parser, Debug)]
#[command(name = "bambutop")]
#[command(about = "Terminal-based status monitor for Bambu Labs printers")]
#[command(version)]
struct Args {
    /// Printer IP address (overrides config file)
    #[arg(short, long)]
    ip: Option<String>,

    /// Printer serial number (overrides config file)
    #[arg(short, long)]
    serial: Option<String>,

    /// Printer access code (overrides config file)
    #[arg(short, long)]
    access_code: Option<String>,

    /// Delete config and run setup wizard
    #[arg(long)]
    reset: bool,

    /// Launch with demo data (no printer connection needed)
    #[arg(long)]
    demo: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --demo flag: launch with pre-populated data, no MQTT
    if args.demo {
        return run_demo().await;
    }

    // Handle --reset flag
    if args.reset {
        let config_path =
            config::Config::config_path().context("failed to determine config path")?;
        if config_path.exists() {
            std::fs::remove_file(&config_path).context("failed to remove config file")?;
        }
    }

    // Build config from CLI args, config file, or wizard
    let config = if let (Some(ip), Some(serial), Some(access_code)) = (
        args.ip.as_ref(),
        args.serial.as_ref(),
        args.access_code.as_ref(),
    ) {
        // All CLI args provided - use them and save to config
        let config = config::Config {
            printer: config::PrinterConfig {
                name: None,
                ip: ip.clone(),
                serial: serial.clone(),
                access_code: access_code.clone(),
                port: config::DEFAULT_MQTT_PORT,
            },
            extra_printers: vec![],
        };
        config.save().context("failed to save config")?;
        config
    } else {
        // Load from file or run wizard
        let mut config = match config::Config::load()? {
            Some(config) => config,
            None => wizard::run_setup_wizard()?,
        };

        // Override with any provided CLI args
        if let Some(ip) = args.ip {
            config.printer.ip = ip;
        }
        if let Some(serial) = args.serial {
            config.printer.serial = serial;
        }
        if let Some(access_code) = args.access_code {
            config.printer.access_code = access_code;
        }

        config
    };

    // Install panic hook to restore terminal state on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if TERMINAL_IN_RAW_MODE.load(Ordering::SeqCst) {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
            let _ = stdout.flush();
        }
        original_hook(panic_info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    TERMINAL_IN_RAW_MODE.store(true, Ordering::SeqCst);
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the main application logic, capturing the result
    let result = run_main(&mut terminal, &config).await;

    // Always restore terminal, regardless of success or failure
    TERMINAL_IN_RAW_MODE.store(false, Ordering::SeqCst);
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    if let Err(err) = result {
        eprintln!("Error: {err}");
    }

    Ok(())
}

/// Runs the main application logic after terminal setup.
///
/// This is separated from `main()` so that terminal restoration always happens
/// in the caller, even if this function returns an error.
async fn run_main(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: &config::Config,
) -> Result<()> {
    // Get all configured printers
    let all_printers = config.all_printers();
    let printer_count = all_printers.len();

    // Create shared event channel for all printers
    let (event_tx, mut mqtt_rx) =
        tokio::sync::mpsc::channel(CHANNEL_CAPACITY_PER_PRINTER * printer_count);

    // Connect to all printers concurrently
    let connect_futures: Vec<_> = all_printers
        .iter()
        .enumerate()
        .map(|(index, config)| MqttClient::connect(config, index, Some(event_tx.clone())))
        .collect();

    // Drop the original sender so the channel closes when all clients disconnect
    drop(event_tx);

    let results = futures::future::join_all(connect_futures).await;

    let mut mqtt_clients = Vec::with_capacity(printer_count);
    let mut printer_states = Vec::with_capacity(printer_count);
    for result in results {
        let (client, state, _) = result?;
        mqtt_clients.push(client);
        printer_states.push(state);
    }

    // Create app with all printer states
    let mut app = App::new_multi(printer_states);

    // Request initial state and version info from all printers
    for client in &mqtt_clients {
        client.request_full_status().await?;
        client.request_version_info().await?;
    }

    // Main loop
    let result = run_app(
        terminal,
        &mut app,
        &mut mqtt_rx,
        UI_TICK_RATE,
        &mqtt_clients,
    )
    .await;

    // Gracefully disconnect from all MQTT brokers
    for client in &mqtt_clients {
        client.disconnect().await;
    }

    result
}

/// Runs the TUI in demo mode with pre-populated printer data.
async fn run_demo() -> Result<()> {
    // Install panic hook to restore terminal state on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if TERMINAL_IN_RAW_MODE.load(Ordering::SeqCst) {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
            let _ = stdout.flush();
        }
        original_hook(panic_info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    TERMINAL_IN_RAW_MODE.store(true, Ordering::SeqCst);
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let printer_states = demo::create_demo_printers();
    let mut app = App::new_multi(printer_states);

    // Mark all printers as connected with recent updates
    for i in 0..app.printer_count() {
        app.set_printer_connected(i, true);
        app.set_printer_last_update(i, Some(std::time::Instant::now()));
    }

    // Create a dummy channel (sender dropped immediately, try_recv returns empty)
    let (tx, mut mqtt_rx) = tokio::sync::mpsc::channel(1);
    drop(tx);

    let result = run_app(&mut terminal, &mut app, &mut mqtt_rx, UI_TICK_RATE, &[]).await;

    // Always restore terminal, regardless of success or failure
    TERMINAL_IN_RAW_MODE.store(false, Ordering::SeqCst);
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    if let Err(err) = result {
        eprintln!("Error: {err}");
    }

    Ok(())
}

/// Minimum speed level (Silent)
const SPEED_LEVEL_MIN: u8 = 1;
/// Maximum speed level (Ludicrous)
const SPEED_LEVEL_MAX: u8 = 4;

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    mqtt_rx: &mut tokio::sync::mpsc::Receiver<mqtt::MqttEvent>,
    tick_rate: Duration,
    mqtt_clients: &[MqttClient],
) -> Result<()> {
    let mut last_status_refresh = Instant::now();
    let mut event_stream = EventStream::new();
    let mut tick_interval = tokio::time::interval(tick_rate);

    loop {
        // Expire old toasts and refresh dirty printer snapshots before rendering
        app.expire_toasts();
        app.refresh_snapshots();

        terminal.draw(|f| ui::render(f, app))?;

        // Wait for next event: MQTT message, keyboard input, or tick
        tokio::select! {
            Some(mqtt_event) = mqtt_rx.recv() => {
                app.handle_mqtt_event(mqtt_event);
                // Drain any additional pending events
                while let Ok(event) = mqtt_rx.try_recv() {
                    app.handle_mqtt_event(event);
                }
            }
            Some(Ok(event)) = event_stream.next() => {
                if let Event::Key(key) = event {
                    if key.kind == KeyEventKind::Press {
                        // If help overlay is shown, any key closes it
                        if app.show_help {
                            app.show_help = false;
                            continue;
                        }

                        match key.code {
                        // Help overlay toggle
                        KeyCode::Char('?') | KeyCode::Char('h') => {
                            app.show_help = true;
                        }
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Esc => {
                            // Esc aborts confirmations, or quits if none pending
                            if app.cancel_pending {
                                app.cancel_pending = false;
                            } else if app.pause_pending {
                                app.pause_pending = false;
                            } else {
                                app.should_quit = true;
                            }
                        }
                        KeyCode::Char('x') => {
                            app.controls_locked = !app.controls_locked;
                            // Clear confirmations when locking controls
                            if app.controls_locked {
                                app.cancel_pending = false;
                                app.pause_pending = false;
                                app.toast_info("Controls locked");
                            } else {
                                app.toast_info("Controls unlocked");
                            }
                        }
                        KeyCode::Char('u') => {
                            app.use_celsius = !app.use_celsius;
                            let unit = if app.use_celsius {
                                "Celsius"
                            } else {
                                "Fahrenheit"
                            };
                            app.toast_info(format!("Temperature: {unit}"));
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char(']') => {
                            if !app.controls_locked {
                                if mqtt_clients.is_empty() {
                                    app.toast_info("Demo mode");
                                } else {
                                    let current = app
                                        .printer_state
                                        .lock()
                                        .expect("state lock poisoned")
                                        .speeds
                                        .speed_level;
                                    if current < SPEED_LEVEL_MAX {
                                        let new_level = current + 1;
                                        let speed_display = format!(
                                            "{} ({}%)",
                                            speed_level_to_name(new_level),
                                            speed_level_to_percent(new_level)
                                        );
                                        let client = &mqtt_clients[app.active_printer_index()];
                                        if let Err(e) = client.set_speed_level(new_level).await {
                                            app.toast_error(format!("Speed change failed: {e}"));
                                        } else {
                                            app.toast_success(format!("Speed: {speed_display}"));
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('-') | KeyCode::Char('[') => {
                            if !app.controls_locked {
                                if mqtt_clients.is_empty() {
                                    app.toast_info("Demo mode");
                                } else {
                                    let current = app
                                        .printer_state
                                        .lock()
                                        .expect("state lock poisoned")
                                        .speeds
                                        .speed_level;
                                    if current > SPEED_LEVEL_MIN {
                                        let new_level = current - 1;
                                        let speed_display = format!(
                                            "{} ({}%)",
                                            speed_level_to_name(new_level),
                                            speed_level_to_percent(new_level)
                                        );
                                        let client = &mqtt_clients[app.active_printer_index()];
                                        if let Err(e) = client.set_speed_level(new_level).await {
                                            app.toast_error(format!("Speed change failed: {e}"));
                                        } else {
                                            app.toast_success(format!("Speed: {speed_display}"));
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('l') => {
                            if !app.controls_locked {
                                if mqtt_clients.is_empty() {
                                    app.toast_info("Demo mode");
                                } else {
                                    let current = app
                                        .printer_state
                                        .lock()
                                        .expect("state lock poisoned")
                                        .lights
                                        .chamber_light;
                                    let new_state = !current;
                                    let status = if new_state { "ON" } else { "OFF" };
                                    let client = &mqtt_clients[app.active_printer_index()];
                                    if let Err(e) = client.set_chamber_light(new_state).await {
                                        app.toast_error(format!("Light toggle failed: {e}"));
                                    } else {
                                        app.toast_success(format!("Light: {status}"));
                                    }
                                }
                            }
                        }
                        KeyCode::Char('w') => {
                            if !app.controls_locked {
                                if mqtt_clients.is_empty() {
                                    app.toast_info("Demo mode");
                                } else {
                                    let current = app
                                        .printer_state
                                        .lock()
                                        .expect("state lock poisoned")
                                        .lights
                                        .work_light;
                                    let new_state = !current;
                                    let status = if new_state { "ON" } else { "OFF" };
                                    let client = &mqtt_clients[app.active_printer_index()];
                                    if let Err(e) = client.set_work_light(new_state).await {
                                        app.toast_error(format!("Work light toggle failed: {e}"));
                                    } else {
                                        app.toast_success(format!("Work light: {status}"));
                                    }
                                }
                            }
                        }
                        KeyCode::Char(' ') => {
                            if !app.controls_locked {
                                if mqtt_clients.is_empty() {
                                    app.toast_info("Demo mode");
                                } else {
                                    let (is_running, is_paused) = {
                                        let state =
                                            app.printer_state.lock().expect("state lock poisoned");
                                        let gcode = state.print_status.gcode_state;
                                        (gcode == GcodeState::Running, gcode == GcodeState::Pause)
                                    };
                                    let has_active_job = is_running || is_paused;
                                    if has_active_job {
                                        if app.pause_pending {
                                            let client = &mqtt_clients[app.active_printer_index()];
                                            if is_running {
                                                match client.pause_print().await {
                                                    Ok(_) => app.toast_warning("Print paused"),
                                                    Err(e) => app.toast_error(format!(
                                                        "Failed to pause: {e}"
                                                    )),
                                                }
                                            } else if is_paused {
                                                match client.resume_print().await {
                                                    Ok(_) => app.toast_success("Print resumed"),
                                                    Err(e) => app.toast_error(format!(
                                                        "Failed to resume: {e}"
                                                    )),
                                                }
                                            }
                                            app.pause_pending = false;
                                        } else {
                                            app.pause_pending = true;
                                            app.cancel_pending = false;
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('c') => {
                            if !app.controls_locked {
                                if mqtt_clients.is_empty() {
                                    app.toast_info("Demo mode");
                                } else {
                                    let has_active_job = {
                                        let state =
                                            app.printer_state.lock().expect("state lock poisoned");
                                        matches!(
                                            state.print_status.gcode_state,
                                            GcodeState::Running | GcodeState::Pause
                                        )
                                    };
                                    if has_active_job {
                                        if app.cancel_pending {
                                            let client = &mqtt_clients[app.active_printer_index()];
                                            match client.stop_print().await {
                                                Ok(_) => app.toast_error("Print cancelled"),
                                                Err(e) => app
                                                    .toast_error(format!("Failed to cancel: {e}")),
                                            }
                                            app.cancel_pending = false;
                                        } else {
                                            app.cancel_pending = true;
                                            app.pause_pending = false;
                                        }
                                    }
                                }
                            }
                        }
                        // Force refresh: re-subscribe and request full status from all printers
                        KeyCode::Char('r') => {
                            if mqtt_clients.is_empty() {
                                app.toast_info("Demo mode");
                            } else {
                                let mut ok = 0usize;
                                for client in mqtt_clients.iter() {
                                    if client.refresh().await.is_ok() {
                                        ok += 1;
                                    }
                                }
                                if ok == mqtt_clients.len() {
                                    app.toast_success("Refreshed all printers");
                                } else {
                                    app.toast_warning(format!(
                                        "Refreshed {}/{}",
                                        ok,
                                        mqtt_clients.len()
                                    ));
                                }
                            }
                        }
                        // Return to aggregate view
                        KeyCode::Char('a') => {
                            if app.printer_count() > 1 && app.view_mode == ViewMode::Single {
                                app.view_mode = ViewMode::Aggregate;
                                app.toast_info("Overview");
                            }
                        }
                        // Multi-printer navigation: Tab cycles to next printer
                        KeyCode::Tab => {
                            let printer_count = app.printer_count();
                            if printer_count > 1 {
                                match app.view_mode {
                                    ViewMode::Aggregate => {
                                        // Switch to single view with first printer
                                        app.view_mode = ViewMode::Single;
                                        app.set_active_printer(0);
                                        app.toast_info(format!("Printer {}/{}", 1, printer_count));
                                    }
                                    ViewMode::Single => {
                                        let current = app.active_printer_index();
                                        if current + 1 >= printer_count {
                                            // At last printer, go back to aggregate
                                            app.view_mode = ViewMode::Aggregate;
                                            app.toast_info("Overview");
                                        } else {
                                            // Go to next printer
                                            let next = current + 1;
                                            app.set_active_printer(next);
                                            app.toast_info(format!(
                                                "Printer {}/{}",
                                                next + 1,
                                                printer_count
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        // Multi-printer navigation: Shift+Tab cycles to previous printer
                        KeyCode::BackTab => {
                            let printer_count = app.printer_count();
                            if printer_count > 1 {
                                match app.view_mode {
                                    ViewMode::Aggregate => {
                                        // Switch to single view with last printer
                                        app.view_mode = ViewMode::Single;
                                        let last = printer_count - 1;
                                        app.set_active_printer(last);
                                        app.toast_info(format!(
                                            "Printer {printer_count}/{printer_count}"
                                        ));
                                    }
                                    ViewMode::Single => {
                                        let current = app.active_printer_index();
                                        if current == 0 {
                                            // At first printer, go back to aggregate
                                            app.view_mode = ViewMode::Aggregate;
                                            app.toast_info("Overview");
                                        } else {
                                            // Go to previous printer
                                            let prev = current - 1;
                                            app.set_active_printer(prev);
                                            app.toast_info(format!(
                                                "Printer {}/{}",
                                                prev + 1,
                                                printer_count
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        // Multi-printer navigation: number keys 1-9 jump to printer by index
                        KeyCode::Char(c @ '1'..='9') => {
                            let index = (c as usize) - ('1' as usize);
                            let printer_count = app.printer_count();
                            if index < printer_count && index < MAX_PRINTER_HOTKEYS {
                                app.view_mode = ViewMode::Single;
                                app.set_active_printer(index);
                                app.toast_info(format!("Printer {}/{}", index + 1, printer_count));
                            }
                        }
                        _ => {}
                    }
                }
                }
            }
            _ = tick_interval.tick() => {
                // Tick: just re-render (happens at top of loop)
            }
        }

        // Periodic full status refresh — guards against silently stale connections
        // where MQTT messages stop arriving without triggering a disconnect.
        if !mqtt_clients.is_empty() && last_status_refresh.elapsed() >= STATUS_REFRESH_INTERVAL {
            for client in mqtt_clients {
                let _ = client.request_full_status().await;
            }
            last_status_refresh = Instant::now();
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
