use crate::config::PrinterConfig;
use crate::printer::{MqttMessage, PrinterState};
use anyhow::{Context, Result};
use rumqttc::{
    AsyncClient, ConnectReturnCode, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport,
};
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

/// Capacity of the internal rumqttc request channel between AsyncClient and EventLoop
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
#[derive(Debug)]
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
    event_loop_handle: JoinHandle<()>,
    /// Cached report topic for re-subscription (e.g., "device/{serial}/report")
    report_topic: String,
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
        config: &PrinterConfig,
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

        // Build topic strings before spawning so the event loop can re-subscribe
        // after reconnections. MQTT brokers discard subscriptions when
        // clean_session=true (the rumqttc default), so every reconnect needs a
        // fresh subscribe + pushall to restore the data stream.
        let report_topic = format!("device/{}/report", config.serial);
        let request_topic = format!("device/{}/request", config.serial);

        let (client, mut eventloop) = AsyncClient::new(mqtt_opts, MQTT_EVENT_QUEUE_CAPACITY);

        // Clones/moves for the spawned event-loop task
        let state_clone = Arc::clone(&state);
        let event_tx = tx;
        let event_client = client.clone();
        let event_report_topic = report_topic.clone();
        let event_request_topic = request_topic.clone();

        // Spawn event loop handler
        let event_loop_handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(connack))) => {
                        // MQTT §3.2: check the return code before treating the
                        // session as usable. A non-Success code means the broker
                        // rejected us (bad credentials, not authorised, etc.).
                        if connack.code != ConnectReturnCode::Success {
                            let _ = event_tx.try_send(MqttEvent::Error {
                                printer_index,
                                message: format!("Connection rejected: {:?}", connack.code),
                            });
                            continue;
                        }
                        {
                            let mut state_guard = state_clone.lock().expect("state lock poisoned");
                            state_guard.connected = true;
                        }
                        // Re-subscribe on every (re)connection. The broker drops
                        // all subscriptions for clean sessions, so without this
                        // the client stays connected but never receives data after
                        // a reconnect — the root cause of stale printer state.
                        let _ = event_client
                            .subscribe(&event_report_topic, QoS::AtMostOnce)
                            .await;
                        // Request full printer state and version info so we have
                        // current data immediately rather than waiting for the
                        // next periodic push.
                        for payload in [
                            r#"{"pushing":{"sequence_id":"0","command":"pushall"}}"#,
                            r#"{"info":{"sequence_id":"0","command":"get_version"}}"#,
                        ] {
                            let _ = event_client
                                .publish(&event_request_topic, QoS::AtMostOnce, false, payload)
                                .await;
                        }
                        let _ = event_tx.try_send(MqttEvent::Connected { printer_index });
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if let Ok(payload) = std::str::from_utf8(&publish.payload) {
                            if let Ok(msg) = serde_json::from_str::<MqttMessage>(payload) {
                                {
                                    let mut state_guard =
                                        state_clone.lock().expect("state lock poisoned");
                                    state_guard.update_from_message(&msg);
                                }
                                let _ =
                                    event_tx.try_send(MqttEvent::StateUpdated { printer_index });
                            }
                            // Many messages may not match our structure — that's ok
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
                        let _ = event_tx.try_send(MqttEvent::Disconnected { printer_index });
                        let _ = event_tx.try_send(MqttEvent::Error {
                            printer_index,
                            message: format!(
                                "MQTT error: {} (reconnecting in {}s)",
                                e,
                                RECONNECT_DELAY.as_secs()
                            ),
                        });
                        // Wait before reconnecting
                        tokio::time::sleep(RECONNECT_DELAY).await;
                    }
                }
            }
        });

        // Subscribe before ConnAck is processed — rumqttc queues the SUBSCRIBE
        // packet and sends it once the CONNECT handshake completes.  The ConnAck
        // handler above will re-subscribe on *re*connections (clean_session=true
        // drops subscriptions), but the initial subscribe must happen here to
        // match the timing that the printer's broker expects.
        tokio::time::timeout(
            OPERATION_TIMEOUT,
            client.subscribe(&report_topic, QoS::AtMostOnce),
        )
        .await
        .context("Subscribe operation timed out")?
        .context("Failed to subscribe to printer topic")?;

        Ok((
            Self {
                client,
                event_loop_handle,
                report_topic,
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

    /// Publishes a JSON command to the printer's request topic.
    ///
    /// Wraps the common timeout → publish → error-context pattern used by
    /// every command method. `qos` should be [`QoS::AtMostOnce`] for
    /// non-critical status requests and [`QoS::AtLeastOnce`] for
    /// user-initiated actions (pause, stop, etc.) where delivery matters.
    async fn publish_command(
        &self,
        payload: serde_json::Value,
        qos: QoS,
        action: &str,
    ) -> Result<()> {
        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client
                .publish(&self.request_topic, qos, false, payload.to_string()),
        )
        .await
        .with_context(|| format!("{action} timed out"))?
        .with_context(|| format!("Failed to {action}"))?;
        Ok(())
    }

    /// Re-subscribes to the printer's report topic and requests a full status push.
    ///
    /// Use this to manually recover from stale connections where the subscription
    /// may have been silently lost without triggering a disconnect.
    pub async fn refresh(&self) -> Result<()> {
        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client.subscribe(&self.report_topic, QoS::AtMostOnce),
        )
        .await
        .context("Subscribe operation timed out")?
        .context("Failed to re-subscribe to printer topic")?;

        self.request_full_status().await
    }

    pub async fn request_full_status(&self) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "pushing": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "pushall"
                }
            }),
            QoS::AtMostOnce,
            "request full status",
        )
        .await
    }

    /// Requests firmware/hardware version information from the printer.
    pub async fn request_version_info(&self) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "info": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "get_version"
                }
            }),
            QoS::AtMostOnce,
            "request version info",
        )
        .await
    }

    /// Sets the print speed level on the printer.
    ///
    /// # Arguments
    /// * `level` - Speed level: 1=Silent, 2=Standard, 3=Sport, 4=Ludicrous
    pub async fn set_speed_level(&self, level: u8) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "print": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "print_speed",
                    "param": level.to_string()
                }
            }),
            QoS::AtLeastOnce,
            "set speed level",
        )
        .await
    }

    /// Sets the chamber light on or off.
    pub async fn set_chamber_light(&self, on: bool) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "system": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "ledctrl",
                    "led_node": "chamber_light",
                    "led_mode": if on { "on" } else { "off" }
                }
            }),
            QoS::AtLeastOnce,
            "set chamber light",
        )
        .await
    }

    /// Sets the work light on or off.
    pub async fn set_work_light(&self, on: bool) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "system": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "ledctrl",
                    "led_node": "work_light",
                    "led_mode": if on { "on" } else { "off" }
                }
            }),
            QoS::AtLeastOnce,
            "set work light",
        )
        .await
    }

    /// Pauses the current print job.
    pub async fn pause_print(&self) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "print": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "pause"
                }
            }),
            QoS::AtLeastOnce,
            "pause print",
        )
        .await
    }

    /// Resumes a paused print job.
    pub async fn resume_print(&self) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "print": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "resume"
                }
            }),
            QoS::AtLeastOnce,
            "resume print",
        )
        .await
    }

    /// Stops/cancels the current print job.
    pub async fn stop_print(&self) -> Result<()> {
        self.publish_command(
            serde_json::json!({
                "print": {
                    "sequence_id": self.next_sequence_id(),
                    "command": "stop"
                }
            }),
            QoS::AtLeastOnce,
            "stop print",
        )
        .await
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
        self.event_loop_handle.abort();
    }
}
