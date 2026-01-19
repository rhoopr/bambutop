use crate::config::PrinterConfig;
use crate::printer::{MqttMessage, PrinterState};
use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
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
/// Note: Adding new variants is a breaking change for exhaustive matches.
#[non_exhaustive]
pub enum MqttEvent {
    /// Successfully connected to the MQTT broker
    Connected,
    /// Disconnected from the MQTT broker
    Disconnected,
    /// Printer state has been updated (read from shared state)
    StateUpdated,
    /// An error occurred
    Error(String),
}

/// Shared printer state that can be accessed by both the MQTT task and the UI.
pub type SharedPrinterState = Arc<Mutex<PrinterState>>;

pub struct MqttClient {
    client: AsyncClient,
    config: PrinterConfig,
    /// Handle to the background event loop task for graceful shutdown
    _event_loop_handle: JoinHandle<()>,
}

impl MqttClient {
    /// Connects to the printer's MQTT broker and starts the event loop.
    ///
    /// Returns the client, a shared printer state, and a receiver for MQTT events.
    /// The shared state is updated directly by the MQTT task, eliminating the need
    /// to clone the entire state on every message.
    pub async fn connect(
        config: PrinterConfig,
    ) -> Result<(Self, SharedPrinterState, mpsc::Receiver<MqttEvent>)> {
        let (tx, rx) = mpsc::channel(100);

        // Create shared state
        let state = Arc::new(Mutex::new(PrinterState::default()));

        // Initialize model from serial
        {
            let mut state_guard = state.lock().expect("state lock poisoned");
            state_guard.set_model_from_serial(&config.serial);
        }

        let client_id = format!("bambutop_{}", std::process::id());
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
                        let _ = tx_clone.send(MqttEvent::Connected).await;
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if let Ok(payload) = std::str::from_utf8(&publish.payload) {
                            if let Ok(msg) = serde_json::from_str::<MqttMessage>(payload) {
                                {
                                    let mut state_guard =
                                        state_clone.lock().expect("state lock poisoned");
                                    state_guard.update_from_message(&msg);
                                }
                                let _ = tx_clone.send(MqttEvent::StateUpdated).await;
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
                        let _ = tx_clone.send(MqttEvent::Disconnected).await;
                        let _ = tx_clone
                            .send(MqttEvent::Error(format!("MQTT error: {}", e)))
                            .await;
                        // Wait before reconnecting
                        tokio::time::sleep(RECONNECT_DELAY).await;
                    }
                }
            }
        });

        // Subscribe to printer reports
        let topic = format!("device/{}/report", config.serial);
        tokio::time::timeout(OPERATION_TIMEOUT, client.subscribe(&topic, QoS::AtMostOnce))
            .await
            .context("Subscribe operation timed out")?
            .context("Failed to subscribe to printer topic")?;

        Ok((
            Self {
                client,
                config,
                _event_loop_handle: event_loop_handle,
            },
            state,
            rx,
        ))
    }

    pub async fn request_full_status(&self) -> Result<()> {
        let topic = format!("device/{}/request", self.config.serial);
        let payload = serde_json::json!({
            "pushing": {
                "sequence_id": "0",
                "command": "pushall"
            }
        });

        tokio::time::timeout(
            OPERATION_TIMEOUT,
            self.client
                .publish(&topic, QoS::AtMostOnce, false, payload.to_string()),
        )
        .await
        .context("Publish operation timed out")?
        .context("Failed to request full status")?;

        Ok(())
    }
}

impl Drop for MqttClient {
    fn drop(&mut self) {
        // Abort the event loop task on drop for clean shutdown
        self._event_loop_handle.abort();
    }
}
