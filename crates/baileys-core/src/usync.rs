//! WhatsApp Unified Sync (USync) and Multi-Device Protocol.
//!
//! Provides models, IQ stanza builders, and response parsers for:
//! - Querying companion devices for user JIDs (`<devices version="2"/>`)
//! - Checking if phone numbers are registered on WhatsApp (`onwa` / `<contact/>`)
//! - Removing / revoking companion devices (`<remove><device jid="..."/></remove>`)

use baileys_binary::jid::S_WHATSAPP_NET;
use baileys_binary::BinaryNode;
use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Details of a single device linked to a user account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRecord {
    pub id: u32,
    pub key_index: Option<u32>,
}

/// Active companion devices for a WhatsApp user JID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDevices {
    pub user_jid: String,
    pub devices: Vec<DeviceRecord>,
}

/// Result of checking whether a phone number is registered on WhatsApp.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnWhatsAppResult {
    pub jid: String,
    pub exists: bool,
    pub phone_number: String,
}

/// Builds a USync IQ stanza to query linked companion devices for a list of user JIDs.
pub fn build_usync_devices_iq(user_jids: &[&str]) -> BinaryNode {
    let iq_id = format!("{}.{}-1", rand::random::<u16>(), rand::random::<u16>());
    let sid = format!("{}.{}-2", rand::random::<u16>(), rand::random::<u16>());

    let user_nodes: Vec<BinaryNode> = user_jids
        .iter()
        .map(|&jid| {
            let normalized = if !jid.contains('@') {
                format!("{}@{}", jid, S_WHATSAPP_NET)
            } else {
                jid.to_string()
            };
            BinaryNode::new("user").with_attr("jid", normalized)
        })
        .collect();

    let has_non_lid = user_jids.iter().any(|&j| !j.ends_with("@lid"));
    let mut query_node =
        BinaryNode::new("query").with_child(BinaryNode::new("devices").with_attr("version", "2"));
    if has_non_lid {
        query_node = query_node.with_child(BinaryNode::new("lid"));
    }

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "usync")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("usync")
                .with_attr("sid", sid)
                .with_attr("mode", "query")
                .with_attr("last", "true")
                .with_attr("index", "0")
                .with_attr("context", "message")
                .with_child(query_node)
                .with_child(BinaryNode::new("list").with_children(user_nodes)),
        )
}

/// Builds a USync IQ stanza to check whether phone numbers exist on WhatsApp (`onwa`).
pub fn build_usync_contact_iq(phone_numbers: &[&str]) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let sid = format!("usync-contact-{:X}", rand::random::<u32>());

    let user_nodes: Vec<BinaryNode> = phone_numbers
        .iter()
        .map(|&phone| {
            let clean: String = phone
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '+')
                .collect();
            let formatted = if !clean.starts_with('+') {
                format!("+{}", clean)
            } else {
                clean
            };
            BinaryNode::new("user")
                .with_child(BinaryNode::new("contact").with_bytes_content(formatted.into_bytes()))
        })
        .collect();

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "usync")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("usync")
                .with_attr("sid", sid)
                .with_attr("mode", "query")
                .with_attr("last", "true")
                .with_attr("index", "0")
                .with_attr("context", "interactive")
                .with_child(BinaryNode::new("query").with_child(BinaryNode::new("contact")))
                .with_child(BinaryNode::new("list").with_children(user_nodes)),
        )
}

/// Builds an IQ stanza to revoke/remove a linked companion device.
pub fn build_remove_device_iq(device_jid: &str) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "set")
        .with_attr("xmlns", "md")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("remove")
                .with_child(BinaryNode::new("device").with_attr("jid", device_jid)),
        )
}

/// Parses a USync device query response into a list of `UserDevices`.
pub fn parse_usync_devices_response(node: &BinaryNode) -> Result<Vec<UserDevices>, CoreError> {
    let usync_node = node
        .get_child("usync")
        .or(Some(node))
        .ok_or_else(|| CoreError::Protocol("Missing usync node in response".to_string()))?;

    let list_node = match usync_node.get_child("list") {
        Some(l) => l,
        None => return Ok(Vec::new()),
    };

    let mut results = Vec::new();

    for user_node in list_node.get_children().iter().filter(|c| c.tag == "user") {
        let user_jid = match user_node
            .get_attr("jid")
            .or_else(|| user_node.get_attr("id"))
        {
            Some(j) => j.to_string(),
            None => continue,
        };

        let mut devices = Vec::new();

        if let Some(devices_node) = user_node.get_child("devices") {
            if let Some(device_list) = devices_node.get_child("device-list") {
                for dev_node in device_list
                    .get_children()
                    .iter()
                    .filter(|c| c.tag == "device")
                {
                    if let Some(id_str) = dev_node.get_attr("id") {
                        if let Ok(dev_id) = id_str.parse::<u32>() {
                            let key_index = dev_node
                                .get_attr("key-index")
                                .and_then(|k| k.parse::<u32>().ok());
                            devices.push(DeviceRecord {
                                id: dev_id,
                                key_index,
                            });
                        }
                    }
                }
            }
        }

        results.push(UserDevices { user_jid, devices });
    }

    Ok(results)
}

/// Parses a USync contact check (`onwa`) response into `Vec<OnWhatsAppResult>`.
pub fn parse_usync_contact_response(node: &BinaryNode) -> Result<Vec<OnWhatsAppResult>, CoreError> {
    let usync_node = node
        .get_child("usync")
        .or(Some(node))
        .ok_or_else(|| CoreError::Protocol("Missing usync node in response".to_string()))?;

    let list_node = match usync_node.get_child("list") {
        Some(l) => l,
        None => return Ok(Vec::new()),
    };

    let mut results = Vec::new();

    for user_node in list_node.get_children().iter().filter(|c| c.tag == "user") {
        let is_error = user_node.get_attr("error").is_some();
        let user_jid = user_node.get_attr("jid").unwrap_or_default().to_string();

        let mut phone_number = String::new();
        let mut contact_type = String::new();

        if let Some(contact_node) = user_node.get_child("contact") {
            contact_type = contact_node
                .get_attr("type")
                .unwrap_or_default()
                .to_string();
            if let Some(bytes) = contact_node.content_as_bytes() {
                phone_number = String::from_utf8_lossy(bytes).trim().to_string();
            }
        }

        let exists = !is_error && contact_type != "out" && !user_jid.is_empty();

        let final_jid = if exists {
            user_jid
        } else {
            let clean: String = phone_number
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            format!("{}@{}", clean, S_WHATSAPP_NET)
        };

        results.push(OnWhatsAppResult {
            jid: final_jid,
            exists,
            phone_number,
        });
    }

    Ok(results)
}

/// Builds a USync IQ stanza to query Phone Number <-> LID mappings.
pub fn build_usync_lid_iq(jids: &[&str]) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let sid = format!("usync-lid-{:X}", rand::random::<u32>());

    let user_nodes: Vec<BinaryNode> = jids
        .iter()
        .map(|&jid| {
            if jid.ends_with("@lid") {
                BinaryNode::new("user").with_child(BinaryNode::new("lid").with_attr("jid", jid))
            } else {
                let normalized = if !jid.contains('@') {
                    format!("{}@{}", jid, S_WHATSAPP_NET)
                } else {
                    jid.to_string()
                };
                BinaryNode::new("user").with_attr("jid", normalized)
            }
        })
        .collect();

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "usync")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("usync")
                .with_attr("sid", sid)
                .with_attr("mode", "query")
                .with_attr("last", "true")
                .with_attr("index", "0")
                .with_attr("context", "interactive")
                .with_child(BinaryNode::new("query").with_child(BinaryNode::new("lid")))
                .with_child(BinaryNode::new("list").with_children(user_nodes)),
        )
}

/// Parses a USync LID mapping response into a list of `(from_jid, mapped_jid)`.
pub fn parse_usync_lid_response(node: &BinaryNode) -> Result<Vec<(String, String)>, CoreError> {
    let usync_node = node
        .get_child("usync")
        .or(Some(node))
        .ok_or_else(|| CoreError::Protocol("Missing usync node in response".to_string()))?;

    let list_node = match usync_node.get_child("list") {
        Some(l) => l,
        None => return Ok(Vec::new()),
    };

    let mut mappings = Vec::new();

    for user_node in list_node.get_children().iter().filter(|c| c.tag == "user") {
        let user_jid = user_node.get_attr("jid").unwrap_or_default().to_string();
        if let Some(lid_node) = user_node.get_child("lid") {
            if let Some(val) = lid_node.get_attr("val") {
                let mapped = if val.contains('@') {
                    val.to_string()
                } else if user_jid.ends_with("@lid") {
                    format!("{}@{}", val, S_WHATSAPP_NET)
                } else {
                    format!("{}@lid", val)
                };
                mappings.push((user_jid, mapped));
            }
        }
    }

    Ok(mappings)
}

/// Builds a USync IQ stanza to query the default disappearing-messages timer of users.
pub fn build_usync_disappearing_mode_iq(jids: &[&str]) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let sid = format!("usync-disappearing-{:X}", rand::random::<u32>());

    let user_nodes: Vec<BinaryNode> = jids
        .iter()
        .map(|&jid| {
            let normalized = if !jid.contains('@') {
                format!("{}@{}", jid, S_WHATSAPP_NET)
            } else {
                jid.to_string()
            };
            BinaryNode::new("user").with_attr("jid", normalized)
        })
        .collect();

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "usync")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("usync")
                .with_attr("sid", sid)
                .with_attr("mode", "query")
                .with_attr("last", "true")
                .with_attr("index", "0")
                .with_attr("context", "interactive")
                .with_child(
                    BinaryNode::new("query").with_child(BinaryNode::new("disappearing_mode")),
                )
                .with_child(BinaryNode::new("list").with_children(user_nodes)),
        )
}

/// Parses a disappearing-mode USync response into `(user_jid, duration_secs, set_at_timestamp)`.
pub fn parse_usync_disappearing_mode_response(
    node: &BinaryNode,
) -> Result<Vec<(String, u32, u64)>, CoreError> {
    let usync_node = node.get_child("usync").unwrap_or(node);
    let Some(list_node) = usync_node.get_child("list") else {
        return Ok(Vec::new());
    };
    let mut results = Vec::new();
    for user_node in list_node.get_children().iter().filter(|c| c.tag == "user") {
        let Some(jid) = user_node.get_attr("jid") else {
            continue;
        };
        if let Some(mode) = user_node.get_child("disappearing_mode") {
            if let Some(err) = mode.get_attr("error") {
                return Err(CoreError::Protocol(format!(
                    "disappearing_mode query failed for {jid}: {err}"
                )));
            }
            let duration = mode
                .get_attr("duration")
                .and_then(|d| d.parse::<u32>().ok())
                .unwrap_or(0);
            let set_at = mode
                .get_attr("t")
                .and_then(|t| t.parse::<u64>().ok())
                .unwrap_or(0);
            results.push((jid.to_string(), duration, set_at));
        }
    }
    Ok(results)
}

/// Builds the IQ that sets our default disappearing-messages timer for new chats
/// (`0` turns it off).
pub fn build_default_disappearing_mode_iq(duration_secs: u32) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    BinaryNode::new("iq")
        .with_attr("xmlns", "disappearing_mode")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "set")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("disappearing_mode").with_attr("duration", duration_secs.to_string()),
        )
}

/// Builds a USync IQ stanza to query user About / Status strings.
pub fn build_usync_status_iq(jids: &[&str]) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let sid = format!("usync-status-{:X}", rand::random::<u32>());

    let user_nodes: Vec<BinaryNode> = jids
        .iter()
        .map(|&jid| {
            let normalized = if !jid.contains('@') {
                format!("{}@{}", jid, S_WHATSAPP_NET)
            } else {
                jid.to_string()
            };
            BinaryNode::new("user").with_attr("jid", normalized)
        })
        .collect();

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "usync")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("usync")
                .with_attr("sid", sid)
                .with_attr("mode", "query")
                .with_attr("last", "true")
                .with_attr("index", "0")
                .with_attr("context", "interactive")
                .with_child(BinaryNode::new("query").with_child(BinaryNode::new("status")))
                .with_child(BinaryNode::new("list").with_children(user_nodes)),
        )
}

/// Parses a USync status query response into `Vec<(user_jid, status_text, set_at_timestamp)>`.
pub fn parse_usync_status_response(
    node: &BinaryNode,
) -> Result<Vec<(String, String, u64)>, CoreError> {
    let usync_node = node
        .get_child("usync")
        .or(Some(node))
        .ok_or_else(|| CoreError::Protocol("Missing usync node in response".to_string()))?;

    let list_node = match usync_node.get_child("list") {
        Some(l) => l,
        None => return Ok(Vec::new()),
    };

    let mut results = Vec::new();

    for user_node in list_node.get_children().iter().filter(|c| c.tag == "user") {
        let user_jid = match user_node.get_attr("jid") {
            Some(j) => j.to_string(),
            None => continue,
        };

        if let Some(status_node) = user_node.get_child("status") {
            let set_at = status_node
                .get_attr("t")
                .and_then(|t| t.parse::<u64>().ok())
                .unwrap_or(0);

            let status_text = status_node
                .content_as_string()
                .map(|s| s.to_string())
                .or_else(|| {
                    status_node
                        .content_as_bytes()
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                })
                .unwrap_or_default();

            results.push((user_jid, status_text, set_at));
        }
    }

    Ok(results)
}

/// Builds a USync IQ stanza to look up an account by its @username and optional 4-digit PIN.
pub fn build_usync_username_lookup_iq(username: &str, pin: Option<&str>) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let sid = format!("usync-username-{:X}", rand::random::<u32>());

    let mut contact_node = BinaryNode::new("contact").with_attr("username", username);
    if let Some(p) = pin {
        contact_node = contact_node.with_attr("pin", p);
    }

    let user_node = BinaryNode::new("user").with_child(contact_node);

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "get")
        .with_attr("xmlns", "usync")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("usync")
                .with_attr("sid", sid)
                .with_attr("mode", "query")
                .with_attr("last", "true")
                .with_attr("index", "0")
                .with_attr("context", "interactive")
                .with_child(
                    BinaryNode::new("query")
                        .with_child(BinaryNode::new("contact").with_attr("addressing_mode", "lid"))
                        .with_child(
                            BinaryNode::new("business")
                                .with_child(BinaryNode::new("verified_name")),
                        ),
                )
                .with_child(BinaryNode::new("list").with_child(user_node)),
        )
}

/// Parses the USync username lookup response and returns the resolved identity if found.
pub fn parse_usync_username_response(
    response: &BinaryNode,
    username: &str,
) -> Result<Option<crate::username::ResolvedUsername>, CoreError> {
    let usync = response
        .get_child("usync")
        .ok_or_else(|| CoreError::Protocol("Missing <usync> node in response".into()))?;

    let list = usync
        .get_child("list")
        .ok_or_else(|| CoreError::Protocol("Missing <list> node in usync response".into()))?;

    for user in list.get_children() {
        if user.tag != "user" {
            continue;
        }

        if user.get_attr("error") == Some("404") {
            return Ok(None);
        }

        let jid = user
            .get_attr("jid")
            .or_else(|| user.get_attr("lid"))
            .map(|s| s.to_string());

        let phone_jid = user
            .get_child("contact")
            .and_then(|c| c.get_attr("jid"))
            .map(|s| s.to_string());

        let verified_name = user
            .get_child("business")
            .and_then(|b| b.get_child("verified_name"))
            .and_then(|v| v.content_as_string().map(|s| s.to_string()));

        if let Some(target_jid) = jid.or_else(|| phone_jid.clone()) {
            return Ok(Some(crate::username::ResolvedUsername {
                username: username.to_string(),
                jid: target_jid,
                phone_jid,
                verified_name,
            }));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_usync_devices_iq() {
        let iq = build_usync_devices_iq(&["62812345678", "62898765432@s.whatsapp.net"]);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("usync"));
        assert_eq!(iq.get_attr("type"), Some("get"));

        let usync = iq.get_child("usync").expect("Missing usync");
        assert_eq!(usync.get_attr("mode"), Some("query"));

        let list = usync.get_child("list").expect("Missing list");
        let users: Vec<&BinaryNode> = list
            .get_children()
            .iter()
            .filter(|c| c.tag == "user")
            .collect();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].get_attr("jid"), Some("62812345678@s.whatsapp.net"));
        assert_eq!(users[1].get_attr("jid"), Some("62898765432@s.whatsapp.net"));
    }

    #[test]
    fn test_build_usync_contact_iq() {
        let iq = build_usync_contact_iq(&["62812345678", "+15551234567"]);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("usync"));

        let usync = iq.get_child("usync").expect("Missing usync");
        let list = usync.get_child("list").expect("Missing list");
        let users: Vec<&BinaryNode> = list
            .get_children()
            .iter()
            .filter(|c| c.tag == "user")
            .collect();
        assert_eq!(users.len(), 2);

        let c1 = users[0].get_child("contact").expect("Missing contact");
        assert_eq!(c1.content_as_bytes(), Some("+62812345678".as_bytes()));
    }

    #[test]
    fn test_build_remove_device_iq() {
        let iq = build_remove_device_iq("62812345678:42@s.whatsapp.net");
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("md"));
        assert_eq!(iq.get_attr("type"), Some("set"));

        let remove = iq.get_child("remove").expect("Missing remove");
        let dev = remove.get_child("device").expect("Missing device");
        assert_eq!(dev.get_attr("jid"), Some("62812345678:42@s.whatsapp.net"));
    }

    #[test]
    fn test_parse_usync_devices_response() {
        let response = BinaryNode::new("iq")
            .with_attr("type", "result")
            .with_child(
                BinaryNode::new("usync")
                    .with_attr("mode", "query")
                    .with_child(
                        BinaryNode::new("list").with_child(
                            BinaryNode::new("user")
                                .with_attr("jid", "62812345678@s.whatsapp.net")
                                .with_child(
                                    BinaryNode::new("devices").with_child(
                                        BinaryNode::new("device-list")
                                            .with_child(
                                                BinaryNode::new("device").with_attr("id", "0"),
                                            )
                                            .with_child(
                                                BinaryNode::new("device")
                                                    .with_attr("id", "37")
                                                    .with_attr("key-index", "1"),
                                            ),
                                    ),
                                ),
                        ),
                    ),
            );

        let parsed = parse_usync_devices_response(&response).expect("Failed to parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].user_jid, "62812345678@s.whatsapp.net");
        assert_eq!(parsed[0].devices.len(), 2);
        assert_eq!(parsed[0].devices[0].id, 0);
        assert_eq!(parsed[0].devices[1].id, 37);
        assert_eq!(parsed[0].devices[1].key_index, Some(1));
    }

    #[test]
    fn test_parse_usync_contact_response() {
        let response = BinaryNode::new("iq")
            .with_attr("type", "result")
            .with_child(
                BinaryNode::new("usync").with_child(
                    BinaryNode::new("list")
                        .with_child(
                            BinaryNode::new("user")
                                .with_attr("jid", "62812345678@s.whatsapp.net")
                                .with_child(
                                    BinaryNode::new("contact")
                                        .with_attr("type", "in")
                                        .with_bytes_content("+62812345678".as_bytes().to_vec()),
                                ),
                        )
                        .with_child(
                            BinaryNode::new("user")
                                .with_attr("error", "404")
                                .with_child(
                                    BinaryNode::new("contact")
                                        .with_attr("type", "out")
                                        .with_bytes_content("+15550009999".as_bytes().to_vec()),
                                ),
                        ),
                ),
            );

        let parsed = parse_usync_contact_response(&response).expect("Failed to parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].phone_number, "+62812345678");
        assert!(parsed[0].exists);
        assert_eq!(parsed[0].jid, "62812345678@s.whatsapp.net");

        assert_eq!(parsed[1].phone_number, "+15550009999");
        assert!(!parsed[1].exists);
    }

    #[test]
    fn test_usync_lid_roundtrip() {
        let iq = build_usync_lid_iq(&["62812345678@s.whatsapp.net", "12345678@lid"]);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("usync"));
        let usync = iq.get_child("usync").expect("Missing usync");
        let query = usync.get_child("query").expect("Missing query");
        assert!(query.get_child("lid").is_some());

        let response = BinaryNode::new("iq")
            .with_attr("type", "result")
            .with_child(
                BinaryNode::new("usync").with_child(
                    BinaryNode::new("list")
                        .with_child(
                            BinaryNode::new("user")
                                .with_attr("jid", "62812345678@s.whatsapp.net")
                                .with_child(
                                    BinaryNode::new("lid").with_attr("val", "99887766@lid"),
                                ),
                        )
                        .with_child(
                            BinaryNode::new("user")
                                .with_attr("jid", "12345678@lid")
                                .with_child(
                                    BinaryNode::new("lid")
                                        .with_attr("val", "62898765432@s.whatsapp.net"),
                                ),
                        ),
                ),
            );

        let parsed = parse_usync_lid_response(&response).expect("Failed to parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0],
            (
                "62812345678@s.whatsapp.net".to_string(),
                "99887766@lid".to_string()
            )
        );
        assert_eq!(
            parsed[1],
            (
                "12345678@lid".to_string(),
                "62898765432@s.whatsapp.net".to_string()
            )
        );
    }

    #[test]
    fn test_usync_status_roundtrip() {
        let iq = build_usync_status_iq(&["62812345678@s.whatsapp.net"]);
        assert_eq!(iq.tag, "iq");
        let usync = iq.get_child("usync").expect("Missing usync");
        let query = usync.get_child("query").expect("Missing query");
        assert!(query.get_child("status").is_some());

        let response = BinaryNode::new("iq")
            .with_attr("type", "result")
            .with_child(
                BinaryNode::new("usync").with_child(
                    BinaryNode::new("list").with_child(
                        BinaryNode::new("user")
                            .with_attr("jid", "62812345678@s.whatsapp.net")
                            .with_child(
                                BinaryNode::new("status")
                                    .with_attr("t", "1728100000")
                                    .with_bytes_content(
                                        "Hey there! I am using WhatsApp.".as_bytes().to_vec(),
                                    ),
                            ),
                    ),
                ),
            );

        let parsed = parse_usync_status_response(&response).expect("Failed to parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "62812345678@s.whatsapp.net");
        assert_eq!(parsed[0].1, "Hey there! I am using WhatsApp.");
        assert_eq!(parsed[0].2, 1728100000);
    }

    #[test]
    fn test_usync_username_lookup_roundtrip() {
        let iq = build_usync_username_lookup_iq("alice", Some("1234"));
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("usync"));
        let usync = iq.get_child("usync").expect("Missing usync");
        let list = usync.get_child("list").expect("Missing list");
        let user = list.get_child("user").expect("Missing user");
        let contact = user.get_child("contact").expect("Missing contact");
        assert_eq!(contact.get_attr("username"), Some("alice"));
        assert_eq!(contact.get_attr("pin"), Some("1234"));

        let response = BinaryNode::new("iq")
            .with_attr("type", "result")
            .with_child(
                BinaryNode::new("usync").with_child(
                    BinaryNode::new("list").with_child(
                        BinaryNode::new("user")
                            .with_attr("jid", "100200300@lid")
                            .with_child(
                                BinaryNode::new("contact")
                                    .with_attr("jid", "62812345678@s.whatsapp.net"),
                            )
                            .with_child(
                                BinaryNode::new("business").with_child(
                                    BinaryNode::new("verified_name")
                                        .with_bytes_content("Alice Store".as_bytes().to_vec()),
                                ),
                            ),
                    ),
                ),
            );

        let resolved = parse_usync_username_response(&response, "alice")
            .expect("Failed to parse")
            .expect("Expected resolved identity");
        assert_eq!(resolved.username, "alice");
        assert_eq!(resolved.jid, "100200300@lid");
        assert_eq!(
            resolved.phone_jid,
            Some("62812345678@s.whatsapp.net".to_string())
        );
        assert_eq!(resolved.verified_name, Some("Alice Store".to_string()));
    }

    #[test]
    fn disappearing_mode_query_and_response() {
        let iq = build_usync_disappearing_mode_iq(&["628111", "629@s.whatsapp.net"]);
        let usync = iq.get_child("usync").unwrap();
        assert!(usync
            .get_child("query")
            .unwrap()
            .get_child("disappearing_mode")
            .is_some());
        let jids: Vec<_> = usync
            .get_child("list")
            .unwrap()
            .get_children()
            .iter()
            .map(|u| u.get_attr("jid").unwrap().to_string())
            .collect();
        assert_eq!(jids, vec!["628111@s.whatsapp.net", "629@s.whatsapp.net"]);

        let response = BinaryNode::new("iq").with_child(
            BinaryNode::new("usync").with_child(
                BinaryNode::new("list").with_child(
                    BinaryNode::new("user")
                        .with_attr("jid", "628111@s.whatsapp.net")
                        .with_child(
                            BinaryNode::new("disappearing_mode")
                                .with_attr("duration", "86400")
                                .with_attr("t", "1700000000"),
                        ),
                ),
            ),
        );
        assert_eq!(
            parse_usync_disappearing_mode_response(&response).unwrap(),
            vec![("628111@s.whatsapp.net".to_string(), 86400, 1_700_000_000)]
        );

        let failed = BinaryNode::new("iq").with_child(
            BinaryNode::new("usync").with_child(
                BinaryNode::new("list").with_child(
                    BinaryNode::new("user")
                        .with_attr("jid", "1@s.whatsapp.net")
                        .with_child(BinaryNode::new("disappearing_mode").with_attr("error", "401")),
                ),
            ),
        );
        assert!(parse_usync_disappearing_mode_response(&failed).is_err());

        let set = build_default_disappearing_mode_iq(604800);
        assert_eq!(set.get_attr("xmlns"), Some("disappearing_mode"));
        assert_eq!(set.get_attr("type"), Some("set"));
        assert_eq!(
            set.get_child("disappearing_mode")
                .unwrap()
                .get_attr("duration"),
            Some("604800")
        );
    }
}
