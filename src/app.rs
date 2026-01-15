use crate::mqtt::MqttEvent;
use crate::printer::PrinterState;
use std::time::{Duration, Instant};

pub struct App {
    pub printer_state: PrinterState,
    pub connected: bool,
    pub last_update: Option<Instant>,
    pub error_message: Option<String>,
    pub should_quit: bool,
    pub auto_refresh: bool,
}

impl App {
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
                self.printer_state = state;
                self.printer_state.connected = true;
                self.last_update = Some(Instant::now());
            }
            MqttEvent::Error(msg) => {
                self.error_message = Some(msg);
            }
        }
    }

    pub fn time_since_update(&self) -> Option<Duration> {
        self.last_update.map(|t| t.elapsed())
    }

    pub fn status_text(&self) -> &str {
        if !self.connected {
            "Disconnected"
        } else {
            match self.printer_state.gcode_state.as_str() {
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
