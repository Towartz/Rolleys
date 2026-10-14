use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::CoreError;
use baileys_binary::BinaryNode;

pub const S_WHATSAPP_NET: &str = "s.whatsapp.net";

// w:mex GraphQL Query IDs (WhatsApp Web / Desktop parity)
pub const QUERY_FETCH_NEWSLETTER: &str = "6563316087068696";
pub const QUERY_SUBSCRIBED_NEWSLETTERS: &str = "6388546374527196";
pub const QUERY_RECOMMENDED_NEWSLETTERS: &str = "7263823273662354";
pub const MUTATION_FOLLOW_NEWSLETTER: &str = "9926858900719341";
pub const MUTATION_UNFOLLOW_NEWSLETTER: &str = "6392786840836363";
pub const MUTATION_MUTE_NEWSLETTER: &str = "6274038279359549";
pub const MUTATION_UNMUTE_NEWSLETTER: &str = "6068417879924485";
pub const MUTATION_CREATE_NEWSLETTER: &str = "6234210096708695";
pub const MUTATION_UPDATE_NEWSLETTER: &str = "7150902998257522";
pub const MUTATION_DELETE_NEWSLETTER: &str = "8316537688363079";
pub const MUTATION_DEMOTE_ADMIN_NEWSLETTER: &str = "6555138211237979";
pub const MUTATION_CHANGE_OWNER_NEWSLETTER: &str = "7346332738735895";
pub const QUERY_ADMIN_COUNT: &str = "7130823597031706";
pub const QUERY_REACHOUT_TIMELOCK: &str = "23983697327930364";
pub const QUERY_MESSAGE_CAPPING_INFO: &str = "24503548349331633";

/// Metadata and state for a WhatsApp Channel (Newsletter).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsletterMetadata {
    pub jid: String,
    pub name: String,
    pub description: Option<String>,
    pub invite_code: Option<String>,
    pub picture_url: Option<String>,
    pub preview_url: Option<String>,
    pub subscribers_count: u64,
    pub role: Option<String>, // "OWNER", "ADMIN", "SUBSCRIBER", "GUEST"
    pub muted: bool,
    pub verified: bool,
    pub creation_time: u64,
}

/// A message post in a WhatsApp Channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsletterMessage {
    pub server_id: u64,
    pub views_count: Option<u64>,
    pub reactions: HashMap<String, u32>,
    pub timestamp: u64,
    pub summary: String,
    pub has_media: bool,
    pub raw_proto: Vec<u8>,
}

/// Builds outgoing `<iq type="get" xmlns="w:mex">` querying Meta GraphQL.
pub fn build_mex_iq(query_id: &str, variables: Value) -> BinaryNode {
    let payload_val = serde_json::json!({
        "variables": variables
    });
    let payload_bytes = serde_json::to_vec(&payload_val).unwrap_or_default();
    let msg_id = format!("mex_{:X}", rand::random::<u64>());

    BinaryNode::new("iq")
        .with_attr("id", msg_id)
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "w:mex")
        .with_child(
            BinaryNode::new("query")
                .with_attr("query_id", query_id)
                .with_content(payload_bytes),
        )
}

/// Parses the JSON content of a `<result>` child node inside a `w:mex` IQ response.
pub fn parse_mex_response(res_node: &BinaryNode) -> Result<Value, CoreError> {
    let result_child = res_node
        .get_child("result")
        .ok_or_else(|| CoreError::Protocol("Missing <result> tag in mex response".to_string()))?;

    let content_bytes = result_child
        .content_as_bytes()
        .ok_or_else(|| CoreError::Protocol("Empty content in <result> node".to_string()))?;

    let root_val: Value = serde_json::from_slice(content_bytes)
        .map_err(|e| CoreError::Protocol(format!("Failed to parse mex JSON: {}", e)))?;

    if let Some(errors) = root_val.get("errors").and_then(|e| e.as_array()) {
        if !errors.is_empty() {
            let msg = errors[0]
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("GraphQL error");
            return Err(CoreError::Protocol(format!("MEX GraphQL error: {}", msg)));
        }
    }

    Ok(root_val.get("data").cloned().unwrap_or(root_val))
}

/// Parses a GraphQL JSON object into a strongly typed `NewsletterMetadata`.
pub fn parse_single_newsletter(obj: &Value) -> Option<NewsletterMetadata> {
    let jid = obj.get("id").and_then(|v| v.as_str())?.to_string();
    let thread = obj.get("thread_metadata");

    let name = thread
        .and_then(|t| t.get("name"))
        .and_then(|n| n.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("<unnamed>")
        .to_string();

    let description = thread
        .and_then(|t| t.get("description"))
        .and_then(|d| d.get("text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let invite_code = thread
        .and_then(|t| t.get("invite"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let picture_url = thread
        .and_then(|t| t.get("picture"))
        .and_then(|p| p.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let preview_url = thread
        .and_then(|t| t.get("preview"))
        .and_then(|p| p.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let subscribers_count = thread
        .and_then(|t| t.get("subscribers_count"))
        .and_then(|s| {
            if let Some(n) = s.as_u64() {
                Some(n)
            } else if let Some(str_val) = s.as_str() {
                str_val.parse::<u64>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    let verified = thread
        .and_then(|t| t.get("verification"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_uppercase() == "VERIFIED")
        .unwrap_or(false);

    let creation_time = thread
        .and_then(|t| t.get("creation_time"))
        .and_then(|c| {
            if let Some(n) = c.as_u64() {
                Some(n)
            } else if let Some(str_val) = c.as_str() {
                str_val.parse::<u64>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    let viewer = obj.get("viewer_metadata");
    let role = viewer
        .and_then(|v| v.get("role"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());

    let muted = viewer
        .and_then(|v| v.get("mute"))
        .and_then(|m| m.as_str())
        .map(|m| m.to_uppercase() == "ON")
        .unwrap_or(false);

    Some(NewsletterMetadata {
        jid,
        name,
        description,
        invite_code,
        picture_url,
        preview_url,
        subscribers_count,
        role,
        muted,
        verified,
        creation_time,
    })
}

/// Parses subscribed newsletters from `xwa2_newsletter_subscribed` query output.
pub fn parse_subscribed_newsletters(data: &Value) -> Vec<NewsletterMetadata> {
    let mut results = Vec::new();
    if let Some(arr) = data
        .get("xwa2_newsletter_subscribed")
        .and_then(|v| v.as_array())
    {
        for item in arr {
            if let Some(meta) = parse_single_newsletter(item) {
                results.push(meta);
            }
        }
    }
    results
}

/// Builds an outgoing query for channel messages:
/// `<iq type="get" to="s.whatsapp.net" xmlns="newsletter"><messages type="jid" jid="..." count="..." [before="..."] /></iq>`
pub fn build_newsletter_messages_iq(jid: &str, count: u32, before: Option<u64>) -> BinaryNode {
    let mut msg_node = BinaryNode::new("messages")
        .with_attr("type", "jid")
        .with_attr("jid", jid)
        .with_attr("count", count.to_string());

    if let Some(b) = before {
        msg_node = msg_node.with_attr("before", b.to_string());
    }

    BinaryNode::new("iq")
        .with_attr("id", format!("nl_msgs_{:X}", rand::random::<u64>()))
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "newsletter")
        .with_child(msg_node)
}

/// Builds a channel reaction stanza:
/// `<message to="..." id="..." server_id="..." type="reaction"><reaction code="❤️"/></message>`
pub fn build_newsletter_reaction_node(
    jid: &str,
    server_id: u64,
    reaction: &str,
    msg_id: Option<&str>,
) -> BinaryNode {
    let id = msg_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("3EB0{:X}", rand::random::<u64>()));

    let mut msg_node = BinaryNode::new("message")
        .with_attr("to", jid)
        .with_attr("id", id)
        .with_attr("server_id", server_id.to_string())
        .with_attr("type", "reaction");

    if reaction.is_empty() {
        // Revoke reaction
        msg_node = msg_node.with_attr("edit", "7");
        let react_node = BinaryNode::new("reaction");
        msg_node.with_child(react_node)
    } else {
        let react_node = BinaryNode::new("reaction").with_attr("code", reaction);
        msg_node.with_child(react_node)
    }
}

/// Builds a view receipt stanza for channel posts:
/// `<receipt to="..." type="view" id="..."><list><item server_id="..."/></list></receipt>`
pub fn build_newsletter_view_receipt(jid: &str, server_ids: &[u64]) -> BinaryNode {
    let items: Vec<BinaryNode> = server_ids
        .iter()
        .map(|sid| BinaryNode::new("item").with_attr("server_id", sid.to_string()))
        .collect();

    BinaryNode::new("receipt")
        .with_attr("to", jid)
        .with_attr("type", "view")
        .with_attr("id", format!("view_{:X}", rand::random::<u64>()))
        .with_child(BinaryNode::new("list").with_children(items))
}

/// Builds a live update subscription stanza:
/// `<iq type="set" to="<jid>" xmlns="newsletter"><live_updates/></iq>`
pub fn build_newsletter_live_updates_iq(jid: &str) -> BinaryNode {
    BinaryNode::new("iq")
        .with_attr("id", format!("nl_live_{:X}", rand::random::<u64>()))
        .with_attr("to", jid)
        .with_attr("type", "set")
        .with_attr("xmlns", "newsletter")
        .with_child(BinaryNode::new("live_updates"))
}

/// Parses the duration string from a `<live_updates duration="..."/>` response.
pub fn parse_newsletter_live_updates_response(node: &BinaryNode) -> Option<String> {
    let live = node.get_child("live_updates")?;
    live.get_attr("duration").map(|s| s.to_string())
}

/// Parses the admin count integer from a MEX `xwa2_newsletter_admin` response.
pub fn parse_newsletter_admin_count(val: &Value) -> Option<u32> {
    if let Some(c) = val.get("admin_count").and_then(|v| v.as_u64()) {
        return Some(c as u32);
    }
    if let Some(admin_obj) = val.get("xwa2_newsletter_admin") {
        return admin_obj
            .get("admin_count")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);
    }
    None
}

/// Parses account reachout timelock / spam enforcement standing.
/// Returns `(is_active, enforcement_type, time_enforcement_ends)`.
pub fn parse_reachout_timelock_response(
    val: &Value,
) -> Option<(bool, Option<String>, Option<u64>)> {
    let target = val
        .get("xwa2_fetch_account_reachout_timelock")
        .unwrap_or(val);
    let is_active = target
        .get("is_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let enforcement_type = target
        .get("enforcement_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let ends = target.get("time_enforcement_ends").and_then(|v| {
        v.as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| v.as_u64())
    });
    Some((is_active, enforcement_type, ends))
}

/// Parses new chat message capping info quota and usage.
pub fn parse_message_capping_response(val: &Value) -> Option<Value> {
    val.get("xwa2_message_capping_info")
        .cloned()
        .or_else(|| Some(val.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mex_iq_builder() {
        let vars = serde_json::json!({
            "key": "120363144038483540@newsletter",
            "type": "JID"
        });
        let iq = build_mex_iq(QUERY_FETCH_NEWSLETTER, vars);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("w:mex"));
        assert_eq!(iq.get_attr("type"), Some("get"));

        let q = iq.get_child("query").unwrap();
        assert_eq!(q.get_attr("query_id"), Some(QUERY_FETCH_NEWSLETTER));
    }

    #[test]
    fn test_parse_single_newsletter() {
        let sample_json = serde_json::json!({
            "id": "120363144038483540@newsletter",
            "state": { "type": "ACTIVE" },
            "thread_metadata": {
                "name": { "text": "WhatsApp Official" },
                "description": { "text": "Product news and updates" },
                "invite": "0029Va4K9bZ5Ui2T",
                "subscribers_count": "55000000",
                "verification": "VERIFIED",
                "creation_time": "1690000000"
            },
            "viewer_metadata": {
                "role": "SUBSCRIBER",
                "mute": "OFF"
            }
        });

        let meta = parse_single_newsletter(&sample_json).unwrap();
        assert_eq!(meta.jid, "120363144038483540@newsletter");
        assert_eq!(meta.name, "WhatsApp Official");
        assert_eq!(meta.subscribers_count, 55000000);
        assert!(meta.verified);
        assert!(!meta.muted);
        assert_eq!(meta.role.as_deref(), Some("SUBSCRIBER"));
    }

    #[test]
    fn test_newsletter_reaction_node() {
        let node = build_newsletter_reaction_node(
            "120363144038483540@newsletter",
            12345,
            "🔥",
            Some("MSG_1"),
        );
        assert_eq!(node.tag, "message");
        assert_eq!(node.get_attr("type"), Some("reaction"));
        assert_eq!(node.get_attr("server_id"), Some("12345"));

        let r = node.get_child("reaction").unwrap();
        assert_eq!(r.get_attr("code"), Some("🔥"));
    }
}
