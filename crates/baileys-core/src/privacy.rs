use baileys_binary::BinaryNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const S_WHATSAPP_NET: &str = "s.whatsapp.net";

/// User's privacy configuration across WhatsApp categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PrivacySettings {
    /// Who can see read receipts: "all" or "none"
    pub readreceipts: Option<String>,
    /// Who can see profile picture: "all", "contacts", "contact_blacklist", "none"
    pub profile: Option<String>,
    /// Who can see status stories: "all", "contacts", "contact_blacklist", "none"
    pub status: Option<String>,
    /// Who can see online presence: "all", "match_last_seen"
    pub online: Option<String>,
    /// Who can see last seen timestamp: "all", "contacts", "contact_blacklist", "none"
    pub last: Option<String>,
    /// Who can add you to groups: "all", "contacts", "contact_blacklist"
    pub groupadd: Option<String>,
    /// Who can call you: "all", "known"
    pub calladd: Option<String>,
    /// Raw key-value privacy settings returned by WhatsApp gateway
    pub raw: HashMap<String, String>,
}

/// WhatsApp user's About / Bio status string and publication timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileStatus {
    pub status: String,
    pub set_at: Option<u64>,
}

/// Verified or standard WhatsApp Business profile metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BusinessProfile {
    pub jid: String,
    pub address: Option<String>,
    pub description: Option<String>,
    pub websites: Vec<String>,
    pub email: Option<String>,
    pub category: Option<String>,
}

/// Builds an IQ node to fetch the current blocklist from WhatsApp gateway.
pub fn build_fetch_blocklist_iq(iq_id: &str) -> BinaryNode {
    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "blocklist")
        .with_attr("type", "get")
        .with_attr("id", iq_id)
        .with_child(BinaryNode::new("list"))
}

/// Builds an IQ node to block or unblock a contact.
pub fn build_block_iq(iq_id: &str, jid: &str, action: &str, pn_jid: Option<&str>) -> BinaryNode {
    let mut item_node = BinaryNode::new("item")
        .with_attr("action", action)
        .with_attr("jid", jid);

    if let Some(pn) = pn_jid {
        item_node = item_node.with_attr("pn_jid", pn);
    }

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "blocklist")
        .with_attr("type", "set")
        .with_attr("id", iq_id)
        .with_child(item_node)
}

/// Parses the list of blocked JIDs from an IQ response.
pub fn parse_blocklist_response(node: &BinaryNode) -> Vec<String> {
    let mut list = Vec::new();
    if let Some(list_node) = node.get_child("list") {
        for item in list_node.get_children() {
            if item.tag == "item" {
                if let Some(jid) = item.get_attr("jid") {
                    list.push(jid.to_string());
                }
            }
        }
    }
    list
}

/// Parses added and removed JIDs from a blocklist push notification or stanza.
pub fn parse_blocklist_notification(node: &BinaryNode) -> (Vec<String>, Vec<String>) {
    let mut added = Vec::new();
    let mut removed = Vec::new();

    let items = if let Some(bl) = node.get_child("blocklist") {
        bl.get_children()
    } else {
        node.get_children()
    };

    for item in items {
        if item.tag == "item" {
            let action = item.get_attr("action").unwrap_or("");
            if let Some(jid) = item.get_attr("jid") {
                if action == "block" || action == "add" {
                    added.push(jid.to_string());
                } else if action == "unblock" || action == "remove" {
                    removed.push(jid.to_string());
                }
            }
        }
    }

    (added, removed)
}

/// Builds an IQ node to query privacy settings.
pub fn build_privacy_query_iq(iq_id: &str) -> BinaryNode {
    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "privacy")
        .with_attr("type", "get")
        .with_attr("id", iq_id)
        .with_child(BinaryNode::new("privacy"))
}

/// Builds an IQ node to update a privacy category setting.
pub fn build_privacy_update_iq(iq_id: &str, category: &str, value: &str) -> BinaryNode {
    let cat_node = BinaryNode::new("category")
        .with_attr("name", category)
        .with_attr("value", value);
    let privacy_node = BinaryNode::new("privacy").with_child(cat_node);

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "privacy")
        .with_attr("type", "set")
        .with_attr("id", iq_id)
        .with_child(privacy_node)
}

/// Parses privacy settings from an IQ response.
pub fn parse_privacy_settings_response(node: &BinaryNode) -> PrivacySettings {
    let mut settings = PrivacySettings::default();
    if let Some(privacy) = node.get_child("privacy") {
        for cat in privacy.get_children() {
            if cat.tag == "category" {
                if let (Some(name), Some(val)) = (cat.get_attr("name"), cat.get_attr("value")) {
                    settings.raw.insert(name.to_string(), val.to_string());
                    match name {
                        "readreceipts" => settings.readreceipts = Some(val.to_string()),
                        "profile" => settings.profile = Some(val.to_string()),
                        "status" => settings.status = Some(val.to_string()),
                        "online" => settings.online = Some(val.to_string()),
                        "last" => settings.last = Some(val.to_string()),
                        "groupadd" => settings.groupadd = Some(val.to_string()),
                        "calladd" => settings.calladd = Some(val.to_string()),
                        _ => {}
                    }
                }
            }
        }
    }
    settings
}

/// Builds an IQ node to query About / Bio status for a JID.
pub fn build_fetch_status_iq(iq_id: &str, jid: &str) -> BinaryNode {
    let status_child = BinaryNode::new("status").with_attr("jid", jid);
    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "status")
        .with_attr("type", "get")
        .with_attr("id", iq_id)
        .with_child(status_child)
}

/// Builds an IQ node to update self About / Bio status.
pub fn build_update_profile_status_iq(iq_id: &str, status: &str) -> BinaryNode {
    let status_child = BinaryNode::new("status").with_content(status.as_bytes().to_vec());
    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "status")
        .with_attr("type", "set")
        .with_attr("id", iq_id)
        .with_child(status_child)
}

fn extract_node_string(node: &BinaryNode) -> Option<String> {
    if let Some(s) = node.content_as_string() {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(b) = node.content_as_bytes() {
        let s = String::from_utf8_lossy(b).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// Parses the About / Bio status from an IQ response.
pub fn parse_profile_status_response(node: &BinaryNode) -> Option<ProfileStatus> {
    if let Some(status_node) = node.get_child("status") {
        let set_at = status_node
            .get_attr("t")
            .and_then(|t| t.parse::<u64>().ok());

        let text = extract_node_string(status_node).unwrap_or_default();

        Some(ProfileStatus {
            status: text,
            set_at,
        })
    } else {
        None
    }
}

/// Builds an IQ node to query WhatsApp Business profile metadata.
pub fn build_business_profile_iq(iq_id: &str, jid: &str) -> BinaryNode {
    let profile_child = BinaryNode::new("profile").with_attr("jid", jid);
    let biz_child = BinaryNode::new("business_profile")
        .with_attr("v", "244")
        .with_child(profile_child);

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "w:biz")
        .with_attr("type", "get")
        .with_attr("id", iq_id)
        .with_child(biz_child)
}

/// Parses a business profile from an IQ response.
pub fn parse_business_profile_response(node: &BinaryNode) -> Option<BusinessProfile> {
    let biz_node = node.get_child("business_profile")?;
    let prof_node = biz_node.get_child("profile")?;

    let jid = prof_node.get_attr("jid").unwrap_or("").to_string();
    let mut profile = BusinessProfile {
        jid,
        ..Default::default()
    };

    if let Some(addr) = prof_node.get_child("address") {
        profile.address = extract_node_string(addr);
    }
    if let Some(desc) = prof_node.get_child("description") {
        profile.description = extract_node_string(desc);
    }
    if let Some(email) = prof_node.get_child("email") {
        profile.email = extract_node_string(email);
    }
    if let Some(categories) = prof_node.get_child("categories") {
        if let Some(first_cat) = categories.get_children().first() {
            profile.category = extract_node_string(first_cat);
        }
    }

    for child in prof_node.get_children() {
        if child.tag == "website" {
            if let Some(w) = extract_node_string(child) {
                profile.websites.push(w);
            }
        }
    }

    Some(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_fetch_blocklist_iq() {
        let iq = build_fetch_blocklist_iq("3EB0123");
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("blocklist"));
        assert_eq!(iq.get_attr("type"), Some("get"));
        assert!(iq.get_child("list").is_some());
    }

    #[test]
    fn test_build_block_iq_and_parse_response() {
        let iq = build_block_iq(
            "3EB0456",
            "628111111111@s.whatsapp.net",
            "block",
            Some("628111111111@s.whatsapp.net"),
        );
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("blocklist"));
        assert_eq!(iq.get_attr("type"), Some("set"));
        let item = iq.get_child("item").expect("item child missing");
        assert_eq!(item.get_attr("action"), Some("block"));
        assert_eq!(item.get_attr("jid"), Some("628111111111@s.whatsapp.net"));
        assert_eq!(item.get_attr("pn_jid"), Some("628111111111@s.whatsapp.net"));

        // Test parsing blocklist response
        let resp = BinaryNode::new("iq").with_child(
            BinaryNode::new("list")
                .with_child(BinaryNode::new("item").with_attr("jid", "user1@s.whatsapp.net"))
                .with_child(BinaryNode::new("item").with_attr("jid", "user2@s.whatsapp.net")),
        );
        let list = parse_blocklist_response(&resp);
        assert_eq!(list, vec!["user1@s.whatsapp.net", "user2@s.whatsapp.net"]);
    }

    #[test]
    fn test_parse_blocklist_notification() {
        let notif = BinaryNode::new("notification").with_child(
            BinaryNode::new("blocklist")
                .with_child(
                    BinaryNode::new("item")
                        .with_attr("action", "block")
                        .with_attr("jid", "blocked1@s.whatsapp.net"),
                )
                .with_child(
                    BinaryNode::new("item")
                        .with_attr("action", "unblock")
                        .with_attr("jid", "unblocked1@s.whatsapp.net"),
                ),
        );
        let (added, removed) = parse_blocklist_notification(&notif);
        assert_eq!(added, vec!["blocked1@s.whatsapp.net"]);
        assert_eq!(removed, vec!["unblocked1@s.whatsapp.net"]);
    }

    #[test]
    fn test_privacy_settings_iq_and_parse() {
        let q_iq = build_privacy_query_iq("IQ1");
        assert_eq!(q_iq.get_attr("xmlns"), Some("privacy"));
        assert!(q_iq.get_child("privacy").is_some());

        let u_iq = build_privacy_update_iq("IQ2", "last", "contacts");
        let priv_node = u_iq.get_child("privacy").unwrap();
        let cat = priv_node.get_child("category").unwrap();
        assert_eq!(cat.get_attr("name"), Some("last"));
        assert_eq!(cat.get_attr("value"), Some("contacts"));

        let resp = BinaryNode::new("iq").with_child(
            BinaryNode::new("privacy")
                .with_child(
                    BinaryNode::new("category")
                        .with_attr("name", "readreceipts")
                        .with_attr("value", "all"),
                )
                .with_child(
                    BinaryNode::new("category")
                        .with_attr("name", "last")
                        .with_attr("value", "contacts"),
                )
                .with_child(
                    BinaryNode::new("category")
                        .with_attr("name", "profile")
                        .with_attr("value", "all"),
                )
                .with_child(
                    BinaryNode::new("category")
                        .with_attr("name", "status")
                        .with_attr("value", "contacts"),
                )
                .with_child(
                    BinaryNode::new("category")
                        .with_attr("name", "online")
                        .with_attr("value", "match_last_seen"),
                ),
        );

        let parsed = parse_privacy_settings_response(&resp);
        assert_eq!(parsed.readreceipts.as_deref(), Some("all"));
        assert_eq!(parsed.last.as_deref(), Some("contacts"));
        assert_eq!(parsed.profile.as_deref(), Some("all"));
        assert_eq!(parsed.status.as_deref(), Some("contacts"));
        assert_eq!(parsed.online.as_deref(), Some("match_last_seen"));
        assert_eq!(parsed.raw.len(), 5);
    }

    #[test]
    fn test_status_and_business_profile_roundtrip() {
        let status_iq = build_fetch_status_iq("IQ3", "user@s.whatsapp.net");
        assert_eq!(status_iq.get_attr("xmlns"), Some("status"));

        let status_resp = BinaryNode::new("iq").with_child(
            BinaryNode::new("status")
                .with_attr("t", "1700000000")
                .with_content(b"Hey there! I am using WhatsApp.".to_vec()),
        );
        let st = parse_profile_status_response(&status_resp).expect("Failed to parse status");
        assert_eq!(st.status, "Hey there! I am using WhatsApp.");
        assert_eq!(st.set_at, Some(1700000000));

        let biz_iq = build_business_profile_iq("IQ4", "biz@s.whatsapp.net");
        assert_eq!(biz_iq.get_attr("xmlns"), Some("w:biz"));

        let biz_resp = BinaryNode::new("iq").with_child(
            BinaryNode::new("business_profile").with_child(
                BinaryNode::new("profile")
                    .with_attr("jid", "biz@s.whatsapp.net")
                    .with_child(BinaryNode::new("address").with_content(b"123 Tech Lane".to_vec()))
                    .with_child(
                        BinaryNode::new("description").with_content(b"Top-tier Software".to_vec()),
                    )
                    .with_child(
                        BinaryNode::new("email").with_content(b"info@techlane.com".to_vec()),
                    )
                    .with_child(
                        BinaryNode::new("website").with_content(b"https://techlane.com".to_vec()),
                    )
                    .with_child(BinaryNode::new("categories").with_child(
                        BinaryNode::new("category").with_content(b"Software Company".to_vec()),
                    )),
            ),
        );

        let bp =
            parse_business_profile_response(&biz_resp).expect("Failed to parse business profile");
        assert_eq!(bp.jid, "biz@s.whatsapp.net");
        assert_eq!(bp.address.as_deref(), Some("123 Tech Lane"));
        assert_eq!(bp.description.as_deref(), Some("Top-tier Software"));
        assert_eq!(bp.email.as_deref(), Some("info@techlane.com"));
        assert_eq!(bp.websites, vec!["https://techlane.com"]);
        assert_eq!(bp.category.as_deref(), Some("Software Company"));
    }
}
