//! Application state management for the TUI.
//!
//! This module contains the [`App`] struct which manages the connection state,
//! printer data, and UI preferences. It serves as the central state container
//! that bridges MQTT events with the terminal UI.

use crate::mqtt::{MqttEvent, SharedPrinterState};
use crate::printer::PrinterState;
use std::collections::VecDeque;
use std::sync::Arc;
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
/// Supports multiple printer connections with an active printer selection.
pub struct App {
    /// Shared printer state (updated by MQTT task) - kept for backward compatibility
    pub printer_state: SharedPrinterState,
    /// Whether the MQTT connection is active - kept for backward compatibility
    pub connected: bool,
    // Multi-printer state management fields - these are new infrastructure for multi-printer
    // support and will be used by main.rs and UI code once multi-printer feature is integrated.
    /// All printer states for multi-printer support
    #[allow(dead_code)] // Will be used by multi-printer integration
    printers: Vec<SharedPrinterState>,
    /// Connection status for each printer (parallel to printers vec)
    #[allow(dead_code)] // Will be used by multi-printer integration
    printer_connections: Vec<bool>,
    /// Cached count of connected printers for O(1) access.
    ///
    /// This field is maintained incrementally by `set_printer_connected()` to avoid
    /// O(n) iteration over `printer_connections` each time the count is needed.
    /// The count is updated only when a connection state actually changes.
    #[allow(dead_code)] // Will be used by multi-printer integration
    connected_count: usize,
    /// Last update timestamp for each printer (parallel to printers vec)
    #[allow(dead_code)] // Will be used by multi-printer integration
    printer_last_updates: Vec<Option<Instant>>,
    /// Index of the currently active/selected printer
    #[allow(dead_code)] // Will be used by multi-printer integration
    active_printer_index: usize,
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
    /// Cached timezone offset in seconds from UTC (computed once at startup).
    /// Positive values are east of UTC, negative values are west.
    /// Note: This field is intentionally cached at startup for use by time-related
    /// rendering (ETA display, last updated timestamps) to avoid repeated computation.
    #[allow(dead_code)] // Getter provided for future use by UI rendering code
    timezone_offset_secs: i32,
    /// Whether to show the help overlay
    pub show_help: bool,
}

impl App {
    /// Creates a new App instance with the given shared printer state.
    ///
    /// Computes and caches the local timezone offset at startup.
    /// For backward compatibility, initializes multi-printer support with the single printer.
    pub fn new(printer_state: SharedPrinterState) -> Self {
        // Initialize with single printer for backward compatibility
        let printers = vec![Arc::clone(&printer_state)];
        let printer_connections = vec![false];
        let printer_last_updates = vec![None];

        Self {
            printer_state,
            connected: false,
            printers,
            printer_connections,
            connected_count: 0,
            printer_last_updates,
            active_printer_index: 0,
            last_update: None,
            error_message: None,
            should_quit: false,
            controls_locked: true,
            use_celsius: true,
            cancel_pending: false,
            pause_pending: false,
            toasts: VecDeque::new(),
            timezone_offset_secs: Self::compute_timezone_offset(),
            show_help: false,
        }
    }

    /// Creates a new App instance with multiple printer states.
    ///
    /// The first printer in the list becomes the active printer.
    /// Panics if the printers vector is empty.
    #[allow(dead_code)] // Will be used by multi-printer integration in main.rs
    pub fn new_multi(printers: Vec<SharedPrinterState>) -> Self {
        assert!(!printers.is_empty(), "At least one printer is required");

        let printer_count = printers.len();
        let printer_state = Arc::clone(&printers[0]);
        let printer_connections = vec![false; printer_count];
        let printer_last_updates = vec![None; printer_count];

        Self {
            printer_state,
            connected: false,
            printers,
            printer_connections,
            connected_count: 0,
            printer_last_updates,
            active_printer_index: 0,
            last_update: None,
            error_message: None,
            should_quit: false,
            controls_locked: true,
            use_celsius: true,
            cancel_pending: false,
            pause_pending: false,
            toasts: VecDeque::new(),
            timezone_offset_secs: Self::compute_timezone_offset(),
            show_help: false,
        }
    }

    // ========================================================================
    // Multi-printer accessors and mutators
    // These methods provide the API for multi-printer support and will be used
    // by main.rs and UI code once multi-printer feature is fully integrated.
    // ========================================================================

    /// Returns the currently active printer state, if any printers exist.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn get_active_printer(&self) -> Option<&SharedPrinterState> {
        self.printers.get(self.active_printer_index)
    }

    /// Returns the printer state at the given index, if it exists.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn get_printer(&self, index: usize) -> Option<&SharedPrinterState> {
        self.printers.get(index)
    }

    /// Returns the number of printers that are currently connected.
    ///
    /// This returns a cached count maintained by `set_printer_connected()`,
    /// providing O(1) access instead of O(n) iteration over printer connections.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn get_connected_count(&self) -> usize {
        self.connected_count
    }

    /// Returns the total number of printers.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn printer_count(&self) -> usize {
        self.printers.len()
    }

    /// Returns the index of the currently active printer.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn active_printer_index(&self) -> usize {
        self.active_printer_index
    }

    /// Sets the active printer to the given index.
    ///
    /// Returns true if the index was valid and the active printer was changed,
    /// false if the index was out of bounds.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn set_active_printer(&mut self, index: usize) -> bool {
        if index < self.printers.len() {
            self.active_printer_index = index;
            // Update legacy fields to point to the new active printer
            self.printer_state = Arc::clone(&self.printers[index]);
            self.connected = self.printer_connections[index];
            self.last_update = self.printer_last_updates[index];
            true
        } else {
            false
        }
    }

    /// Adds a new printer to the list.
    ///
    /// Returns the index of the newly added printer.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn add_printer(&mut self, printer_state: SharedPrinterState) -> usize {
        let index = self.printers.len();
        self.printers.push(printer_state);
        self.printer_connections.push(false);
        self.printer_last_updates.push(None);
        index
    }

    /// Updates the connection status for a specific printer.
    ///
    /// Maintains the cached `connected_count` incrementally:
    /// - Increments when changing from disconnected to connected
    /// - Decrements when changing from connected to disconnected
    /// - No change if the state is already the target value
    ///
    /// Also updates the legacy `connected` field if this is the active printer.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn set_printer_connected(&mut self, index: usize, connected: bool) {
        if let Some(conn) = self.printer_connections.get_mut(index) {
            let was_connected = *conn;
            if was_connected != connected {
                *conn = connected;
                // Update cached count based on state transition
                if connected {
                    self.connected_count += 1;
                } else {
                    self.connected_count = self.connected_count.saturating_sub(1);
                }
            }
            // Update legacy field if this is the active printer
            if index == self.active_printer_index {
                self.connected = connected;
            }
        }
    }

    /// Updates the last update timestamp for a specific printer.
    ///
    /// Also updates the legacy `last_update` field if this is the active printer.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn set_printer_last_update(&mut self, index: usize, timestamp: Option<Instant>) {
        if let Some(last_update) = self.printer_last_updates.get_mut(index) {
            *last_update = timestamp;
            // Update legacy field if this is the active printer
            if index == self.active_printer_index {
                self.last_update = timestamp;
            }
        }
    }

    /// Returns the connection status for a specific printer.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn is_printer_connected(&self, index: usize) -> bool {
        self.printer_connections
            .get(index)
            .copied()
            .unwrap_or(false)
    }

    /// Returns the last update timestamp for a specific printer.
    #[allow(dead_code)] // Will be used by multi-printer integration
    pub fn get_printer_last_update(&self, index: usize) -> Option<Instant> {
        self.printer_last_updates.get(index).copied().flatten()
    }

    /// Computes the local timezone offset in seconds from UTC.
    ///
    /// Uses the system's `date` command to get the timezone offset.
    /// This is computed once at startup to avoid repeated overhead.
    /// Returns the offset where positive values are east of UTC and negative values are west.
    fn compute_timezone_offset() -> i32 {
        use std::process::Command;

        // Use the `date` command to get timezone offset in +HHMM/-HHMM format
        // This works on macOS, Linux, and most Unix-like systems
        if let Ok(output) = Command::new("date").arg("+%z").output() {
            if output.status.success() {
                if let Ok(offset_str) = std::str::from_utf8(&output.stdout) {
                    return Self::parse_timezone_offset(offset_str.trim());
                }
            }
        }

        // Fallback: Use environment variable TZ parsing or assume UTC
        if let Ok(tz) = std::env::var("TZ") {
            // Simple parsing for common formats like "EST5EDT" or "UTC"
            if tz.starts_with("UTC") || tz.starts_with("GMT") {
                // Parse optional offset like "UTC-5" or "GMT+1"
                if let Some(offset_part) = tz.get(3..) {
                    if let Ok(hours) = offset_part.parse::<i32>() {
                        // Note: TZ convention is opposite (EST5 means UTC-5)
                        return -hours * 3600;
                    }
                }
                return 0;
            }
        }

        // Final fallback: assume UTC
        0
    }

    /// Parses a timezone offset string in +HHMM or -HHMM format.
    fn parse_timezone_offset(offset_str: &str) -> i32 {
        if offset_str.len() >= 5 {
            let sign = if offset_str.starts_with('-') { -1 } else { 1 };
            // Parse "+HHMM" or "-HHMM" format
            if let (Ok(hours), Ok(mins)) = (
                offset_str[1..3].parse::<i32>(),
                offset_str[3..5].parse::<i32>(),
            ) {
                return sign * (hours * 3600 + mins * 60);
            }
        }
        0
    }

    /// Returns the cached timezone offset in seconds from UTC.
    ///
    /// Positive values indicate timezones east of UTC (e.g., +3600 for UTC+1).
    /// Negative values indicate timezones west of UTC (e.g., -18000 for UTC-5).
    ///
    /// This value is computed once at startup and cached for use by time-related
    /// rendering (ETA display, last updated timestamps).
    #[allow(dead_code)] // Provided for future use by UI rendering code
    pub fn timezone_offset_secs(&self) -> i32 {
        self.timezone_offset_secs
    }

    /// Handles an MQTT event, updating application state accordingly.
    ///
    /// - `Connected`: Marks the connection as active and clears errors
    /// - `Disconnected`: Marks the connection as inactive
    /// - `StateUpdated`: Records the update time (state is already updated via shared reference)
    /// - `Error`: Stores the error message for display
    pub fn handle_mqtt_event(&mut self, event: MqttEvent) {
        match event {
            MqttEvent::Connected { printer_index } => {
                self.connected = true;
                self.error_message = None;
                // Update multi-printer state if available
                self.set_printer_connected(printer_index, true);
            }
            MqttEvent::Disconnected { printer_index } => {
                self.connected = false;
                self.set_printer_connected(printer_index, false);
            }
            MqttEvent::StateUpdated { printer_index } => {
                // State is updated via shared reference, just record the time
                self.connected = true;
                self.last_update = Some(Instant::now());
                self.set_printer_last_update(printer_index, Some(Instant::now()));
            }
            MqttEvent::Error { message, .. } => {
                self.error_message = Some(message);
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

    mod timezone_offset_tests {
        use super::*;

        #[test]
        fn timezone_offset_is_within_valid_range() {
            let app = create_test_app();
            let offset = app.timezone_offset_secs();
            // Valid timezone offsets are between UTC-12 and UTC+14
            // In seconds: -43200 to +50400
            assert!(
                (-43200..=50400).contains(&offset),
                "Timezone offset {} is outside valid range",
                offset
            );
        }

        #[test]
        fn timezone_offset_is_consistent() {
            // Create two apps and verify they get the same timezone offset
            let app1 = create_test_app();
            let app2 = create_test_app();
            assert_eq!(
                app1.timezone_offset_secs(),
                app2.timezone_offset_secs(),
                "Timezone offset should be consistent across App instances"
            );
        }
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
