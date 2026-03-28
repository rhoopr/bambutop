//! Application state management for the TUI.
//!
//! This module contains the [`App`] struct which manages the connection state,
//! printer data, and UI preferences. It serves as the central state container
//! that bridges MQTT events with the terminal UI.

use crate::config::NotificationConfig;
use crate::mqtt::{MqttEvent, SharedPrinterState};
use crate::printer::{GcodeState, PrinterState};
use std::collections::{HashSet, VecDeque};
#[cfg(test)]
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long toasts are displayed before auto-dismissing
const TOAST_DURATION: Duration = Duration::from_secs(3);

/// Duration after which a connection is considered stale if no messages received
#[cfg(test)]
const STALE_CONNECTION_THRESHOLD: Duration = Duration::from_secs(60);

/// Maximum number of toasts to display at once
const MAX_TOASTS: usize = 3;

/// View mode for the UI - single printer detail or aggregate overview
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    /// Show all printers in a grid overview (default when multiple printers)
    #[default]
    Aggregate,
    /// Show detailed view of a single selected printer
    Single,
}

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
    /// All printer states for multi-printer support
    printers: Vec<SharedPrinterState>,
    /// Connection status for each printer (parallel to printers vec)
    printer_connections: Vec<bool>,
    /// Cached count of connected printers for O(1) access.
    ///
    /// This field is maintained incrementally by `set_printer_connected()` to avoid
    /// O(n) iteration over `printer_connections` each time the count is needed.
    /// The count is updated only when a connection state actually changes.
    connected_count: usize,
    /// Last update timestamp for each printer (parallel to printers vec)
    printer_last_updates: Vec<Option<Instant>>,
    /// Error messages for each printer (parallel to printers vec)
    printer_error_messages: Vec<Option<String>>,
    /// Index of the currently active/selected printer
    active_printer_index: usize,
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
    timezone_offset_secs: i32,
    /// Whether to show the help overlay
    pub show_help: bool,
    /// Current view mode (aggregate or single printer)
    pub view_mode: ViewMode,
    /// Cached printer state snapshots (one per printer).
    /// Refreshed lazily via `refresh_snapshots()` before each render frame.
    cached_snapshots: Vec<PrinterState>,
    /// Dirty flags for each printer snapshot (set on StateUpdated, cleared by refresh).
    snapshot_dirty: Vec<bool>,
    /// Desktop notification preferences (toggleable at runtime).
    pub notifications: NotificationConfig,
}

impl App {
    /// Creates a new App instance with the given shared printer state.
    ///
    /// Computes and caches the local timezone offset at startup.
    /// For backward compatibility, initializes multi-printer support with the single printer.
    #[cfg(test)]
    pub fn new(printer_state: SharedPrinterState) -> Self {
        // Initialize with single printer for backward compatibility
        let initial_snapshot = printer_state.lock().expect("state lock poisoned").clone();
        let printers = vec![Arc::clone(&printer_state)];
        let printer_connections = vec![false];
        let printer_last_updates = vec![None];
        let printer_error_messages = vec![None];

        Self {
            printers,
            printer_connections,
            connected_count: 0,
            printer_last_updates,
            printer_error_messages,
            active_printer_index: 0,
            should_quit: false,
            controls_locked: true,
            use_celsius: true,
            cancel_pending: false,
            pause_pending: false,
            toasts: VecDeque::new(),
            timezone_offset_secs: Self::compute_timezone_offset(),
            show_help: false,
            view_mode: ViewMode::Single,
            cached_snapshots: vec![initial_snapshot],
            snapshot_dirty: vec![true],
            notifications: NotificationConfig::default(),
        }
    }

    /// Creates a new App instance with multiple printer states.
    ///
    /// The first printer in the list becomes the active printer.
    /// Panics if the printers vector is empty.
    pub fn new_multi(printers: Vec<SharedPrinterState>, notifications: NotificationConfig) -> Self {
        assert!(!printers.is_empty(), "At least one printer is required");

        let printer_count = printers.len();
        let printer_connections = vec![false; printer_count];
        let printer_last_updates = vec![None; printer_count];
        let printer_error_messages = vec![None; printer_count];

        // Take initial snapshots of all printers
        let cached_snapshots: Vec<PrinterState> = printers
            .iter()
            .map(|p| p.lock().expect("state lock poisoned").clone())
            .collect();

        // Use aggregate view when multiple printers, single view otherwise
        let view_mode = if printer_count > 1 {
            ViewMode::Aggregate
        } else {
            ViewMode::Single
        };

        Self {
            printers,
            printer_connections,
            connected_count: 0,
            printer_last_updates,
            printer_error_messages,
            active_printer_index: 0,
            should_quit: false,
            controls_locked: true,
            use_celsius: true,
            cancel_pending: false,
            pause_pending: false,
            toasts: VecDeque::new(),
            timezone_offset_secs: Self::compute_timezone_offset(),
            show_help: false,
            view_mode,
            cached_snapshots,
            snapshot_dirty: vec![true; printer_count],
            notifications,
        }
    }

    // ========================================================================
    // Multi-printer accessors and mutators
    // ========================================================================

    /// Returns the number of printers that are currently connected.
    ///
    /// This returns a cached count maintained by `set_printer_connected()`,
    /// providing O(1) access instead of O(n) iteration over printer connections.
    pub fn get_connected_count(&self) -> usize {
        self.connected_count
    }

    /// Returns the total number of printers.
    pub fn printer_count(&self) -> usize {
        self.printers.len()
    }

    /// Returns the index of the currently active printer.
    pub fn active_printer_index(&self) -> usize {
        self.active_printer_index
    }

    /// Returns snapshots of all printer states for rendering.
    ///
    /// This clones each state to avoid holding locks during rendering.
    /// Returns cached snapshots of all printer states.
    ///
    /// Call `refresh_snapshots()` before each render frame to ensure freshness.
    pub fn all_printer_snapshots(&self) -> &[PrinterState] {
        &self.cached_snapshots
    }

    /// Sets the active printer to the given index.
    ///
    /// Returns true if the index was valid and the active printer was changed,
    /// false if the index was out of bounds.
    pub fn set_active_printer(&mut self, index: usize) -> bool {
        if index < self.printers.len() {
            self.active_printer_index = index;
            true
        } else {
            false
        }
    }

    /// Updates the connection status for a specific printer.
    ///
    /// Maintains the cached `connected_count` incrementally:
    /// - Increments when changing from disconnected to connected
    /// - Decrements when changing from connected to disconnected
    /// - No change if the state is already the target value
    pub fn set_printer_connected(&mut self, index: usize, connected: bool) {
        if let Some(conn) = self.printer_connections.get_mut(index) {
            let was_connected = *conn;
            if was_connected != connected {
                *conn = connected;
                if connected {
                    self.connected_count += 1;
                } else {
                    self.connected_count = self.connected_count.saturating_sub(1);
                }
            }
        }
    }

    /// Updates the last update timestamp for a specific printer.
    pub fn set_printer_last_update(&mut self, index: usize, timestamp: Option<Instant>) {
        if let Some(last_update) = self.printer_last_updates.get_mut(index) {
            *last_update = timestamp;
        }
    }

    /// Returns the connection status for a specific printer.
    pub fn is_printer_connected(&self, index: usize) -> bool {
        self.printer_connections
            .get(index)
            .copied()
            .unwrap_or(false)
    }

    /// Returns the last update timestamp for a specific printer.
    pub fn get_printer_last_update(&self, index: usize) -> Option<Instant> {
        self.printer_last_updates.get(index).copied().flatten()
    }

    /// Computes the local timezone offset in seconds from UTC.
    ///
    /// Uses libc `localtime_r` to get the offset directly from the OS,
    /// avoiding the overhead of spawning a subprocess.
    fn compute_timezone_offset() -> i32 {
        unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&now, &mut tm);
            tm.tm_gmtoff as i32
        }
    }

    /// Returns the cached timezone offset in seconds from UTC.
    ///
    /// Positive values indicate timezones east of UTC (e.g., +3600 for UTC+1).
    /// Negative values indicate timezones west of UTC (e.g., -18000 for UTC-5).
    ///
    /// This value is computed once at startup and cached for use by time-related
    /// rendering (ETA display, last updated timestamps).
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
                // Clear error for this printer
                self.set_printer_error(printer_index, None);
                // Update multi-printer state
                self.set_printer_connected(printer_index, true);
            }
            MqttEvent::Disconnected { printer_index } => {
                self.set_printer_connected(printer_index, false);
            }
            MqttEvent::StateUpdated { printer_index } => {
                // Check for notification-worthy transitions before marking dirty
                self.check_state_notifications(printer_index);
                // State is updated via shared reference, just record the time
                self.set_printer_last_update(printer_index, Some(Instant::now()));
                self.set_printer_connected(printer_index, true);
                // Mark snapshot as dirty so it gets refreshed before next render
                if let Some(flag) = self.snapshot_dirty.get_mut(printer_index) {
                    *flag = true;
                }
            }
            MqttEvent::Error {
                printer_index,
                message,
            } => {
                self.set_printer_error(printer_index, Some(message));
            }
        }
    }

    /// Sets the error message for a specific printer.
    fn set_printer_error(&mut self, index: usize, error: Option<String>) {
        if let Some(err_slot) = self.printer_error_messages.get_mut(index) {
            *err_slot = error;
        }
    }

    /// Returns the shared state of the active printer.
    pub fn active_printer_state(&self) -> &SharedPrinterState {
        &self.printers[self.active_printer_index]
    }

    /// Returns the error message for the active printer, if any.
    pub fn active_error_message(&self) -> Option<&str> {
        self.printer_error_messages
            .get(self.active_printer_index)
            .and_then(|e| e.as_deref())
    }

    /// Returns the duration since the last state update for the active printer.
    pub fn time_since_update(&self) -> Option<Duration> {
        self.printer_last_updates
            .get(self.active_printer_index)
            .copied()
            .flatten()
            .map(|t| t.elapsed())
    }

    /// Returns true if the active connection appears stale.
    #[cfg(test)]
    pub fn is_connection_stale(&self) -> bool {
        let connected = self
            .printer_connections
            .get(self.active_printer_index)
            .copied()
            .unwrap_or(false);
        if !connected {
            return false;
        }
        match self
            .printer_last_updates
            .get(self.active_printer_index)
            .copied()
            .flatten()
        {
            Some(t) => t.elapsed() > STALE_CONNECTION_THRESHOLD,
            None => true,
        }
    }

    /// Returns a human-readable status text based on connection and print state.
    pub fn status_text(&self) -> &'static str {
        let connected = self
            .printer_connections
            .get(self.active_printer_index)
            .copied()
            .unwrap_or(false);
        if !connected {
            return "Disconnected";
        }

        let state = self.printers[self.active_printer_index]
            .lock()
            .expect("state lock poisoned");
        crate::ui::common::gcode_state_to_status(state.print_status.gcode_state)
    }

    /// Returns a cached snapshot of the active printer state for rendering.
    ///
    /// Call `refresh_snapshots()` before each render frame to ensure freshness.
    pub fn printer_state_snapshot(&self) -> &PrinterState {
        &self.cached_snapshots[self.active_printer_index]
    }

    /// Refreshes cached printer state snapshots for any printers marked dirty.
    ///
    /// Call this once per render frame, before drawing. Only re-clones states
    /// that have changed since the last refresh, reducing lock contention.
    pub fn refresh_snapshots(&mut self) {
        for (i, dirty) in self.snapshot_dirty.iter_mut().enumerate() {
            if *dirty {
                if let Some(printer) = self.printers.get(i) {
                    self.cached_snapshots[i] = printer.lock().expect("state lock poisoned").clone();
                }
                *dirty = false;
            }
        }
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

    /// Checks for state transitions that should trigger notifications.
    ///
    /// Compares the cached snapshot (previous state) with the current shared state
    /// to detect GcodeState transitions and new HMS errors. Must be called BEFORE
    /// marking the snapshot dirty so the cached snapshot still holds the old state.
    fn check_state_notifications(&mut self, printer_index: usize) {
        // Extract old state (cheap copies only, no heap allocation)
        let (old_gcode, old_hms_received) = match self.cached_snapshots.get(printer_index) {
            Some(s) => (s.print_status.gcode_state, s.hms_received),
            None => return,
        };

        // Early exit: nothing to detect on initial connection with no HMS data
        if old_gcode == GcodeState::Unknown && !old_hms_received {
            return;
        }

        let shared = match self.printers.get(printer_index) {
            Some(p) => p,
            None => return,
        };
        let state = shared.lock().expect("state lock poisoned");
        let new_gcode = state.print_status.gcode_state;

        // Detect transitions
        let is_completion = old_gcode != GcodeState::Unknown
            && new_gcode == GcodeState::Finish
            && old_gcode != GcodeState::Finish;
        let is_failure = old_gcode != GcodeState::Unknown
            && new_gcode == GcodeState::Failed
            && old_gcode != GcodeState::Failed;

        // Find new HMS error messages (only allocate when there are actually new codes)
        let new_hms_messages: Vec<String> = if old_hms_received {
            let old_codes: HashSet<u32> = self.cached_snapshots[printer_index]
                .hms_errors
                .iter()
                .map(|e| e.code)
                .collect();
            state
                .hms_errors
                .iter()
                .filter(|e| !old_codes.contains(&e.code))
                .map(|e| e.message.to_string())
                .collect()
        } else {
            Vec::new()
        };

        // Early exit if no notifications needed (avoids name/failure string allocs)
        if !is_completion && !is_failure && new_hms_messages.is_empty() {
            return;
        }

        // Allocate strings only when we know a notification will fire
        let printer_name = if state.printer_name.is_empty() {
            format!("Printer {}", printer_index + 1)
        } else {
            state.printer_name.clone()
        };
        let failure_desc = if is_failure {
            state
                .print_status
                .failure_description()
                .map(|c| c.into_owned())
        } else {
            None
        };
        drop(state);

        if is_completion {
            let msg = format!("{printer_name}: Print complete!");
            self.add_toast(&msg, ToastSeverity::Success);
            if self.notifications.completions {
                crate::notifications::send("Print Complete", &msg);
            }
        }

        if is_failure {
            let msg = match &failure_desc {
                Some(desc) => format!("{printer_name}: Print failed — {desc}"),
                None => format!("{printer_name}: Print failed"),
            };
            self.add_toast(&msg, ToastSeverity::Error);
            if self.notifications.errors {
                crate::notifications::send("Print Failed", &msg);
            }
        }

        for message in &new_hms_messages {
            let msg = format!("{printer_name}: {message}");
            self.add_toast(&msg, ToastSeverity::Warning);
            if self.notifications.errors {
                crate::notifications::send("HMS Alert", &msg);
            }
        }
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
            assert!(!app.is_connection_stale());
        }

        #[test]
        fn returns_true_when_connected_but_never_received_data() {
            let mut app = create_test_app();
            app.set_printer_connected(0, true);
            assert!(app.is_connection_stale());
        }

        #[test]
        fn returns_false_when_connected_with_recent_update() {
            let mut app = create_test_app();
            app.set_printer_connected(0, true);
            app.set_printer_last_update(0, Some(Instant::now()));
            assert!(!app.is_connection_stale());
        }

        #[test]
        fn returns_true_when_connected_with_old_update() {
            let mut app = create_test_app();
            app.set_printer_connected(0, true);
            app.set_printer_last_update(
                0,
                Some(Instant::now() - STALE_CONNECTION_THRESHOLD - Duration::from_secs(1)),
            );
            assert!(app.is_connection_stale());
        }

        #[test]
        fn returns_false_when_update_near_threshold() {
            let mut app = create_test_app();
            app.set_printer_connected(0, true);
            app.set_printer_last_update(
                0,
                Some(Instant::now() - STALE_CONNECTION_THRESHOLD + Duration::from_millis(100)),
            );
            assert!(!app.is_connection_stale());
        }
    }

    mod notification_tests {
        use super::*;
        use crate::printer::HmsError;
        use std::borrow::Cow;

        fn make_hms_error(code: u32, message: &'static str) -> HmsError {
            HmsError {
                code,
                module: 0,
                severity: 0,
                message: Cow::Borrowed(message),
                received_at: Instant::now(),
            }
        }

        fn app_with_running_print() -> App {
            let mut app = create_test_app();
            app.cached_snapshots[0].print_status.gcode_state = GcodeState::Running;
            app
        }

        #[test]
        fn print_completion_generates_success_toast() {
            let mut app = app_with_running_print();
            app.printers[0]
                .lock()
                .expect("lock")
                .print_status
                .gcode_state = GcodeState::Finish;
            app.check_state_notifications(0);
            assert_eq!(app.toasts.len(), 1);
            assert_eq!(app.toasts[0].severity, ToastSeverity::Success);
            assert!(app.toasts[0].message.contains("Print complete"));
        }

        #[test]
        fn print_failure_generates_error_toast() {
            let mut app = app_with_running_print();
            app.printers[0]
                .lock()
                .expect("lock")
                .print_status
                .gcode_state = GcodeState::Failed;
            app.check_state_notifications(0);
            assert_eq!(app.toasts.len(), 1);
            assert_eq!(app.toasts[0].severity, ToastSeverity::Error);
            assert!(app.toasts[0].message.contains("Print failed"));
        }

        #[test]
        fn no_toast_on_initial_connection() {
            let mut app = create_test_app();
            app.printers[0]
                .lock()
                .expect("lock")
                .print_status
                .gcode_state = GcodeState::Finish;
            app.check_state_notifications(0);
            assert!(app.toasts.is_empty());
        }

        #[test]
        fn new_hms_error_generates_warning_toast() {
            let mut app = create_test_app();
            app.cached_snapshots[0].hms_received = true;
            app.printers[0].lock().expect("lock").hms_errors =
                vec![make_hms_error(0x0500_0200, "Filament may be tangled")];
            app.check_state_notifications(0);
            assert_eq!(app.toasts.len(), 1);
            assert_eq!(app.toasts[0].severity, ToastSeverity::Warning);
            assert!(app.toasts[0].message.contains("Filament may be tangled"));
        }

        #[test]
        fn existing_hms_error_does_not_re_notify() {
            let mut app = create_test_app();
            let error = make_hms_error(0x0500_0200, "Filament may be tangled");
            app.cached_snapshots[0].hms_received = true;
            app.cached_snapshots[0].hms_errors = vec![error.clone()];
            app.printers[0].lock().expect("lock").hms_errors = vec![error];
            app.check_state_notifications(0);
            assert!(app.toasts.is_empty());
        }

        #[test]
        fn no_hms_toast_before_first_hms_report() {
            let mut app = create_test_app();
            app.printers[0].lock().expect("lock").hms_errors =
                vec![make_hms_error(0x0700_0100, "AMS warning")];
            app.check_state_notifications(0);
            assert!(app.toasts.is_empty());
        }
    }
}
