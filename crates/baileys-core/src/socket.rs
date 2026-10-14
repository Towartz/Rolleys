use baileys_binary::{BinaryEncoder, BinaryNode};
use baileys_crypto::generate_key_pair;
use baileys_noise::NoiseHandler;
use baileys_proto::proto::{
    handshake_message::{ClientFinish, ClientHello},
    HandshakeMessage,
};
use baileys_store::AuthCreds;
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, trace, warn};

use crate::config::BaileysConfig;
use crate::registration::{generate_login_node, generate_registration_node};
use crate::CoreError;

pub struct WhatsAppSocket {
    outgoing_tx: mpsc::Sender<BinaryNode>,
    writer_handle: tokio::task::AbortHandle,
    reader_handle: tokio::task::AbortHandle,
}

impl Drop for WhatsAppSocket {
    fn drop(&mut self) {
        self.writer_handle.abort();
        self.reader_handle.abort();
    }
}

impl WhatsAppSocket {
    /// Connects to the WhatsApp Web Gateway, executes the Noise_XX handshake, and returns a socket handle along with an incoming node receiver.
    pub async fn connect(
        creds: &AuthCreds,
        config: &BaileysConfig,
    ) -> Result<(Self, mpsc::Receiver<BinaryNode>), CoreError> {
        debug!(
            "Connecting to WhatsApp Web Gateway at '{}'...",
            config.wa_websocket_url
        );

        let (ws_stream, response) = if config.wa_websocket_url.starts_with("mock://") {
            connect_async(&config.wa_websocket_url)
                .await
                .map_err(|e| CoreError::Network(format!("WebSocket connection failed: {}", e)))?
        } else {
            use tokio_tungstenite::tungstenite::client::IntoClientRequest;
            let mut request = config
                .wa_websocket_url
                .as_str()
                .into_client_request()
                .map_err(|e| CoreError::Network(format!("Invalid URL: {}", e)))?;
            request
                .headers_mut()
                .insert("Origin", "https://web.whatsapp.com".parse().unwrap());
            request.headers_mut().insert(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36".parse().unwrap(),
            );
            connect_async(request)
                .await
                .map_err(|e| CoreError::Network(format!("WebSocket connection failed: {}", e)))?
        };

        debug!("WebSocket handshake status: {}", response.status());

        let (mut write_half, mut read_half) = ws_stream.split();

        // 1. Initialize NoiseHandler with fresh ephemeral key
        let ephemeral = generate_key_pair();
        let mut noise = NoiseHandler::new(ephemeral.clone(), None);

        // 2. Send ClientHello
        let hello_msg = HandshakeMessage {
            client_hello: Some(ClientHello {
                ephemeral: Some(ephemeral.public_key.to_vec()),
                ..Default::default()
            }),
            server_hello: None,
            client_finish: None,
        };

        let hello_bytes = hello_msg.encode_to_vec();
        let framed_hello = noise
            .encode_frame(&hello_bytes)
            .map_err(|e| CoreError::Noise(e.to_string()))?;

        write_half
            .send(WsMessage::Binary(framed_hello))
            .await
            .map_err(|e| CoreError::Network(format!("Failed to send ClientHello: {}", e)))?;

        trace!("Sent Noise ClientHello (len={})", hello_bytes.len());

        // 3. Receive ServerHello
        let server_msg = read_half
            .next()
            .await
            .ok_or_else(|| {
                CoreError::Network(
                    "Connection closed by server while waiting for ServerHello".into(),
                )
            })?
            .map_err(|e| CoreError::Network(format!("Error reading ServerHello: {}", e)))?;

        let server_data = match server_msg {
            WsMessage::Binary(b) => b,
            _ => {
                return Err(CoreError::Network(
                    "Unexpected non-binary message for ServerHello".into(),
                ))
            }
        };

        if server_data.len() < 3 {
            return Err(CoreError::Noise("ServerHello frame too short".into()));
        }

        // Strip 3-byte big-endian frame length
        let server_handshake_bytes = &server_data[3..];
        let server_handshake = HandshakeMessage::decode(server_handshake_bytes).map_err(|e| {
            CoreError::Noise(format!(
                "Failed to decode ServerHello HandshakeMessage: {}",
                e
            ))
        })?;

        let server_hello = server_handshake
            .server_hello
            .ok_or_else(|| CoreError::Noise("HandshakeMessage missing ServerHello".into()))?;

        // 4. Process ServerHello, authenticate certificate chain, and compute DH shared keys
        let server_ephemeral = server_hello
            .ephemeral
            .as_deref()
            .ok_or_else(|| CoreError::Noise("Missing ephemeral key in ServerHello".into()))?;
        let server_static = server_hello
            .r#static
            .as_deref()
            .ok_or_else(|| CoreError::Noise("Missing static field in ServerHello".into()))?;
        let server_payload = server_hello
            .payload
            .as_deref()
            .ok_or_else(|| CoreError::Noise("Missing payload in ServerHello".into()))?;

        use baileys_crypto::KeyPair;
        let noise_key_pair =
            KeyPair::try_from(&creds.noise_key).map_err(|e| CoreError::Crypto(e.to_string()))?;

        let key_enc = noise
            .process_handshake(
                server_ephemeral,
                server_static,
                server_payload,
                &noise_key_pair,
            )
            .map_err(|e| CoreError::Noise(e.to_string()))?;

        debug!("Server certificate chain verified successfully!");

        // 5. Build ClientPayload (registration or login) and encrypt it
        let client_payload = if let Some(me) = &creds.me_jid {
            debug!("Existing session detected, logging in as '{}'...", me);
            generate_login_node(me, config)
        } else {
            debug!("No existing session, generating registration payload for companion pairing...");
            generate_registration_node(creds, config)
        };

        let payload_bytes = client_payload.encode_to_vec();
        let payload_enc = noise
            .encrypt(&payload_bytes)
            .map_err(|e| CoreError::Noise(e.to_string()))?;

        // 6. Send ClientFinish
        let finish_msg = HandshakeMessage {
            client_hello: None,
            server_hello: None,
            client_finish: Some(ClientFinish {
                r#static: Some(key_enc),
                payload: Some(payload_enc),
                ..Default::default()
            }),
        };

        let finish_bytes = finish_msg.encode_to_vec();
        let framed_finish = noise
            .encode_frame(&finish_bytes)
            .map_err(|e| CoreError::Noise(e.to_string()))?;

        write_half
            .send(WsMessage::Binary(framed_finish))
            .await
            .map_err(|e| CoreError::Network(format!("Failed to send ClientFinish: {}", e)))?;

        // 7. Transition Noise session to authenticated transport state
        noise
            .finish_init()
            .map_err(|e| CoreError::Noise(e.to_string()))?;

        debug!("Noise_XX Handshake complete! Transport cipher active.");

        // 8. Setup channels for transport message passing
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<BinaryNode>(64);
        let (incoming_tx, incoming_rx) = mpsc::channel::<BinaryNode>(128);
        let (pong_tx, mut pong_rx) = mpsc::channel::<Vec<u8>>(32);

        let noise_arc = Arc::new(Mutex::new(noise));
        let noise_writer = Arc::clone(&noise_arc);
        let noise_reader = Arc::clone(&noise_arc);

        let writer_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(pong_data) = pong_rx.recv() => {
                        trace!("Sending WebSocket Pong response (len={})", pong_data.len());
                        if let Err(e) = write_half.send(WsMessage::Pong(pong_data)).await {
                            error!("Failed to send Pong: {}", e);
                            break;
                        }
                    }
                    Some(node) = outgoing_rx.recv() => {
                        trace!("Encoding outgoing node: <{}>", node.tag);
                        let encoded_node = match BinaryEncoder::encode(&node) {
                            Ok(b) => b,
                            Err(e) => {
                                error!("Failed to encode outgoing BinaryNode: {}", e);
                                continue;
                            }
                        };

                        let framed = {
                            let mut lock = noise_writer.lock().await;
                            match lock.encode_frame(&encoded_node) {
                                Ok(f) => f,
                                Err(e) => {
                                    error!("Failed to encrypt/frame outgoing node: {}", e);
                                    continue;
                                }
                            }
                        };

                        if let Err(e) = write_half.send(WsMessage::Binary(framed)).await {
                            error!("WebSocket send failed: {}", e);
                            break;
                        }
                    }
                    else => break,
                }
            }
        });

        // Reader task
        let reader_task = tokio::spawn(async move {
            let mut in_bytes: Vec<u8> = Vec::new();

            while let Some(msg_result) = read_half.next().await {
                let msg = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("WebSocket read error: {}", e);
                        break;
                    }
                };

                let data = match msg {
                    WsMessage::Binary(b) => b,
                    WsMessage::Ping(p) => {
                        trace!("Received WebSocket Ping (len={}), sending Pong", p.len());
                        let _ = pong_tx.send(p).await;
                        continue;
                    }
                    WsMessage::Pong(_) => {
                        trace!("Received WebSocket Pong");
                        continue;
                    }
                    WsMessage::Close(c) => {
                        warn!("Server closed WebSocket: {:?}", c);
                        break;
                    }
                    _ => continue,
                };

                in_bytes.extend_from_slice(&data);

                // Extract all complete 3-byte framed chunks from buffer
                while in_bytes.len() >= 3 {
                    let frame_len = ((in_bytes[0] as usize) << 16)
                        | ((in_bytes[1] as usize) << 8)
                        | (in_bytes[2] as usize);

                    if in_bytes.len() < 3 + frame_len {
                        // Frame not completely received yet, wait for more data
                        break;
                    }

                    let chunk: Vec<u8> = in_bytes.drain(3..3 + frame_len).collect();
                    in_bytes.drain(0..3); // remove length prefix

                    let decoded_node = {
                        let mut lock = noise_reader.lock().await;
                        lock.decode_frame(&chunk)
                    };

                    match decoded_node {
                        Ok(node) => {
                            trace!("Received incoming node: <{}>", node.tag);
                            if let Err(e) = incoming_tx.send(node).await {
                                warn!("Incoming node receiver dropped: {}", e);
                                return;
                            }
                        }
                        Err(e) => {
                            error!("Failed to decrypt or decode incoming frame: {}", e);
                        }
                    }
                }
            }
            debug!("WebSocket reader task terminated.");
        });

        Ok((
            Self {
                outgoing_tx,
                writer_handle: writer_task.abort_handle(),
                reader_handle: reader_task.abort_handle(),
            },
            incoming_rx,
        ))
    }

    /// Explicitly aborts reader and writer background tasks, ensuring the underlying TCP connection closes immediately.
    pub async fn close(&self) {
        debug!("Explicitly closing WhatsAppSocket and aborting tasks...");
        self.writer_handle.abort();
        self.reader_handle.abort();
    }

    /// Sends a BinaryNode over the encrypted transport stream.
    pub async fn send_node(&self, node: BinaryNode) -> Result<(), CoreError> {
        self.outgoing_tx
            .send(node)
            .await
            .map_err(|e| CoreError::Network(format!("Failed to enqueue outgoing node: {}", e)))
    }
}
