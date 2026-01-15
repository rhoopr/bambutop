use crate::config::PrinterConfig;
use crate::printer::{MqttMessage, PrinterState};
use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

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

pub enum MqttEvent {
    Connected,
    Disconnected,
    StateUpdate(PrinterState),
    Error(String),
}

pub struct MqttClient {
    client: AsyncClient,
    config: PrinterConfig,
}

impl MqttClient {
    pub async fn connect(config: PrinterConfig) -> Result<(Self, mpsc::Receiver<MqttEvent>)> {
        let (tx, rx) = mpsc::channel(100);

        let client_id = format!("bambutop_{}", std::process::id());
        let mut mqtt_opts = MqttOptions::new(&client_id, &config.ip, config.port);

        mqtt_opts.set_credentials("bblp", &config.access_code);
        mqtt_opts.set_keep_alive(Duration::from_secs(30));

        // Configure TLS - Bambu printers use self-signed certs, so we skip verification
        let tls_config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();

        mqtt_opts.set_transport(Transport::tls_with_config(
            TlsConfiguration::Rustls(Arc::new(tls_config)),
        ));

        let (client, mut eventloop) = AsyncClient::new(mqtt_opts, 10);

        let tx_clone = tx.clone();

        // Spawn event loop handler
        let serial_for_model = config.serial.clone();
        tokio::spawn(async move {
            let mut state = PrinterState::default();
            state.set_model_from_serial(&serial_for_model);

            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        state.connected = true;
                        let _ = tx_clone.send(MqttEvent::Connected).await;
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if let Ok(payload) = std::str::from_utf8(&publish.payload) {
                            match serde_json::from_str::<MqttMessage>(payload) {
                                Ok(msg) => {
                                    state.update_from_message(&msg);
                                    let _ = tx_clone
                                        .send(MqttEvent::StateUpdate(state.clone()))
                                        .await;
                                }
                                Err(_) => {
                                    // Many messages may not match our structure - that's ok
                                }
                            }
                        }
                    }
                    Ok(Event::Incoming(Packet::SubAck(_))) => {
                        // Successfully subscribed
                    }
                    Ok(_) => {}
                    Err(e) => {
                        state.connected = false;
                        let _ = tx_clone.send(MqttEvent::Disconnected).await;
                        let _ = tx_clone
                            .send(MqttEvent::Error(format!("MQTT error: {}", e)))
                            .await;
                        // Wait before reconnecting
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        // Subscribe to printer reports
        let topic = format!("device/{}/report", config.serial);
        client
            .subscribe(&topic, QoS::AtMostOnce)
            .await
            .context("Failed to subscribe to printer topic")?;

        Ok((
            Self {
                client,
                config,
            },
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

        self.client
            .publish(&topic, QoS::AtMostOnce, false, payload.to_string())
            .await
            .context("Failed to request full status")?;

        Ok(())
    }
}
