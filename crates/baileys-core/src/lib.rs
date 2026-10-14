//! Core WhatsApp Web client engine orchestrator.

#![allow(clippy::too_many_arguments)]

pub mod auth;
pub mod biz;
pub mod call;
pub mod client;
pub mod commerce;
pub mod community;
pub mod companion;
pub mod config;
pub mod ephemeral;
pub mod events;
pub mod groups;
pub mod inner;
pub mod interactive;
pub mod labels;
pub mod messages;
pub mod newsletter;
pub mod polls;
pub mod privacy;
pub mod proto_builders;
pub mod proto_recognizer;
pub mod registration;
pub mod reporting;
pub mod retry;
pub mod rich_messages;
pub mod services;
pub mod socket;
pub mod state;
pub mod status;
pub mod syncd;
pub mod tc_token;
pub mod telemetry;
pub mod usync;
pub mod version;

pub use auth::*;
pub use baileys_proto;
pub use baileys_proto::proto::WebMessageInfo;
pub use baileys_store::{
    PollTally, StoredCall, StoredCatalogCollection, StoredCatalogProduct, StoredChat,
    StoredCommunitySubgroup, StoredContact, StoredLabel, StoredMessage, StoredNewsletter,
    StoredOrder, StoredPoll, StoredPollVote, StoredStory, StoredUserDevice,
};
pub use biz::*;
pub use call::*;
pub use client::*;
pub use commerce::*;
pub use community::*;
pub use companion::*;
pub use config::*;
pub use ephemeral::*;
pub use events::*;
pub use groups::*;
pub use inner::*;
pub use interactive::*;
pub use labels::*;
pub use messages::*;
pub use newsletter::*;
pub use polls::*;
pub use privacy::*;
pub use proto_builders::*;
pub use proto_recognizer::*;
pub use registration::*;
pub use reporting::*;
pub use retry::*;
pub use rich_messages::*;
pub use services::*;
pub use socket::*;
pub use state::*;
pub use status::*;
pub use syncd::*;
pub use tc_token::*;
pub use telemetry::*;
pub use usync::*;
pub use version::*;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("Storage error: {0}")]
    Store(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Noise handshake error: {0}")]
    Noise(String),

    #[error("Signal E2EE error: {0}")]
    Signal(String),

    #[error("Binary XML error: {0}")]
    Binary(String),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Authentication failure: {0}")]
    Auth(String),

    #[error("Media error: {0}")]
    Media(String),
}

impl From<baileys_store::StoreError> for CoreError {
    fn from(e: baileys_store::StoreError) -> Self {
        Self::Store(e.to_string())
    }
}

impl From<baileys_binary::BinaryError> for CoreError {
    fn from(e: baileys_binary::BinaryError) -> Self {
        Self::Binary(e.to_string())
    }
}

impl From<baileys_crypto::CryptoError> for CoreError {
    fn from(e: baileys_crypto::CryptoError) -> Self {
        Self::Crypto(e.to_string())
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Network(e.to_string())
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Protocol(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_store::MemoryStore;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_client_connect_and_events() {
        let store = Arc::new(MemoryStore::new());
        let config = BaileysConfigBuilder::new()
            .with_browser("Aurora Chats", "Android", "1.0.0")
            .with_websocket_url("mock://wa")
            .build();

        let client = BaileysClient::new(store, config);
        let mut rx = client.events();

        // 1. Connect
        client.connect().await.expect("Connect failed");

        // 2. Expect Connecting event
        let ev1 = rx.recv().await.unwrap();
        match ev1 {
            BaileysEvent::Connecting => (),
            _ => panic!("Expected Connecting event, got {:?}", ev1),
        }

        // 3. Expect QrCode event (since unauthenticated)
        let ev2 = rx.recv().await.unwrap();
        match ev2 {
            BaileysEvent::QrCode { code } => {
                assert!(!code.is_empty());
            }
            _ => panic!("Expected QrCode event, got {:?}", ev2),
        }

        // 4. Send message
        let msg_id = client
            .send_text_message("123456789@s.whatsapp.net", "Hello from Rust Core!")
            .await
            .expect("Send message failed");
        assert!(!msg_id.is_empty());

        let ev3 = rx.recv().await.unwrap();
        match ev3 {
            BaileysEvent::Message { message } => {
                let text = message.message.unwrap().conversation.unwrap();
                assert_eq!(text, "Hello from Rust Core!");
            }
            _ => panic!("Expected Message event, got {:?}", ev3),
        }

        // 5. Disconnect
        client.disconnect().await;
        let ev4 = rx.recv().await.unwrap();
        match ev4 {
            BaileysEvent::Disconnected { reason } => {
                assert_eq!(reason, "User requested disconnect");
            }
            _ => panic!("Expected Disconnected event, got {:?}", ev4),
        }
    }

    #[test]
    fn test_registration_payload_builder() {
        use baileys_proto::proto;
        use baileys_store::AuthCreds;
        use prost::Message;

        let creds = AuthCreds::init();
        let config = BaileysConfigBuilder::new()
            .with_browser("Rose-Baileys", "Desktop", "1.0.0")
            .build();

        let payload = generate_registration_node(&creds, &config);
        let bytes = payload.encode_to_vec();
        assert!(!bytes.is_empty());

        let decoded = proto::ClientPayload::decode(&bytes[..]).expect("Decode payload failed");
        assert_eq!(decoded.passive, Some(false));
        assert!(decoded.device_pairing_data.is_some());
    }

    #[test]
    fn test_companion_pairing_state() {
        use baileys_store::AuthCreds;

        let creds = AuthCreds::init();
        let config = BaileysConfigBuilder::new()
            .with_browser("Rose-Baileys", "Desktop", "1.0.0")
            .build();

        let companion = CompanionPairingState::new("6281234567890");
        assert_eq!(companion.pairing_code.len(), 8); // 8-char Crockford Base32

        let hello = companion
            .build_companion_hello_iq(&creds, &config)
            .expect("build hello failed");
        assert_eq!(hello.tag, "iq");
        assert_eq!(hello.get_attr("type"), Some("set"));
        assert!(hello.get_child("link_code_companion_reg").is_some());
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_gateway_stanzas() {
        use baileys_store::AuthCreds;
        let creds = AuthCreds::init();
        let config = BaileysConfig::default();
        let res = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let (_socket, mut incoming_rx) = WhatsAppSocket::connect(&creds, &config)
                .await
                .expect("connect failed");
            println!("Connected to WhatsAppSocket, awaiting incoming nodes...");
            while let Some(node) = incoming_rx.recv().await {
                println!("INCOMING NODE: tag=<{}> attrs={:?}", node.tag, node.attrs);
                if let Some(pair) = node.get_child("pair-device") {
                    println!("FOUND PAIR-DEVICE NODE! {:?}", pair);
                    break;
                }
            }
        })
        .await;
        println!("Test finished: {:?}", res);
    }
}
