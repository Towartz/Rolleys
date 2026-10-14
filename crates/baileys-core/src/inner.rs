//! Shared inner core client orchestrator holding immutable subsystems and mutable session state.

use crate::config::BaileysConfig;
use crate::events::EventEmitter;
use crate::state::SessionState;
use baileys_signal::SignalRepository;
use baileys_store::BaileysStore;
use std::sync::Arc;

pub struct ClientInner {
    pub store: Arc<dyn BaileysStore>,
    pub config: BaileysConfig,
    pub events: EventEmitter,
    pub signal: Arc<SignalRepository>,
    pub session: SessionState,
    pub metrics: crate::telemetry::TelemetryMetrics,
}

use crate::CoreError;
use baileys_binary::BinaryNode;
use std::time::Duration;

impl ClientInner {
    pub fn new(
        store: Arc<dyn BaileysStore>,
        config: BaileysConfig,
        signal: Arc<SignalRepository>,
    ) -> Self {
        let ghost_mode = config.ghost_mode;
        Self {
            store,
            config,
            events: EventEmitter::default(),
            signal,
            session: SessionState::new(ghost_mode),
            metrics: crate::telemetry::TelemetryMetrics::default(),
        }
    }

    /// Sends an XML stanza node directly over the active WebSocket connection.
    pub async fn send_node(&self, node: BinaryNode) -> Result<(), CoreError> {
        let socket_opt = {
            let lock = self.session.socket.lock().await;
            lock.clone()
        };
        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;
        self.metrics.record_msg_out();
        socket.send_node(node).await?;
        Ok(())
    }

    /// Sends an IQ query node and awaits the corresponding response matching the stanza `id`.
    pub async fn send_iq(
        &self,
        node: BinaryNode,
        timeout: Duration,
    ) -> Result<BinaryNode, CoreError> {
        let msg_id = node
            .get_attr("id")
            .ok_or_else(|| CoreError::Protocol("Missing 'id' attribute on IQ node".into()))?
            .to_string();

        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut lock = self.session.pending_iq.lock().await;
            lock.insert(msg_id.clone(), tx);
        }

        let socket_opt = {
            let lock = self.session.socket.lock().await;
            lock.clone()
        };

        let socket = socket_opt.ok_or_else(|| CoreError::Network("Client not connected".into()))?;
        socket.send_node(node).await?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => {
                if resp.tag == "iq" && resp.get_attr("type") == Some("error") {
                    let code = resp
                        .get_child("error")
                        .and_then(|e| e.get_attr("code"))
                        .unwrap_or("unknown");
                    let text = resp
                        .get_child("error")
                        .and_then(|e| e.get_attr("text"))
                        .unwrap_or("");
                    return Err(CoreError::Protocol(format!(
                        "IQ error response code {}: {}",
                        code, text
                    )));
                }
                Ok(resp)
            }
            Ok(Err(_)) => {
                let mut lock = self.session.pending_iq.lock().await;
                lock.remove(&msg_id);
                Err(CoreError::Network("IQ response channel dropped".into()))
            }
            Err(_) => {
                let mut lock = self.session.pending_iq.lock().await;
                lock.remove(&msg_id);
                Err(CoreError::Network(format!(
                    "IQ request {} timed out after {:?}",
                    msg_id, timeout
                )))
            }
        }
    }
}

impl std::ops::Deref for ClientInner {
    type Target = SessionState;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.session
    }
}
