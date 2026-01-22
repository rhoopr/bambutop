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

/// Internal connection data for a single printer within `MultiMqttClient`.
struct PrinterConnection {
    client: AsyncClient,
    /// Handle to the background event loop task for graceful shutdown
    event_loop_handle: JoinHandle<()>,
    /// Cached request topic to avoid repeated format! allocations
    request_topic: String,
    /// Atomic counter for generating unique sequence IDs for MQTT commands
    sequence_id: AtomicU64,
    /// Shared state for this printer
    state: SharedPrinterState,
}

impl PrinterConnection {
    /// Generates the next unique sequence ID for MQTT commands.
    fn next_sequence_id(&self) -> String {
        self.sequence_id.fetch_add(1, Ordering::Relaxed).to_string()
    }

    /// Sends a disconnect message to the MQTT broker.
    async fn disconnect(&self) {
        let _ = tokio::time::timeout(Duration::from_secs(2), self.client.disconnect()).await;
    }

    /// Abort the event loop task
    fn abort(&self) {
        self.event_loop_handle.abort();
    }
}

/// MQTT client for a single printer connection.
///
/// This maintains backward compatibility with the original single-printer design.
pub struct MqttClient {
    client: AsyncClient,
    /// Handle to the background event loop task for graceful shutdown
    _event_loop_handle: JoinHandle<()>,
    /// Cached request topic to avoid repeated format! allocations
    request_topic: String,
    /// Atomic counter for generating unique sequence IDs for MQTT commands
    sequence_id: AtomicU64,
    /// Printer index (default 0 for single-printer backward compatibility)
    printer_index: usize,
}

impl MqttClient {
    /// Connects to the printer's MQTT broker and starts the event loop.
    ///
    /// Returns the client, a shared printer state, and a receiver for MQTT events.
    /// The shared state is updated directly by the MQTT task, eliminating the need
    /// to clone the entire state on every message.
    ///
    /// This method uses printer_index 0 for backward compatibility with single-printer setups.
    pub async fn connect(
        config: PrinterConfig,
    ) -> Result<(Self, SharedPrinterState, mpsc::Receiver<MqttEvent>)> {
        Self::connect_with_index(config, 0).await
    }

    /// Connects to the printer's MQTT broker with a specific printer index.
    ///
    /// Returns the client, a shared printer state, and a receiver for MQTT events.
    /// The printer_index is included in all events to identify which printer they relate to.
    pub async fn connect_with_index(
        config: PrinterConfig,
        printer_index: usize,
    ) -> Result<(Self, SharedPrinterState, mpsc::Receiver<MqttEvent>)> {
        let (tx, rx) = mpsc::channel(100);

        // Create shared state
        let state = Arc::new(Mutex::new(PrinterState::default()));

        // Initialize model from serial
        {
            let mut state_guard = state.lock().expect("state lock poisoned");
            state_guard.set_model_from_serial(&config.serial);
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

        let (client, mut eventloop) = AsyncClient::new(mqtt_opts, 10);

        // Clone for the spawned task
        let state_clone = Arc::clone(&state);
        let tx_clone = tx.clone();

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
                printer_index,
            },
            state,
            rx,
        ))
    }

    /// Returns the printer index associated with this client.
    pub fn printer_index(&self) -> usize {
        self.printer_index
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

/// Manages multiple MQTT client connections for multiple printers.
///
/// Each printer has its own independent connection, event loop, and state.
/// Events from all printers are multiplexed onto a single receiver channel.
pub struct MultiMqttClient {
    /// Active connections indexed by printer index
    connections: Vec<Option<PrinterConnection>>,
    /// Sender for events (cloned for each connection's event loop)
    event_tx: mpsc::Sender<MqttEvent>,
    /// Receiver for events from all connections
    event_rx: Option<mpsc::Receiver<MqttEvent>>,
}

impl MultiMqttClient {
    /// Creates a new multi-client manager with capacity for the specified number of printers.
    ///
    /// The event receiver can be taken with `take_event_receiver()` after creation.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(100 * capacity.max(1));
        let mut connections = Vec::with_capacity(capacity);
        connections.resize_with(capacity, || None);
        Self {
            connections,
            event_tx: tx,
            event_rx: Some(rx),
        }
    }

    /// Takes the event receiver for use in the main application loop.
    ///
    /// This can only be called once; subsequent calls return `None`.
    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<MqttEvent>> {
        self.event_rx.take()
    }

    /// Returns the number of printer slots in this manager.
    pub fn capacity(&self) -> usize {
        self.connections.len()
    }

    /// Returns `true` if the printer at the given index is currently connected.
    pub fn is_connected(&self, index: usize) -> bool {
        self.connections
            .get(index)
            .and_then(|c| c.as_ref())
            .is_some()
    }

    /// Returns the shared state for the printer at the given index, if connected.
    pub fn get_state(&self, index: usize) -> Option<SharedPrinterState> {
        self.connections
            .get(index)
            .and_then(|c| c.as_ref())
            .map(|conn| Arc::clone(&conn.state))
    }

    /// Returns the shared states for all connected printers.
    ///
    /// Returns a Vec of (printer_index, SharedPrinterState) tuples.
    pub fn get_all_states(&self) -> Vec<(usize, SharedPrinterState)> {
        self.connections
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|conn| (i, Arc::clone(&conn.state))))
            .collect()
    }

    /// Connects to a single printer at the specified index.
    ///
    /// If a connection already exists at this index, it is disconnected first.
    pub async fn connect(&mut self, index: usize, config: PrinterConfig) -> Result<()> {
        // Disconnect existing connection if any
        self.disconnect(index).await;

        // Ensure we have capacity
        if index >= self.connections.len() {
            self.connections.resize_with(index + 1, || None);
        }

        let state = Arc::new(Mutex::new(PrinterState::default()));

        // Initialize model from serial
        {
            let mut state_guard = state.lock().expect("state lock poisoned");
            state_guard.set_model_from_serial(&config.serial);
        }

        let client_id = format!("bambutop_{}_{}", std::process::id(), index);
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

        let (client, mut eventloop) = AsyncClient::new(mqtt_opts, 10);

        // Clone for the spawned task
        let state_clone = Arc::clone(&state);
        let tx_clone = self.event_tx.clone();
        let printer_index = index;

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

        // Cache the request topic
        let request_topic = format!("device/{}/request", config.serial);

        self.connections[index] = Some(PrinterConnection {
            client,
            event_loop_handle,
            request_topic,
            sequence_id: AtomicU64::new(1),
            state,
        });

        Ok(())
    }

    /// Connects to all printers from a list of configurations.
    ///
    /// Returns a Vec of results for each connection attempt, indexed by printer index.
    pub async fn connect_all(&mut self, configs: &[PrinterConfig]) -> Vec<Result<()>> {
        let mut results = Vec::with_capacity(configs.len());

        for (index, config) in configs.iter().enumerate() {
            results.push(self.connect(index, config.clone()).await);
        }

        results
    }

    /// Disconnects from a specific printer.
    pub async fn disconnect(&mut self, index: usize) {
        if let Some(Some(conn)) = self.connections.get_mut(index) {
            conn.disconnect().await;
            conn.abort();
        }
        if index < self.connections.len() {
            self.connections[index] = None;
        }
    }

    /// Disconnects from all printers.
    pub async fn disconnect_all(&mut self) {
        for index in 0..self.connections.len() {
            self.disconnect(index).await;
        }
    }

    /// Requests full status from a specific printer.
    pub async fn request_full_status(&self, index: usize) -> Result<()> {
        let conn = self
            .connections
            .get(index)
            .and_then(|c| c.as_ref())
            .context("Printer not connected")?;

        let payload = serde_json::json!({
            "pushing": {
                "sequence_id": conn.next_sequence_id(),
                "command": "pushall"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            conn.client.publish(
                &conn.request_topic,
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

    /// Requests full status from all connected printers.
    pub async fn request_all_full_status(&self) -> Vec<(usize, Result<()>)> {
        let mut results = Vec::new();

        for (index, conn) in self.connections.iter().enumerate() {
            if conn.is_some() {
                results.push((index, self.request_full_status(index).await));
            }
        }

        results
    }

    /// Sets the print speed level on a specific printer.
    ///
    /// # Arguments
    /// * `index` - Printer index
    /// * `level` - Speed level: 1=Silent, 2=Standard, 3=Sport, 4=Ludicrous
    pub async fn set_speed_level(&self, index: usize, level: u8) -> Result<()> {
        let conn = self
            .connections
            .get(index)
            .and_then(|c| c.as_ref())
            .context("Printer not connected")?;

        let payload = serde_json::json!({
            "print": {
                "sequence_id": conn.next_sequence_id(),
                "command": "print_speed",
                "param": level.to_string()
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            conn.client.publish(
                &conn.request_topic,
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

    /// Sets the chamber light on or off for a specific printer.
    pub async fn set_chamber_light(&self, index: usize, on: bool) -> Result<()> {
        let conn = self
            .connections
            .get(index)
            .and_then(|c| c.as_ref())
            .context("Printer not connected")?;

        let mode = if on { "on" } else { "off" };
        let payload = serde_json::json!({
            "system": {
                "sequence_id": conn.next_sequence_id(),
                "command": "ledctrl",
                "led_node": "chamber_light",
                "led_mode": mode
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            conn.client.publish(
                &conn.request_topic,
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

    /// Pauses the current print job on a specific printer.
    pub async fn pause_print(&self, index: usize) -> Result<()> {
        let conn = self
            .connections
            .get(index)
            .and_then(|c| c.as_ref())
            .context("Printer not connected")?;

        let payload = serde_json::json!({
            "print": {
                "sequence_id": conn.next_sequence_id(),
                "command": "pause"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            conn.client.publish(
                &conn.request_topic,
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

    /// Resumes a paused print job on a specific printer.
    pub async fn resume_print(&self, index: usize) -> Result<()> {
        let conn = self
            .connections
            .get(index)
            .and_then(|c| c.as_ref())
            .context("Printer not connected")?;

        let payload = serde_json::json!({
            "print": {
                "sequence_id": conn.next_sequence_id(),
                "command": "resume"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            conn.client.publish(
                &conn.request_topic,
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

    /// Stops/cancels the current print job on a specific printer.
    pub async fn stop_print(&self, index: usize) -> Result<()> {
        let conn = self
            .connections
            .get(index)
            .and_then(|c| c.as_ref())
            .context("Printer not connected")?;

        let payload = serde_json::json!({
            "print": {
                "sequence_id": conn.next_sequence_id(),
                "command": "stop"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            conn.client.publish(
                &conn.request_topic,
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
}

impl Drop for MultiMqttClient {
    fn drop(&mut self) {
        // Abort all event loop tasks on drop for clean shutdown
        for conn in self.connections.iter().flatten() {
            conn.abort();
        }
    }
}
