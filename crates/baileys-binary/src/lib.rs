//! WABinary XML token serialization and JID engine for WhatsApp.

pub mod decoder;
pub mod encoder;
pub mod jid;
pub mod node;
pub mod tokens;

pub use decoder::*;
pub use encoder::*;
pub use jid::*;
pub use node::*;
pub use tokens::*;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum BinaryError {
    #[error("Unexpected end of binary stream")]
    UnexpectedEof,

    #[error("Invalid nibble character: '{0}'")]
    InvalidNibbleChar(char),

    #[error("Invalid hex character: '{0}'")]
    InvalidHexChar(char),

    #[error("Invalid nibble byte: {0}")]
    InvalidNibbleByte(u8),

    #[error("Invalid hex byte: {0}")]
    InvalidHexByte(u8),

    #[error("String too large for packing: {0} bytes")]
    StringTooLarge(usize),

    #[error("Zlib decompression failed: {0}")]
    DecompressionFailed(String),

    #[error("Invalid UTF-8 string: {0}")]
    InvalidUtf8(String),

    #[error("Unknown token in dictionary {0}, index {1}")]
    UnknownToken(u8, u8),

    #[error("Unknown WABinary tag: {0}")]
    UnknownTag(u8),

    #[error("Unexpected list tag: {0}")]
    UnexpectedListTag(usize),

    #[error("Encountered empty list where node was expected")]
    EmptyList,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_encode_decode_roundtrip() {
        let node = BinaryNode::new("iq")
            .with_attr("id", "msg-12345")
            .with_attr("type", "set")
            .with_attr("to", "s.whatsapp.net")
            .with_children(vec![
                BinaryNode::new("ping"),
                BinaryNode::new("user")
                    .with_attr("jid", "123456789:1@s.whatsapp.net")
                    .with_bytes_content(vec![0xAA, 0xBB, 0xCC]),
            ]);

        let encoded = BinaryEncoder::encode(&node).expect("Encoding failed");
        assert!(!encoded.is_empty());
        assert_eq!(encoded[0], 0); // prefix 0 byte

        let decoded = BinaryDecoder::decode(&encoded).expect("Decoding failed");
        assert_eq!(decoded.tag, "iq");
        assert_eq!(decoded.get_attr("id"), Some("msg-12345"));
        assert_eq!(decoded.get_attr("type"), Some("set"));

        let children = decoded.get_children();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].tag, "ping");
        assert_eq!(children[1].tag, "user");
        assert_eq!(
            children[1].get_attr("jid"),
            Some("123456789:1@s.whatsapp.net")
        );

        match &children[1].content {
            Some(BinaryNodeContent::Bytes(b)) => assert_eq!(b, &[0xAA, 0xBB, 0xCC]),
            _ => panic!("Expected bytes content"),
        }
    }
}
