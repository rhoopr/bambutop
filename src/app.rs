//! Application state management for the TUI.
//!
//! This module contains the [`App`] struct which manages the connection state,
//! printer data, and UI preferences. It serves as the central state container
//! that bridges MQTT events with the terminal UI.

use crate::mqtt::{MqttEvent, SharedPrinterState};
use crate::printer::PrinterState;
use std::time::{Duration, Instant};

/// Application state for the TUI.
///
/// Manages the connection state, printer data, and UI preferences.
pub struct App {
    /// Shared printer state (updated by MQTT task)
    pub printer_state: SharedPrinterState,
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
    /// Creates a new App instance with the given shared printer state.
    pub fn new(printer_state: SharedPrinterState) -> Self {
        Self {
            printer_state,
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
    /// - `StateUpdated`: Records the update time (state is already updated via shared reference)
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
            MqttEvent::StateUpdated => {
                // State is updated via shared reference, just record the time
                self.connected = true;
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
    /// All return values are static strings, so the mutex lock is safely released
    /// before the return value is used.
    pub fn status_text(&self) -> &'static str {
        if !self.connected {
            return "Disconnected";
        }

        let state = self.printer_state.lock().expect("state lock poisoned");
        match state.print_status.gcode_state.as_str() {
            "IDLE" => "Idle",
            "PREPARE" => "Preparing",
            "RUNNING" => "Printing",
            "PAUSE" => "Paused",
            "FINISH" => "Finished",
            "FAILED" => "Failed",
            "" => "Connecting...",
            _ => "Unknown",
        }
    }

    /// Returns a snapshot of the printer state for rendering.
    ///
    /// This clones the state to avoid holding the lock during rendering.
    pub fn printer_state_snapshot(&self) -> PrinterState {
        self.printer_state
            .lock()
            .expect("state lock poisoned")
            .clone()
    }
}
