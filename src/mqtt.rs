use crate::config::PrinterConfig;
use crate::printer::{MqttMessage, PrinterState};
use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// MQTT keepalive interval in seconds
const KEEPALIVE_SECS: u64 = 30;

/// Delay before attempting to reconnect after a connection error
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Timeout for MQTT operations (subscribe, publish)
const OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Default channel capacity when creating a standalone event channel (no shared sender)
const FALLBACK_CHANNEL_CAPACITY: usize = 100;

/// Capacity of the internal rumqttc event queue for buffering incoming MQTT packets
const MQTT_EVENT_QUEUE_CAPACITY: usize = 10;

/// Certificate verifier that accepts any certificate (for self-signed Bambu certs)
#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Events sent from the MQTT background task to the main application.
///
/// All events include a `printer_index` to identify which printer the event relates to.
/// Note: Adding new variants is a breaking change for exhaustive matches.
#[non_exhaustive]
pub enum MqttEvent {
    /// Successfully connected to the MQTT broker for a specific printer
    Connected { printer_index: usize },
    /// Disconnected from the MQTT broker for a specific printer
    Disconnected { printer_index: usize },
    /// Printer state has been updated (read from shared state)
    StateUpdated { printer_index: usize },
    /// An error occurred for a specific printer
    Error {
        printer_index: usize,
        message: String,
    },
}

/// Shared printer state that can be accessed by both the MQTT task and the UI.
pub type SharedPrinterState = Arc<Mutex<PrinterState>>;

/// MQTT client for a single printer connection.
pub struct MqttClient {
    client: AsyncClient,
    /// Handle to the background event loop task for graceful shutdown
    _event_loop_handle: JoinHandle<()>,
    /// Cached request topic to avoid repeated format! allocations
    request_topic: String,
    /// Atomic counter for generating unique sequence IDs for MQTT commands
    sequence_id: AtomicU64,
}

impl MqttClient {
    /// Connects to a printer's MQTT broker and starts the event loop.
    ///
    /// Returns the client, a shared printer state, and a receiver for MQTT events.
    /// The shared state is updated directly by the MQTT task, eliminating the need
    /// to clone the entire state on every message.
    ///
    /// # Arguments
    /// * `config` - Printer connection configuration
    /// * `printer_index` - Index of this printer (for multi-printer support)
    /// * `event_tx` - Optional shared event sender. If provided, events are sent to this
    ///   channel instead of creating a new one. This allows aggregating events from
    ///   multiple printers into a single channel.
    ///
    /// # Returns
    /// Returns the client, shared printer state, and optionally a new receiver if
    /// `event_tx` was None.
    pub async fn connect(
        config: PrinterConfig,
        printer_index: usize,
        event_tx: Option<mpsc::Sender<MqttEvent>>,
    ) -> Result<(Self, SharedPrinterState, Option<mpsc::Receiver<MqttEvent>>)> {
        // Use provided sender or create a new channel
        let (tx, rx) = match event_tx {
            Some(sender) => (sender, None),
            None => {
                let (sender, receiver) = mpsc::channel(FALLBACK_CHANNEL_CAPACITY);
                (sender, Some(receiver))
            }
        };

        // Create shared state
        let state = Arc::new(Mutex::new(PrinterState::default()));

        // Initialize state from config
        {
            let mut state_guard = state.lock().expect("state lock poisoned");
            state_guard.set_model_from_serial(&config.serial);
            // Set config name if provided
            if let Some(name) = &config.name {
                state_guard.printer_name.clone_from(name);
            }
        }

        let client_id = format!("bambutop_{}_{}", std::process::id(), printer_index);
        let mut mqtt_opts = MqttOptions::new(&client_id, &config.ip, config.port);

        mqtt_opts.set_credentials("bblp", &config.access_code);
        mqtt_opts.set_keep_alive(Duration::from_secs(KEEPALIVE_SECS));

        // Configure TLS - Bambu printers use self-signed certs, so we skip verification
        let tls_config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();

        mqtt_opts.set_transport(Transport::tls_with_config(TlsConfiguration::Rustls(
            Arc::new(tls_config),
        )));

        let (client, mut eventloop) = AsyncClient::new(mqtt_opts, MQTT_EVENT_QUEUE_CAPACITY);

        // Clone for the spawned task
        let state_clone = Arc::clone(&state);
        let tx_clone = tx;

        // Spawn event loop handler
        let event_loop_handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        {
                            let mut state_guard = state_clone.lock().expect("state lock poisoned");
                            state_guard.connected = true;
                        }
                        let _ = tx_clone.send(MqttEvent::Connected { printer_index }).await;
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if let Ok(payload) = std::str::from_utf8(&publish.payload) {
                            if let Ok(msg) = serde_json::from_str::<MqttMessage>(payload) {
                                {
                                    let mut state_guard =
                                        state_clone.lock().expect("state lock poisoned");
                                    state_guard.update_from_message(&msg);
                                }
                                let _ = tx_clone
                                    .send(MqttEvent::StateUpdated { printer_index })
                                    .await;
                            }
                            // Many messages may not match our structure - that's ok
                        }
                    }
                    Ok(Event::Incoming(Packet::SubAck(_))) => {
                        // Successfully subscribed
                    }
                    Ok(_) => {}
                    Err(e) => {
                        {
                            let mut state_guard = state_clone.lock().expect("state lock poisoned");
                            state_guard.connected = false;
                        }
                        let _ = tx_clone
                            .send(MqttEvent::Disconnected { printer_index })
                            .await;
                        let _ = tx_clone
                            .send(MqttEvent::Error {
                                printer_index,
                                message: format!(
                                    "MQTT error: {} (reconnecting in {}s)",
                                    e,
                                    RECONNECT_DELAY.as_secs()
                                ),
                            })
                            .await;
                        // Wait before reconnecting
                        tokio::time::sleep(RECONNECT_DELAY).await;
                    }
                }
            }
        });

        // Subscribe to printer reports
        let report_topic = format!("device/{}/report", config.serial);
        tokio::time::timeout(
            OPERATION_TIMEOUT,
            client.subscribe(&report_topic, QoS::AtMostOnce),
        )
        .await
        .context("Subscribe operation timed out")?
        .context("Failed to subscribe to printer topic")?;

        // Cache the request topic to avoid repeated format! allocations
        let request_topic = format!("device/{}/request", config.serial);

        Ok((
            Self {
                client,
                _event_loop_handle: event_loop_handle,
                request_topic,
                sequence_id: AtomicU64::new(1),
            },
            state,
            rx,
        ))
    }

    /// Generates the next unique sequence ID for MQTT commands.
    ///
    /// Sequence IDs are monotonically increasing values used to correlate
    /// requests with responses from the printer.
    fn next_sequence_id(&self) -> String {
        self.sequence_id.fetch_add(1, Ordering::Relaxed).to_string()
    }

    pub async fn request_full_status(&self) -> Result<()> {
        let payload = serde_json::json!({
            "pushing": {
                "sequence_id": self.next_sequence_id(),
                "command": "pushall"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Publish operation timed out")?
        .context("Failed to request full status")?;

        Ok(())
    }

    /// Requests firmware/hardware version information from the printer.
    pub async fn request_version_info(&self) -> Result<()> {
        let payload = serde_json::json!({
            "info": {
                "sequence_id": self.next_sequence_id(),
                "command": "get_version"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Publish operation timed out")?
        .context("Failed to request version info")?;

        Ok(())
    }

    /// Sets the print speed level on the printer.
    ///
    /// # Arguments
    /// * `level` - Speed level: 1=Silent, 2=Standard, 3=Sport, 4=Ludicrous
    pub async fn set_speed_level(&self, level: u8) -> Result<()> {
        let payload = serde_json::json!({
            "print": {
                "sequence_id": self.next_sequence_id(),
                "command": "print_speed",
                "param": level.to_string()
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Set speed operation timed out")?
        .context("Failed to set speed level")?;

        Ok(())
    }

    /// Sets the chamber light on or off.
    ///
    /// # Arguments
    /// * `on` - true to turn the light on, false to turn it off
    pub async fn set_chamber_light(&self, on: bool) -> Result<()> {
        let mode = if on { "on" } else { "off" };
        let payload = serde_json::json!({
            "system": {
                "sequence_id": self.next_sequence_id(),
                "command": "ledctrl",
                "led_node": "chamber_light",
                "led_mode": mode
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Set chamber light operation timed out")?
        .context("Failed to set chamber light")?;

        Ok(())
    }

    /// Sets the work light on or off.
    ///
    /// # Arguments
    /// * `on` - true to turn the light on, false to turn it off
    pub async fn set_work_light(&self, on: bool) -> Result<()> {
        let mode = if on { "on" } else { "off" };
        let payload = serde_json::json!({
            "system": {
                "sequence_id": self.next_sequence_id(),
                "command": "ledctrl",
                "led_node": "work_light",
                "led_mode": mode
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Set work light operation timed out")?
        .context("Failed to set work light")?;

        Ok(())
    }

    /// Pauses the current print job.
    pub async fn pause_print(&self) -> Result<()> {
        let payload = serde_json::json!({
            "print": {
                "sequence_id": self.next_sequence_id(),
                "command": "pause"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Pause print operation timed out")?
        .context("Failed to pause print")?;

        Ok(())
    }

    /// Resumes a paused print job.
    pub async fn resume_print(&self) -> Result<()> {
        let payload = serde_json::json!({
            "print": {
                "sequence_id": self.next_sequence_id(),
                "command": "resume"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Resume print operation timed out")?
        .context("Failed to resume print")?;

        Ok(())
    }

    /// Stops/cancels the current print job.
    pub async fn stop_print(&self) -> Result<()> {
        let payload = serde_json::json!({
            "print": {
                "sequence_id": self.next_sequence_id(),
                "command": "stop"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.publish(
                &self.request_topic,
                QoS::AtMostOnce,
                false,
                payload.to_string(),
            ),
        )
        .await
        .context("Stop print operation timed out")?
        .context("Failed to stop print")?;

        Ok(())
    }

    /// Sends a disconnect message to the MQTT broker.
    ///
    /// This should be called before dropping the client for a clean shutdown.
    /// If the disconnect fails or times out, it is logged but not treated as an error
    /// since we're shutting down anyway.
    pub async fn disconnect(&self) {
        // Try to disconnect gracefully with a short timeout
        let _ = tokio::time::timeout(Duration::from_secs(2), self.client.disconnect()).await;
    }
}

impl Drop for MqttClient {
    fn drop(&mut self) {
        // Abort the event loop task on drop for clean shutdown
        self._event_loop_handle.abort();
    }
}
