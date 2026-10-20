//! Account management protocol including Terms of Service (ToS) and linked email verification.

use baileys_binary::BinaryNode;
use serde::{Deserialize, Serialize};

use crate::CoreError;

const S_WHATSAPP_NET: &str = "s.whatsapp.net";

/// Notice entry returned by the WhatsApp Terms of Service gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TosNotice {
    pub id: String,
    pub stage: Option<String>,
    pub state: Option<String>,
}

/// Builds an IQ stanza to query the state of Terms of Service notices.
pub fn build_tos_query_iq(iq_id: &str, notice_ids: &[&str]) -> BinaryNode {
    let notices = notice_ids
        .iter()
        .map(|id| BinaryNode::new("notice").with_attr("id", *id))
        .collect();

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "tos")
        .with_attr("type", "get")
        .with_attr("id", iq_id)
        .with_children(notices)
}

/// Builds an IQ stanza to accept or update Terms of Service notice stages.
pub fn build_tos_update_iq(iq_id: &str, notice_ids: &[&str], stage: Option<&str>) -> BinaryNode {
    let notices = notice_ids
        .iter()
        .map(|id| {
            let mut node = BinaryNode::new("notice").with_attr("id", *id);
            if let Some(s) = stage {
                node = node.with_attr("stage", s);
            } else {
                node = node.with_attr("state", "accept");
            }
            node
        })
        .collect();

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "tos")
        .with_attr("type", "set")
        .with_attr("id", iq_id)
        .with_children(notices)
}

/// Parses notice items from a ToS query response stanza.
pub fn parse_tos_response(node: &BinaryNode) -> Result<Vec<TosNotice>, CoreError> {
    let mut notices = Vec::new();

    for child in node.get_children() {
        if child.tag == "notice" {
            if let Some(id) = child.get_attr("id") {
                notices.push(TosNotice {
                    id: id.to_string(),
                    stage: child.get_attr("stage").map(|s| s.to_string()),
                    state: child.get_attr("state").map(|s| s.to_string()),
                });
            }
        }
    }

    Ok(notices)
}

/// Builds an IQ stanza to query the account's linked email address.
pub fn build_get_email_iq(iq_id: &str) -> BinaryNode {
    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "urn:xmpp:whatsapp:account")
        .with_attr("type", "get")
        .with_attr("id", iq_id)
        .with_child(BinaryNode::new("email"))
}

/// Builds an IQ stanza to associate a recovery email address with the account.
pub fn build_set_email_iq(iq_id: &str, email: &str) -> BinaryNode {
    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "urn:xmpp:whatsapp:account")
        .with_attr("type", "set")
        .with_attr("id", iq_id)
        .with_child(BinaryNode::new("email").with_attr("address", email))
}

/// Builds an IQ stanza to request a 6-digit verification code sent to the email.
pub fn build_request_email_code_iq(iq_id: &str, lang: &str, locale: &str) -> BinaryNode {
    let code_req = BinaryNode::new("code")
        .with_attr("lg", lang)
        .with_attr("lc", locale);

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "urn:xmpp:whatsapp:account")
        .with_attr("type", "set")
        .with_attr("id", iq_id)
        .with_child(BinaryNode::new("email").with_child(code_req))
}

/// Builds an IQ stanza to verify an email confirmation code.
pub fn build_verify_email_code_iq(iq_id: &str, code: &str) -> BinaryNode {
    let code_verify = BinaryNode::new("code").with_attr("value", code);

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "urn:xmpp:whatsapp:account")
        .with_attr("type", "set")
        .with_attr("id", iq_id)
        .with_child(BinaryNode::new("email").with_child(code_verify))
}

/// Parses the verified or pending email address from an account query response.
pub fn parse_email_response(node: &BinaryNode) -> Result<Option<String>, CoreError> {
    if let Some(email_node) = node.get_child("email") {
        if let Some(addr) = email_node.get_attr("address") {
            return Ok(Some(addr.to_string()));
        }
        if let Some(s) = email_node.content_as_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed.to_string()));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tos_query_builder() {
        let iq = build_tos_query_iq("tos1", &["20601218", "20601219"]);
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr("xmlns"), Some("tos"));
        assert_eq!(iq.get_attr("type"), Some("get"));
        let children = iq.get_children();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].get_attr("id"), Some("20601218"));
        assert_eq!(children[1].get_attr("id"), Some("20601219"));
    }

    #[test]
    fn test_tos_update_builder() {
        let iq = build_tos_update_iq("tos2", &["20601218"], Some("5"));
        assert_eq!(iq.get_attr("type"), Some("set"));
        let notice = &iq.get_children()[0];
        assert_eq!(notice.get_attr("id"), Some("20601218"));
        assert_eq!(notice.get_attr("stage"), Some("5"));
    }

    #[test]
    fn test_parse_tos_response() {
        let resp = BinaryNode::new("iq").with_children(vec![
            BinaryNode::new("notice")
                .with_attr("id", "20601218")
                .with_attr("stage", "5")
                .with_attr("state", "accept"),
            BinaryNode::new("notice").with_attr("id", "20601219"),
        ]);
        let parsed = parse_tos_response(&resp).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "20601218");
        assert_eq!(parsed[0].stage.as_deref(), Some("5"));
        assert_eq!(parsed[0].state.as_deref(), Some("accept"));
        assert_eq!(parsed[1].id, "20601219");
        assert_eq!(parsed[1].stage, None);
    }

    #[test]
    fn test_email_iq_builders() {
        let get_iq = build_get_email_iq("e1");
        assert_eq!(get_iq.get_attr("xmlns"), Some("urn:xmpp:whatsapp:account"));
        assert_eq!(get_iq.get_attr("type"), Some("get"));

        let set_iq = build_set_email_iq("e2", "alice@example.com");
        let email = set_iq.get_child("email").unwrap();
        assert_eq!(email.get_attr("address"), Some("alice@example.com"));

        let req_iq = build_request_email_code_iq("e3", "en", "US");
        let code = req_iq
            .get_child("email")
            .unwrap()
            .get_child("code")
            .unwrap();
        assert_eq!(code.get_attr("lg"), Some("en"));
        assert_eq!(code.get_attr("lc"), Some("US"));

        let verify_iq = build_verify_email_code_iq("e4", "123456");
        let vcode = verify_iq
            .get_child("email")
            .unwrap()
            .get_child("code")
            .unwrap();
        assert_eq!(vcode.get_attr("value"), Some("123456"));
    }

    #[test]
    fn test_parse_email_response() {
        let resp_attr = BinaryNode::new("iq")
            .with_child(BinaryNode::new("email").with_attr("address", "alice@example.com"));
        assert_eq!(
            parse_email_response(&resp_attr).unwrap(),
            Some("alice@example.com".to_string())
        );

        let resp_text = BinaryNode::new("iq")
            .with_child(BinaryNode::new("email").with_string_content("bob@example.com"));
        assert_eq!(
            parse_email_response(&resp_text).unwrap(),
            Some("bob@example.com".to_string())
        );

        let resp_none = BinaryNode::new("iq").with_child(BinaryNode::new("other"));
        assert_eq!(parse_email_response(&resp_none).unwrap(), None);
    }
}
