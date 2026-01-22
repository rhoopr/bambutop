//! Application state management for the TUI.
//!
//! This module contains the [`App`] struct which manages the connection state,
//! printer data, and UI preferences. It serves as the central state container
//! that bridges MQTT events with the terminal UI.

use crate::mqtt::{MqttEvent, SharedPrinterState};
use crate::printer::PrinterState;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How long toasts are displayed before auto-dismissing
const TOAST_DURATION: Duration = Duration::from_secs(3);

/// Duration after which a connection is considered stale if no messages received
const STALE_CONNECTION_THRESHOLD: Duration = Duration::from_secs(60);

/// Maximum number of toasts to display at once
const MAX_TOASTS: usize = 3;

/// Severity level for toast notifications, determines color
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastSeverity {
    /// Informational message (cyan)
    Info,
    /// Success message (green)
    Success,
    /// Warning message (yellow)
    Warning,
    /// Error message (red)
    Error,
}

/// A toast notification message
#[derive(Clone, Debug)]
pub struct Toast {
    /// The message to display
    pub message: String,
    /// Severity level (determines color)
    pub severity: ToastSeverity,
    /// When the toast was created
    pub created_at: Instant,
}

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
    /// Whether printer controls are locked (prevents accidental changes)
    pub controls_locked: bool,
    /// Whether to display temperatures in Celsius (true) or Fahrenheit (false)
    pub use_celsius: bool,
    /// Whether a cancel confirmation is pending (user pressed 'c' once)
    pub cancel_pending: bool,
    /// Whether a pause confirmation is pending (user pressed Space once)
    pub pause_pending: bool,
    /// Queue of toast notifications to display
    pub toasts: VecDeque<Toast>,
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
            controls_locked: true,
            use_celsius: true,
            cancel_pending: false,
            pause_pending: false,
            toasts: VecDeque::new(),
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

    /// Returns true if the connection appears stale (connected but no recent messages).
    /// A connection is considered stale if we're marked as connected but haven't
    /// received any messages for STALE_CONNECTION_THRESHOLD duration.
    pub fn is_connection_stale(&self) -> bool {
        if !self.connected {
            return false;
        }
        match self.last_update {
            Some(t) => t.elapsed() > STALE_CONNECTION_THRESHOLD,
            None => true, // Connected but never received data
        }
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

    /// Adds a toast notification with the given message and severity.
    pub fn add_toast(&mut self, message: impl Into<String>, severity: ToastSeverity) {
        let toast = Toast {
            message: message.into(),
            severity,
            created_at: Instant::now(),
        };
        self.toasts.push_back(toast);

        // Limit the number of toasts
        while self.toasts.len() > MAX_TOASTS {
            self.toasts.pop_front();
        }
    }

    /// Adds an info toast (convenience method).
    pub fn toast_info(&mut self, message: impl Into<String>) {
        self.add_toast(message, ToastSeverity::Info);
    }

    /// Adds a success toast (convenience method).
    pub fn toast_success(&mut self, message: impl Into<String>) {
        self.add_toast(message, ToastSeverity::Success);
    }

    /// Adds a warning toast (convenience method).
    pub fn toast_warning(&mut self, message: impl Into<String>) {
        self.add_toast(message, ToastSeverity::Warning);
    }

    /// Adds an error toast (convenience method).
    pub fn toast_error(&mut self, message: impl Into<String>) {
        self.add_toast(message, ToastSeverity::Error);
    }

    /// Removes expired toasts from the queue.
    pub fn expire_toasts(&mut self) {
        self.toasts
            .retain(|toast| toast.created_at.elapsed() < TOAST_DURATION);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn create_test_app() -> App {
        let printer_state = Arc::new(Mutex::new(PrinterState::default()));
        App::new(printer_state)
    }

    mod is_connection_stale_tests {
        use super::*;

        #[test]
        fn returns_false_when_disconnected() {
            let app = create_test_app();
            // App starts disconnected
            assert!(!app.is_connection_stale());
        }

        #[test]
        fn returns_true_when_connected_but_never_received_data() {
            let mut app = create_test_app();
            app.connected = true;
            app.last_update = None;
            assert!(app.is_connection_stale());
        }

        #[test]
        fn returns_false_when_connected_with_recent_update() {
            let mut app = create_test_app();
            app.connected = true;
            app.last_update = Some(Instant::now());
            assert!(!app.is_connection_stale());
        }

        #[test]
        fn returns_true_when_connected_with_old_update() {
            let mut app = create_test_app();
            app.connected = true;
            // Set last_update to a time older than the threshold
            app.last_update =
                Some(Instant::now() - STALE_CONNECTION_THRESHOLD - Duration::from_secs(1));
            assert!(app.is_connection_stale());
        }

        #[test]
        fn returns_false_when_update_exactly_at_threshold() {
            let mut app = create_test_app();
            app.connected = true;
            // Set last_update to exactly the threshold (not stale yet)
            app.last_update = Some(Instant::now() - STALE_CONNECTION_THRESHOLD);
            // Since we check elapsed() > threshold (not >=), this should not be stale
            // However, due to timing, a tiny amount of time may have passed
            // So we test with a small buffer
            app.last_update =
                Some(Instant::now() - STALE_CONNECTION_THRESHOLD + Duration::from_millis(100));
            assert!(!app.is_connection_stale());
        }
    }
}
