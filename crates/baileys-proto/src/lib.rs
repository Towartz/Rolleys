//! Generated Protobuf structures for the WhatsApp Web Protocol.
//!
//! Includes all messages and enums from `WAProto.proto`:
//! `WebMessageInfo`, `Message`, `HandshakeMessage`, `ClientPayload`, `HistorySync`, etc.

pub mod proto {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/proto.rs"));
}

pub mod schema;

#[cfg(feature = "extractor")]
pub mod extractor;

pub use proto::*;
pub use schema::*;

#[cfg(feature = "extractor")]
pub use extractor::*;

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message as ProstMessage;

    #[test]
    fn test_protobuf_encode_decode_message() {
        let msg = Message {
            conversation: Some("Hello from WhatsApp Rust Core!".to_string()),
            ..Default::default()
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).expect("Encode failed");
        assert!(!buf.is_empty());

        let decoded = Message::decode(&buf[..]).expect("Decode failed");
        assert_eq!(
            decoded.conversation.as_deref(),
            Some("Hello from WhatsApp Rust Core!")
        );
    }

    #[test]
    fn test_protobuf_client_payload() {
        let payload = ClientPayload {
            connect_type: Some(client_payload::ConnectType::WifiUnknown as i32),
            connect_reason: Some(client_payload::ConnectReason::UserActivated as i32),
            push_name: Some("Aurora User".to_string()),
            passive: Some(true),
            ..Default::default()
        };

        let mut buf = Vec::new();
        payload.encode(&mut buf).expect("Encode failed");
        assert!(!buf.is_empty());

        let decoded = ClientPayload::decode(&buf[..]).expect("Decode failed");
        assert_eq!(decoded.push_name.as_deref(), Some("Aurora User"));
        assert_eq!(decoded.passive, Some(true));
    }
}
