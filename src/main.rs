mod app;
mod config;
mod mqtt;
mod printer;
mod ui;

use anyhow::Result;
use app::App;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mqtt::MqttClient;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load config
    let mut config = config::Config::load()?;

    // Override with CLI args
    if let Some(ip) = args.ip {
        config.printer.ip = ip;
    }
    if let Some(serial) = args.serial {
        config.printer.serial = serial;
    }
    if let Some(access_code) = args.access_code {
        config.printer.access_code = access_code;
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new();

    // Connect to printer
    let (mqtt_client, mut mqtt_rx) = MqttClient::connect(config.printer.clone()).await?;

    // Request initial state
    mqtt_client.request_full_status().await?;

    // Main loop
    let tick_rate = Duration::from_millis(250);
    let result = run_app(&mut terminal, &mut app, &mut mqtt_rx, tick_rate, &mqtt_client).await;

    // Restore terminal
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

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    mqtt_rx: &mut tokio::sync::mpsc::Receiver<mqtt::MqttEvent>,
    tick_rate: Duration,
    _mqtt_client: &MqttClient, // Kept alive to maintain MQTT connection
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        // Check for MQTT events (non-blocking)
        while let Ok(event) = mqtt_rx.try_recv() {
            if app.auto_refresh {
                app.handle_mqtt_event(event);
            }
        }

        // Check for keyboard events
        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('r') => {
                            app.auto_refresh = !app.auto_refresh;
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
