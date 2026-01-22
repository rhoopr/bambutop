mod app;
mod config;
mod mqtt;
mod printer;
mod ui;
mod wizard;

use anyhow::Result;
use app::App;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mqtt::MqttClient;
use printer::{speed_level_to_name, speed_level_to_percent};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Flag to track whether terminal is in raw mode (for panic hook)
static TERMINAL_IN_RAW_MODE: AtomicBool = AtomicBool::new(false);

/// UI refresh rate - how often to poll for events and redraw
const UI_TICK_RATE: Duration = Duration::from_millis(250);

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --reset flag
    if args.reset {
        let config_path = config::Config::config_path()?;
        if config_path.exists() {
            std::fs::remove_file(&config_path)?;
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
        config.save()?;
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

    // Connect to printer (returns shared state for zero-copy updates)
    let (mqtt_client, printer_state, mut mqtt_rx) =
        MqttClient::connect(config.printer.clone()).await?;

    // Create app with shared printer state
    let mut app = App::new(printer_state);

    // Request initial state
    mqtt_client.request_full_status().await?;

    // Main loop
    let result = run_app(
        &mut terminal,
        &mut app,
        &mut mqtt_rx,
        UI_TICK_RATE,
        &mqtt_client,
    )
    .await;

    // Gracefully disconnect from MQTT broker
    mqtt_client.disconnect().await;

    // Restore terminal
    TERMINAL_IN_RAW_MODE.store(false, Ordering::SeqCst);
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {}", err);
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
    mqtt_client: &MqttClient,
) -> Result<()> {
    loop {
        // Expire old toasts before rendering
        app.expire_toasts();

        terminal.draw(|f| ui::render(f, app))?;

        // Check for MQTT events (non-blocking)
        while let Ok(event) = mqtt_rx.try_recv() {
            app.handle_mqtt_event(event);
        }

        // Check for keyboard events
        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
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
                            app.toast_info(format!("Temperature: {}", unit));
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char(']') => {
                            if !app.controls_locked {
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
                                    if let Err(e) = mqtt_client.set_speed_level(new_level).await {
                                        app.toast_error(format!("Speed change failed: {}", e));
                                    } else {
                                        app.toast_success(format!("Speed: {}", speed_display));
                                    }
                                }
                            }
                        }
                        KeyCode::Char('-') | KeyCode::Char('[') => {
                            if !app.controls_locked {
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
                                    if let Err(e) = mqtt_client.set_speed_level(new_level).await {
                                        app.toast_error(format!("Speed change failed: {}", e));
                                    } else {
                                        app.toast_success(format!("Speed: {}", speed_display));
                                    }
                                }
                            }
                        }
                        KeyCode::Char('l') => {
                            if !app.controls_locked {
                                let current = app
                                    .printer_state
                                    .lock()
                                    .expect("state lock poisoned")
                                    .lights
                                    .chamber_light;
                                let new_state = !current;
                                let status = if new_state { "ON" } else { "OFF" };
                                if let Err(e) = mqtt_client.set_chamber_light(new_state).await {
                                    app.toast_error(format!("Light toggle failed: {}", e));
                                } else {
                                    app.toast_success(format!("Light: {}", status));
                                }
                            }
                        }
                        KeyCode::Char(' ') => {
                            if !app.controls_locked {
                                let (is_running, is_paused) = {
                                    let state =
                                        app.printer_state.lock().expect("state lock poisoned");
                                    let gcode = state.print_status.gcode_state.as_str();
                                    (gcode == "RUNNING", gcode == "PAUSE")
                                };
                                let has_active_job = is_running || is_paused;
                                if has_active_job {
                                    if app.pause_pending {
                                        // Second press - confirm pause/resume
                                        if is_running {
                                            match mqtt_client.pause_print().await {
                                                Ok(_) => app.toast_warning("Print paused"),
                                                Err(e) => app
                                                    .toast_error(format!("Failed to pause: {}", e)),
                                            }
                                        } else if is_paused {
                                            match mqtt_client.resume_print().await {
                                                Ok(_) => app.toast_success("Print resumed"),
                                                Err(e) => app.toast_error(format!(
                                                    "Failed to resume: {}",
                                                    e
                                                )),
                                            }
                                        }
                                        app.pause_pending = false;
                                    } else {
                                        // First press - request confirmation
                                        app.pause_pending = true;
                                        // Clear cancel confirmation if it was pending
                                        app.cancel_pending = false;
                                    }
                                }
                            }
                        }
                        KeyCode::Char('c') => {
                            if !app.controls_locked {
                                let has_active_job = {
                                    let state =
                                        app.printer_state.lock().expect("state lock poisoned");
                                    let gcode = state.print_status.gcode_state.as_str();
                                    gcode == "RUNNING" || gcode == "PAUSE"
                                };
                                if has_active_job {
                                    if app.cancel_pending {
                                        // Second press - confirm cancel
                                        match mqtt_client.stop_print().await {
                                            Ok(_) => app.toast_error("Print cancelled"),
                                            Err(e) => {
                                                app.toast_error(format!("Failed to cancel: {}", e))
                                            }
                                        }
                                        app.cancel_pending = false;
                                    } else {
                                        // First press - request confirmation
                                        app.cancel_pending = true;
                                        // Clear pause confirmation if it was pending
                                        app.pause_pending = false;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
