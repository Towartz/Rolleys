//! Consolidated runtime state management for the Baileys client engine.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

use crate::companion::CompanionPairingState;
use crate::events::CallEvent;
use crate::socket::WhatsAppSocket;
use baileys_binary::BinaryNode;
use baileys_proto::proto::WebMessageInfo;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

/// Formal connection lifecycle states for the WhatsApp WebSocket session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Handshaking,
    Syncing,
    Connected,
    Reconnecting {
        attempt: u32,
        next_retry_millis: u64,
    },
    LoggedOut,
}

/// Ephemeral session and network state.
pub struct SessionState {
    pub is_connected: AtomicBool,
    pub is_running: AtomicBool,
    pub auto_download: AtomicBool,
    pub ghost_mode: AtomicBool,
    pub connection_tx: watch::Sender<ConnectionState>,
    pub connection_rx: watch::Receiver<ConnectionState>,
    pub socket: Mutex<Option<Arc<WhatsAppSocket>>>,
    pub pending_iq: Mutex<HashMap<String, oneshot::Sender<BinaryNode>>>,
    pub recent_messages: Mutex<HashMap<String, WebMessageInfo>>,
    pub media_conn: Mutex<Option<baileys_media::MediaConnInfo>>,
    pub active_calls: Mutex<HashMap<String, CallEvent>>,
    pub companion_state: Mutex<Option<Arc<CompanionPairingState>>>,
    pub last_qr: Mutex<Option<String>>,
    pub outbound_retries: Mutex<HashMap<String, u32>>,
    pub sender_key_memory: Mutex<HashMap<String, HashSet<String>>>,
}

impl SessionState {
    pub fn new(ghost_mode: bool) -> Self {
        let (connection_tx, connection_rx) = watch::channel(ConnectionState::Disconnected);
        Self {
            is_connected: AtomicBool::new(false),
            is_running: AtomicBool::new(false),
            auto_download: AtomicBool::new(false),
            ghost_mode: AtomicBool::new(ghost_mode),
            connection_tx,
            connection_rx,
            socket: Mutex::new(None),
            pending_iq: Mutex::new(HashMap::new()),
            recent_messages: Mutex::new(HashMap::new()),
            media_conn: Mutex::new(None),
            active_calls: Mutex::new(HashMap::new()),
            companion_state: Mutex::new(None),
            last_qr: Mutex::new(None),
            outbound_retries: Mutex::new(HashMap::new()),
            sender_key_memory: Mutex::new(HashMap::new()),
        }
    }

    #[inline]
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set_connected(&self, val: bool) {
        self.is_connected.store(val, Ordering::Release);
        let state = if val {
            ConnectionState::Connected
        } else {
            ConnectionState::Disconnected
        };
        let _ = self.connection_tx.send(state);
    }

    #[inline]
    pub fn get_connection_state(&self) -> ConnectionState {
        *self.connection_rx.borrow()
    }

    #[inline]
    pub fn set_connection_state(&self, state: ConnectionState) {
        let is_conn = state == ConnectionState::Connected;
        self.is_connected.store(is_conn, Ordering::Release);
        let _ = self.connection_tx.send(state);
    }

    pub fn subscribe_connection_state(&self) -> watch::Receiver<ConnectionState> {
        self.connection_rx.clone()
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set_running(&self, val: bool) {
        self.is_running.store(val, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state_lifecycle() {
        let state = SessionState::new(false);
        assert_eq!(state.get_connection_state(), ConnectionState::Disconnected);
        assert!(!state.is_connected());

        state.set_connection_state(ConnectionState::Connecting);
        assert_eq!(state.get_connection_state(), ConnectionState::Connecting);
        assert!(!state.is_connected());

        state.set_connection_state(ConnectionState::Handshaking);
        assert_eq!(state.get_connection_state(), ConnectionState::Handshaking);

        state.set_connection_state(ConnectionState::Connected);
        assert_eq!(state.get_connection_state(), ConnectionState::Connected);
        assert!(state.is_connected());

        state.set_connection_state(ConnectionState::Reconnecting {
            attempt: 2,
            next_retry_millis: 5000,
        });
        assert_eq!(
            state.get_connection_state(),
            ConnectionState::Reconnecting {
                attempt: 2,
                next_retry_millis: 5000,
            }
        );
        assert!(!state.is_connected());
    }
}
