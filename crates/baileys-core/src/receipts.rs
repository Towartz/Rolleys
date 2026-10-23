//! Read receipts, mirroring Baileys' `readMessages` / `sendReceipt` / `aggregateMessageKeysNotFromMe`.
//!
//! Pure functions: they group message keys and build `<receipt>` stanzas without any I/O.

use baileys_binary::BinaryNode;
use std::collections::BTreeMap;

/// Identifies a message to mark as read (a Baileys `WAMessageKey`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReadKey {
    pub remote_jid: String,
    pub id: String,
    /// Author of the message inside a group.
    pub participant: Option<String>,
    pub from_me: bool,
}

/// Messages of one chat/author that share a single receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptGroup {
    pub jid: String,
    pub participant: Option<String>,
    pub message_ids: Vec<String>,
}

/// Groups keys by chat and author, dropping our own messages (they cannot be marked as read).
/// Groups keep the order in which they first appear.
pub fn aggregate_keys_not_from_me(keys: &[ReadKey]) -> Vec<ReceiptGroup> {
    let mut order: Vec<(String, Option<String>)> = Vec::new();
    let mut groups: BTreeMap<(String, Option<String>), Vec<String>> = BTreeMap::new();
    for key in keys.iter().filter(|k| !k.from_me) {
        let group = (key.remote_jid.clone(), key.participant.clone());
        if !groups.contains_key(&group) {
            order.push(group.clone());
        }
        groups.entry(group).or_default().push(key.id.clone());
    }
    order
        .into_iter()
        .map(|g| {
            let message_ids = groups.remove(&g).unwrap_or_default();
            ReceiptGroup {
                jid: g.0,
                participant: g.1,
                message_ids,
            }
        })
        .collect()
}

/// The receipt type for reading: `read` when our privacy lets others see read receipts,
/// `read-self` (synced to our other devices only) when it does not.
pub fn read_receipt_type(readreceipts_privacy: Option<&str>) -> &'static str {
    if readreceipts_privacy == Some("all") {
        "read"
    } else {
        "read-self"
    }
}

/// Builds the `<receipt>` for a group: the first id is the stanza id, the rest go in a `<list>`.
pub fn build_read_receipt(
    group: &ReceiptGroup,
    receipt_type: &str,
    now_secs: u64,
) -> Option<BinaryNode> {
    let (first, rest) = group.message_ids.split_first()?;
    let mut node = BinaryNode::new("receipt")
        .with_attr("id", first.as_str())
        .with_attr("t", now_secs.to_string())
        .with_attr("to", group.jid.as_str())
        .with_attr("type", receipt_type);
    if let Some(participant) = &group.participant {
        node = node.with_attr("participant", participant.as_str());
    }
    if !rest.is_empty() {
        node = node.with_child(
            BinaryNode::new("list").with_children(
                rest.iter()
                    .map(|id| BinaryNode::new("item").with_attr("id", id.as_str()))
                    .collect(),
            ),
        );
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(jid: &str, id: &str, participant: Option<&str>, from_me: bool) -> ReadKey {
        ReadKey {
            remote_jid: jid.into(),
            id: id.into(),
            participant: participant.map(String::from),
            from_me,
        }
    }

    #[test]
    fn keys_are_grouped_per_chat_and_author_and_skip_own_messages() {
        let keys = vec![
            key("a@s.whatsapp.net", "1", None, false),
            key("g@g.us", "2", Some("p1@lid"), false),
            key("a@s.whatsapp.net", "3", None, false),
            key("g@g.us", "4", Some("p2@lid"), false),
            key("g@g.us", "5", Some("p1@lid"), false),
            key("a@s.whatsapp.net", "mine", None, true),
        ];
        let groups = aggregate_keys_not_from_me(&keys);
        assert_eq!(
            groups,
            vec![
                ReceiptGroup {
                    jid: "a@s.whatsapp.net".into(),
                    participant: None,
                    message_ids: vec!["1".into(), "3".into()]
                },
                ReceiptGroup {
                    jid: "g@g.us".into(),
                    participant: Some("p1@lid".into()),
                    message_ids: vec!["2".into(), "5".into()]
                },
                ReceiptGroup {
                    jid: "g@g.us".into(),
                    participant: Some("p2@lid".into()),
                    message_ids: vec!["4".into()]
                },
            ]
        );
        assert!(aggregate_keys_not_from_me(&[key("a@s.whatsapp.net", "x", None, true)]).is_empty());
    }

    #[test]
    fn receipt_type_follows_the_privacy_setting() {
        assert_eq!(read_receipt_type(Some("all")), "read");
        assert_eq!(read_receipt_type(Some("none")), "read-self");
        assert_eq!(read_receipt_type(None), "read-self");
    }

    #[test]
    fn receipt_node_has_timestamp_list_and_participant() {
        let group = ReceiptGroup {
            jid: "g@g.us".into(),
            participant: Some("p@lid".into()),
            message_ids: vec!["1".into(), "2".into(), "3".into()],
        };
        let node = build_read_receipt(&group, "read", 1_700_000_000).unwrap();
        assert_eq!(node.get_attr("id"), Some("1"));
        assert_eq!(node.get_attr("t"), Some("1700000000"));
        assert_eq!(node.get_attr("to"), Some("g@g.us"));
        assert_eq!(node.get_attr("type"), Some("read"));
        assert_eq!(node.get_attr("participant"), Some("p@lid"));
        let ids: Vec<_> = node
            .get_child("list")
            .unwrap()
            .get_children()
            .iter()
            .map(|i| i.get_attr("id").unwrap().to_string())
            .collect();
        assert_eq!(ids, vec!["2", "3"]);

        let single = build_read_receipt(
            &ReceiptGroup {
                jid: "a@s.whatsapp.net".into(),
                participant: None,
                message_ids: vec!["9".into()],
            },
            "read-self",
            5,
        )
        .unwrap();
        assert!(single.get_child("list").is_none() && single.get_attr("participant").is_none());
        assert!(build_read_receipt(
            &ReceiptGroup {
                jid: "x".into(),
                participant: None,
                message_ids: vec![]
            },
            "read",
            0
        )
        .is_none());
    }
}
