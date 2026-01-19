use crate::mqtt::MqttEvent;
use crate::printer::PrinterState;
use std::time::{Duration, Instant};

/// Application state for the TUI.
///
/// Manages the connection state, printer data, and UI preferences.
pub struct App {
    /// Current state of the connected printer
    pub printer_state: PrinterState,
    /// Whether the MQTT connection is active
    pub connected: bool,
    /// Timestamp of the last state update from the printer
    pub last_update: Option<Instant>,
    /// Current error message to display, if any
    pub error_message: Option<String>,
    /// Flag to signal the application should exit
    pub should_quit: bool,
    /// Whether to automatically refresh state from MQTT events
    pub auto_refresh: bool,
}

impl App {
    /// Creates a new App instance with default state.
    pub fn new() -> Self {
        Self {
            printer_state: PrinterState::default(),
            connected: false,
            last_update: None,
            error_message: None,
            should_quit: false,
            auto_refresh: true,
        }
    }

    /// Handles an MQTT event, updating application state accordingly.
    ///
    /// - `Connected`: Marks the connection as active and clears errors
    /// - `Disconnected`: Marks the connection as inactive
    /// - `StateUpdate`: Updates printer state and records the update time
    /// - `Error`: Stores the error message for display
    pub fn handle_mqtt_event(&mut self, event: MqttEvent) {
        match event {
            MqttEvent::Connected => {
                self.connected = true;
                self.error_message = None;
            }
            MqttEvent::Disconnected => {
                self.connected = false;
            }
            MqttEvent::StateUpdate(state) => {
                self.printer_state = *state;
                self.printer_state.connected = true;
                self.last_update = Some(Instant::now());
            }
            MqttEvent::Error(msg) => {
                self.error_message = Some(msg);
            }
        }
    }

    /// Returns the duration since the last state update, if any.
    pub fn time_since_update(&self) -> Option<Duration> {
        self.last_update.map(|t| t.elapsed())
    }

    /// Returns a human-readable status text based on connection and print state.
    ///
    /// Maps the printer's gcode_state to user-friendly labels.
    pub fn status_text(&self) -> &str {
        if !self.connected {
            "Disconnected"
        } else {
            match self.printer_state.print_status.gcode_state.as_str() {
                "IDLE" => "Idle",
                "PREPARE" => "Preparing",
                "RUNNING" => "Printing",
                "PAUSE" => "Paused",
                "FINISH" => "Finished",
                "FAILED" => "Failed",
                "" => "Connecting...",
                other => other,
            }
        }
    }
}
