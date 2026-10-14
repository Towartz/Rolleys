//! Trusted Contact (TC) Tokens / Privacy Tokens.
//!
//! Mirrors WhatsApp Web and canonical Baileys `tc-token-utils.ts`:
//! - 7-day bucket duration with ~28-day (4-bucket) rolling window.
//! - Attaches cryptographic privacy tokens to messages directed at non-contacts
//!   to satisfy WhatsApp anti-spam gateway policies.

use baileys_binary::BinaryNode;
use serde::{Deserialize, Serialize};

/// 7 days in seconds.
pub const TC_TOKEN_BUCKET_DURATION: u64 = 604_800;

/// Number of rolling buckets (~28 days).
pub const TC_TOKEN_NUM_BUCKETS: u64 = 4;

/// Represents a cached or persisted Trusted Contact Token for a WhatsApp user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TcToken {
    pub token: Vec<u8>,
    pub timestamp: u64,
    pub sender_timestamp: Option<u64>,
}

/// Checks whether a TC token timestamp has expired outside the 28-day rolling window.
pub fn is_tc_token_expired(timestamp: u64, current_time: u64) -> bool {
    let current_bucket = current_time / TC_TOKEN_BUCKET_DURATION;
    if current_bucket < (TC_TOKEN_NUM_BUCKETS - 1) {
        return false;
    }
    let cutoff_bucket = current_bucket - (TC_TOKEN_NUM_BUCKETS - 1);
    let cutoff_timestamp = cutoff_bucket * TC_TOKEN_BUCKET_DURATION;
    timestamp < cutoff_timestamp
}

/// Determines whether a new TC token should be sent based on the sender's last issuance timestamp.
pub fn should_send_new_tc_token(sender_timestamp: Option<u64>, current_time: u64) -> bool {
    match sender_timestamp {
        None => true,
        Some(ts) => {
            let current_bucket = current_time / TC_TOKEN_BUCKET_DURATION;
            let sender_bucket = ts / TC_TOKEN_BUCKET_DURATION;
            current_bucket > sender_bucket
        }
    }
}

/// Builds the `<tctoken>` XML stanza element attached to outgoing messages.
pub fn build_tc_token_node(token_bytes: &[u8], timestamp: u64) -> BinaryNode {
    BinaryNode::new("tctoken")
        .with_attr("t", timestamp.to_string())
        .with_bytes_content(token_bytes.to_vec())
}

/// Parses an incoming `<tctoken>` stanza element from an IQ response or notification.
pub fn parse_tc_token_node(node: &BinaryNode) -> Option<TcToken> {
    let t_node = if node.tag == "tctoken" {
        node
    } else {
        node.get_child("tctoken")?
    };

    let ts = t_node
        .get_attr("t")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let bytes = t_node.content_as_bytes()?.to_vec();

    Some(TcToken {
        token: bytes,
        timestamp: ts,
        sender_timestamp: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tc_token_expiration_logic() {
        let now = 1_728_000_000u64; // arbitrary reference time
        assert!(!is_tc_token_expired(now, now));
        assert!(!is_tc_token_expired(now - TC_TOKEN_BUCKET_DURATION, now));
        assert!(!is_tc_token_expired(
            now - 2 * TC_TOKEN_BUCKET_DURATION,
            now
        ));
        assert!(!is_tc_token_expired(
            now - 3 * TC_TOKEN_BUCKET_DURATION,
            now
        ));

        // 4 buckets older (28+ days) is expired
        assert!(is_tc_token_expired(now - 4 * TC_TOKEN_BUCKET_DURATION, now));
    }

    #[test]
    fn test_should_send_new_tc_token() {
        let now = 1_728_000_000u64;
        assert!(should_send_new_tc_token(None, now));

        // Same bucket -> should not re-send
        assert!(!should_send_new_tc_token(Some(now), now));

        // Past bucket -> should re-send
        let past = now - TC_TOKEN_BUCKET_DURATION - 1;
        assert!(should_send_new_tc_token(Some(past), now));
    }

    #[test]
    fn test_build_and_parse_tc_token_node() {
        let raw = vec![0x01, 0x02, 0x03, 0x04];
        let ts = 1728100000;
        let node = build_tc_token_node(&raw, ts);
        assert_eq!(node.tag, "tctoken");
        assert_eq!(node.get_attr("t"), Some("1728100000"));
        assert_eq!(node.content_as_bytes(), Some(raw.as_slice()));

        let parsed = parse_tc_token_node(&node).expect("Failed to parse tc token");
        assert_eq!(parsed.token, raw);
        assert_eq!(parsed.timestamp, ts);
    }
}
