use crate::events::CallEvent;
use crate::CoreError;
use baileys_binary::BinaryNode;

/// Builds a `<call>` stanza node to reject an incoming WhatsApp call.
pub fn build_reject_call_node(me_jid: &str, call_id: &str, call_from: &str) -> BinaryNode {
    BinaryNode::new("call")
        .with_attr("from", me_jid)
        .with_attr("to", call_from)
        .with_child(
            BinaryNode::new("reject")
                .with_attr("call-id", call_id)
                .with_attr("call-creator", call_from)
                .with_attr("count", "0"),
        )
}

/// Determines the call status string from the first child node inside `<call>`.
pub fn get_call_status_from_child(child: &BinaryNode) -> String {
    match child.tag.as_str() {
        "offer" | "offer_notice" => "offer".to_string(),
        "terminate" => {
            if child.get_attr("reason") == Some("timeout") {
                "timeout".to_string()
            } else {
                "terminate".to_string()
            }
        }
        "reject" => "reject".to_string(),
        "accept" => "accept".to_string(),
        "ringing" => "ringing".to_string(),
        "preaccept" => "preaccept".to_string(),
        "transport" => "transport".to_string(),
        "relaylatency" => "relaylatency".to_string(),
        other => other.to_string(),
    }
}

/// Parses an incoming `<call>` stanza into a structured `CallEvent`.
pub fn parse_call_stanza(node: &BinaryNode) -> Result<CallEvent, CoreError> {
    let chat_jid = node
        .get_attr("from")
        .ok_or_else(|| CoreError::Protocol("Call stanza missing 'from' attribute".into()))?
        .to_string();

    let timestamp = node
        .get_attr("t")
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });

    let offline = node.get_attr("offline").is_some();

    // Look for the primary action/info child (offer, ringing, terminate, reject, accept, relaylatency)
    let info_child = node
        .get_children()
        .first()
        .ok_or_else(|| CoreError::Protocol("Call stanza missing child node".into()))?;

    let status = get_call_status_from_child(info_child);

    let call_id = info_child
        .get_attr("call-id")
        .or_else(|| node.get_attr("id"))
        .unwrap_or("")
        .to_string();

    let from = info_child
        .get_attr("from")
        .or_else(|| info_child.get_attr("call-creator"))
        .unwrap_or(&chat_jid)
        .to_string();

    let caller_pn = info_child.get_attr("caller_pn").map(|s| s.to_string());

    let is_video = info_child.get_child("video").is_some();
    let is_group =
        info_child.get_attr("type") == Some("group") || info_child.get_attr("group-jid").is_some();
    let group_jid = info_child.get_attr("group-jid").map(|s| s.to_string());

    let latency_ms = if status == "relaylatency" {
        info_child
            .get_attr("latency")
            .or_else(|| info_child.get_attr("latency_ms"))
            .or_else(|| info_child.get_attr("latency-ms"))
            .and_then(|l| l.parse::<u32>().ok())
    } else {
        None
    };

    Ok(CallEvent {
        id: call_id,
        chat_jid,
        from,
        caller_pn,
        is_group,
        group_jid,
        is_video,
        status,
        timestamp,
        offline,
        latency_ms,
    })
}

/// Builds a `<call>` stanza node to generate a WhatsApp call link.
pub fn build_create_call_link_node(media_type: &str, event_start_time: Option<u64>) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let mut link_create = BinaryNode::new("link_create").with_attr("media", media_type);

    if let Some(st) = event_start_time {
        link_create = link_create
            .with_child(BinaryNode::new("event").with_attr("start_time", st.to_string()));
    }

    BinaryNode::new("call")
        .with_attr("id", msg_id)
        .with_attr("to", "@call")
        .with_child(link_create)
}

/// Parses the generated call link token from a `<call>` query result.
pub fn parse_call_link_response(node: &BinaryNode) -> Option<String> {
    let child = node.get_child("link_create")?;
    child.get_attr("token").map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_reject_call_node() {
        let node = build_reject_call_node(
            "6283162498175:37@s.whatsapp.net",
            "CALL_12345",
            "1234567890@s.whatsapp.net",
        );
        assert_eq!(node.tag, "call");
        assert_eq!(
            node.get_attr("from"),
            Some("6283162498175:37@s.whatsapp.net")
        );
        assert_eq!(node.get_attr("to"), Some("1234567890@s.whatsapp.net"));

        let reject = node.get_child("reject").expect("Missing reject child");
        assert_eq!(reject.get_attr("call-id"), Some("CALL_12345"));
        assert_eq!(
            reject.get_attr("call-creator"),
            Some("1234567890@s.whatsapp.net")
        );
        assert_eq!(reject.get_attr("count"), Some("0"));
    }

    #[test]
    fn test_parse_call_offer_stanza() {
        let stanza = BinaryNode::new("call")
            .with_attr("from", "1234567890@s.whatsapp.net")
            .with_attr("id", "CALL_STZ_001")
            .with_attr("t", "1700000000")
            .with_child(
                BinaryNode::new("offer")
                    .with_attr("call-id", "CID_ABC_999")
                    .with_attr("call-creator", "1234567890@s.whatsapp.net")
                    .with_attr("caller_pn", "1234567890")
                    .with_child(BinaryNode::new("video")),
            );

        let event = parse_call_stanza(&stanza).expect("Failed to parse call offer");
        assert_eq!(event.id, "CID_ABC_999");
        assert_eq!(event.chat_jid, "1234567890@s.whatsapp.net");
        assert_eq!(event.from, "1234567890@s.whatsapp.net");
        assert_eq!(event.caller_pn.as_deref(), Some("1234567890"));
        assert_eq!(event.status, "offer");
        assert!(event.is_video);
        assert_eq!(event.timestamp, 1700000000);
        assert!(!event.is_group);
    }

    #[test]
    fn test_parse_call_terminate_stanza() {
        let stanza = BinaryNode::new("call")
            .with_attr("from", "1234567890@s.whatsapp.net")
            .with_attr("id", "CALL_STZ_002")
            .with_attr("t", "1700000015")
            .with_child(
                BinaryNode::new("terminate")
                    .with_attr("call-id", "CID_ABC_999")
                    .with_attr("call-creator", "1234567890@s.whatsapp.net")
                    .with_attr("reason", "timeout"),
            );

        let event = parse_call_stanza(&stanza).expect("Failed to parse call terminate");
        assert_eq!(event.id, "CID_ABC_999");
        assert_eq!(event.status, "timeout");
        assert_eq!(event.timestamp, 1700000015);
    }

    #[test]
    fn test_create_call_link_node_and_parse() {
        let node = build_create_call_link_node("video", Some(1728100000));
        assert_eq!(node.tag, "call");
        assert_eq!(node.get_attr("to"), Some("@call"));
        let link_create = node.get_child("link_create").expect("Missing link_create");
        assert_eq!(link_create.get_attr("media"), Some("video"));
        let event = link_create.get_child("event").expect("Missing event");
        assert_eq!(event.get_attr("start_time"), Some("1728100000"));

        let resp = BinaryNode::new("call").with_child(
            BinaryNode::new("link_create")
                .with_attr("token", "https://call.whatsapp.com/video/xyz123"),
        );
        let token = parse_call_link_response(&resp).expect("Failed to parse token");
        assert_eq!(token, "https://call.whatsapp.com/video/xyz123");
    }
}
