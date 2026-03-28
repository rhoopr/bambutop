//! Desktop notification support via the system notification center.
//!
//! Sends native notifications for print events (completion, failure, HMS errors).
//! Uses `notify-rust` for cross-platform support (macOS, Linux, Windows).

use notify_rust::Notification;

/// Sends a desktop notification on a background thread.
///
/// Non-blocking: `notify-rust` may block on macOS (synchronous ObjC call),
/// so we spawn a thread to avoid stalling the event loop. Failures are
/// silently ignored — desktop notifications are best-effort.
pub fn send(title: &str, body: &str) {
    let title = title.to_string();
    let body = body.to_string();
    std::thread::spawn(move || {
        let _ = Notification::new()
            .summary(&title)
            .body(&body)
            .appname("bambutop")
            .show();
    });
}
