//! Pure parsers that turn server `<notification>` stanzas into [`BaileysEvent`]s.
//!
//! These mirror Baileys' `handleGroupNotification` / `handleNewsletterNotification`
//! (messages-recv.ts) but have no I/O, so they can be unit-tested with hand-built nodes.

use crate::events::{BaileysEvent, GroupUpdate};
use crate::groups::{parse_group_metadata, GroupParticipant};
use baileys_binary::BinaryNode;

fn parse_participants(child: &BinaryNode) -> Vec<GroupParticipant> {
    child
        .get_children()
        .iter()
        .filter(|c| c.tag == "participant")
        .filter_map(|p| {
            let jid = p.get_attr("jid")?.to_string();
            Some(GroupParticipant {
                jid,
                phone_number: p.get_attr("phone_number").map(String::from),
                lid: p.get_attr("lid").map(String::from),
                admin: p.get_attr("type").map(String::from),
            })
        })
        .collect()
}

/// Parses a `<notification type="w:gp2">` stanza into zero or more events.
pub fn parse_group_notification(node: &BinaryNode) -> Vec<BaileysEvent> {
    let group_jid = node.get_attr("from").unwrap_or("").to_string();
    if group_jid.is_empty() {
        return Vec::new();
    }
    let author = node.get_attr("participant").map(String::from);
    let mut events = Vec::new();

    for child in node.get_children() {
        let mut update = GroupUpdate {
            id: group_jid.clone(),
            author: author.clone(),
            ..Default::default()
        };
        let mut is_update = true;

        match child.tag.as_str() {
            "create" => {
                is_update = false;
                if let Ok(meta) = parse_group_metadata(child) {
                    events.push(BaileysEvent::GroupsUpsert { groups: vec![meta] });
                }
            }
            "ephemeral" | "not_ephemeral" => {
                update.ephemeral_duration = Some(if child.tag == "not_ephemeral" {
                    0
                } else {
                    child
                        .get_attr("expiration")
                        .and_then(|e| e.parse().ok())
                        .unwrap_or(0)
                });
            }
            "promote" | "demote" | "remove" | "add" | "leave" | "modify" => {
                is_update = false;
                let participants = parse_participants(child);
                // A `remove` that targets the sender is really a voluntary leave.
                let action = if child.tag == "remove"
                    && participants.len() == 1
                    && author.as_deref() == Some(participants[0].jid.as_str())
                {
                    "leave"
                } else {
                    child.tag.as_str()
                };
                events.push(BaileysEvent::GroupParticipantsUpdate {
                    group_jid: group_jid.clone(),
                    author: author.clone(),
                    participants,
                    action: action.to_string(),
                });
            }
            "subject" => {
                update.subject = child.get_attr("subject").map(String::from);
                update.subject_owner = child.get_attr("s_o").map(String::from);
                update.subject_time = child.get_attr("s_t").and_then(|t| t.parse().ok());
            }
            "description" => {
                update.description_id = child.get_attr("id").map(String::from);
                if child.get_child("delete").is_some() {
                    update.description_deleted = true;
                } else {
                    update.description = child
                        .get_child("body")
                        .and_then(|b| b.content_as_bytes())
                        .map(|b| String::from_utf8_lossy(b).to_string());
                }
            }
            "announcement" | "not_announcement" => {
                update.announce = Some(child.tag == "announcement");
            }
            "locked" | "unlocked" => {
                update.restrict = Some(child.tag == "locked");
            }
            "invite" => {
                update.invite_code = child.get_attr("code").map(String::from);
            }
            "member_add_mode" => {
                update.member_add_mode = child.content_as_str().map(String::from);
            }
            "membership_approval_mode" => {
                update.join_approval_mode = child
                    .get_child("group_join")
                    .and_then(|g| g.get_attr("state"))
                    .map(|s| s == "on");
            }
            "created_membership_requests" | "revoked_membership_requests" => {
                is_update = false;
                let created = child.tag == "created_membership_requests";
                for p in child
                    .get_children()
                    .iter()
                    .filter(|c| c.tag == "requested_user")
                {
                    if let Some(jid) = p.get_attr("jid") {
                        events.push(BaileysEvent::GroupJoinRequest {
                            group_jid: group_jid.clone(),
                            author: author.clone(),
                            participant: jid.to_string(),
                            participant_pn: p.get_attr("phone_number").map(String::from),
                            action: if created { "created" } else { "revoked" }.to_string(),
                            method: child.get_attr("request_method").map(String::from),
                        });
                    }
                }
            }
            _ => is_update = false,
        }

        if is_update {
            events.push(BaileysEvent::GroupsUpdate { update });
        }
    }
    events
}

/// Parses a `<notification type="newsletter">` stanza into zero or more events.
pub fn parse_newsletter_notification(node: &BinaryNode) -> Vec<BaileysEvent> {
    let jid = node.get_attr("from").unwrap_or("").to_string();
    if jid.is_empty() {
        return Vec::new();
    }
    let author = node.get_attr("participant").map(String::from);
    let mut events = Vec::new();

    for child in node.get_children() {
        match child.tag.as_str() {
            "reaction" => {
                let Some(server_id) = child.get_attr("message_id") else {
                    continue;
                };
                let code = child
                    .get_child("reaction")
                    .and_then(|r| r.content_as_str())
                    .map(String::from);
                events.push(BaileysEvent::NewsletterReaction {
                    newsletter_jid: jid.clone(),
                    server_id: server_id.to_string(),
                    removed: code.is_none(),
                    code,
                    count: child.get_attr("count").and_then(|c| c.parse().ok()),
                });
            }
            "view" => {
                let Some(server_id) = child.get_attr("message_id") else {
                    continue;
                };
                let count = child
                    .content_as_str()
                    .and_then(|c| c.trim().parse().ok())
                    .unwrap_or(0);
                events.push(BaileysEvent::NewsletterView {
                    newsletter_jid: jid.clone(),
                    server_id: server_id.to_string(),
                    count,
                });
            }
            "participant" => {
                if let (Some(user), Some(action)) =
                    (child.get_attr("jid"), child.get_attr("action"))
                {
                    events.push(BaileysEvent::NewsletterParticipantsUpdate {
                        newsletter_jid: jid.clone(),
                        author: author.clone(),
                        user: user.to_string(),
                        action: action.to_string(),
                        new_role: child.get_attr("role").map(String::from),
                    });
                }
            }
            "update" => {
                if let Some(settings) = child.get_child("settings") {
                    let text = |tag: &str| {
                        settings
                            .get_child(tag)
                            .and_then(|n| n.content_as_str())
                            .map(String::from)
                    };
                    events.push(BaileysEvent::NewsletterSettingsUpdate {
                        newsletter_jid: jid.clone(),
                        name: text("name"),
                        description: text("description"),
                    });
                }
            }
            _ => {}
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notif(from: &str, child: BinaryNode) -> BinaryNode {
        BinaryNode::new("notification")
            .with_attr("type", "w:gp2")
            .with_attr("from", from)
            .with_attr("participant", "111@s.whatsapp.net")
            .with_child(child)
    }

    #[test]
    fn add_emits_participants_update() {
        let n = notif(
            "123@g.us",
            BinaryNode::new("add")
                .with_child(BinaryNode::new("participant").with_attr("jid", "222@s.whatsapp.net")),
        );
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupParticipantsUpdate {
                group_jid,
                action,
                participants,
                author,
            }] => {
                assert_eq!(group_jid, "123@g.us");
                assert_eq!(action, "add");
                assert_eq!(participants[0].jid, "222@s.whatsapp.net");
                assert_eq!(author.as_deref(), Some("111@s.whatsapp.net"));
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn self_remove_becomes_leave() {
        let n = notif(
            "123@g.us",
            BinaryNode::new("remove")
                .with_child(BinaryNode::new("participant").with_attr("jid", "111@s.whatsapp.net")),
        );
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupParticipantsUpdate { action, .. }] => assert_eq!(action, "leave"),
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn subject_and_flags_emit_group_update() {
        let n = notif(
            "123@g.us",
            BinaryNode::new("subject")
                .with_attr("subject", "New name")
                .with_attr("s_o", "111@s.whatsapp.net")
                .with_attr("s_t", "1700000000"),
        );
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupsUpdate { update }] => {
                assert_eq!(update.subject.as_deref(), Some("New name"));
                assert_eq!(update.subject_time, Some(1_700_000_000));
            }
            other => panic!("unexpected events: {other:?}"),
        }

        let n = notif("123@g.us", BinaryNode::new("not_announcement"));
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupsUpdate { update }] => assert_eq!(update.announce, Some(false)),
            other => panic!("unexpected events: {other:?}"),
        }
        let n = notif("123@g.us", BinaryNode::new("locked"));
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupsUpdate { update }] => assert_eq!(update.restrict, Some(true)),
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn description_delete_and_body() {
        let n = notif(
            "123@g.us",
            BinaryNode::new("description")
                .with_attr("id", "D1")
                .with_child(BinaryNode::new("delete")),
        );
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupsUpdate { update }] => {
                assert!(update.description_deleted);
                assert_eq!(update.description_id.as_deref(), Some("D1"));
            }
            other => panic!("unexpected events: {other:?}"),
        }

        let n = notif(
            "123@g.us",
            BinaryNode::new("description")
                .with_attr("id", "D2")
                .with_child(BinaryNode::new("body").with_bytes_content(b"hello".to_vec())),
        );
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupsUpdate { update }] => {
                assert_eq!(update.description.as_deref(), Some("hello"))
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn approval_mode_and_ephemeral() {
        let n = notif(
            "123@g.us",
            BinaryNode::new("membership_approval_mode")
                .with_child(BinaryNode::new("group_join").with_attr("state", "on")),
        );
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupsUpdate { update }] => {
                assert_eq!(update.join_approval_mode, Some(true))
            }
            other => panic!("unexpected events: {other:?}"),
        }
        let n = notif("123@g.us", BinaryNode::new("not_ephemeral"));
        match &parse_group_notification(&n)[..] {
            [BaileysEvent::GroupsUpdate { update }] => {
                assert_eq!(update.ephemeral_duration, Some(0))
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn unknown_child_and_missing_from_emit_nothing() {
        let n = notif("123@g.us", BinaryNode::new("something_new"));
        assert!(parse_group_notification(&n).is_empty());
        let n = BinaryNode::new("notification").with_child(BinaryNode::new("add"));
        assert!(parse_group_notification(&n).is_empty());
    }

    #[test]
    fn newsletter_view_and_participant() {
        let n = BinaryNode::new("notification")
            .with_attr("type", "newsletter")
            .with_attr("from", "999@newsletter")
            .with_child(
                BinaryNode::new("view")
                    .with_attr("message_id", "42")
                    .with_string_content("17"),
            )
            .with_child(
                BinaryNode::new("participant")
                    .with_attr("jid", "5@s.whatsapp.net")
                    .with_attr("action", "promote")
                    .with_attr("role", "admin"),
            );
        match &parse_newsletter_notification(&n)[..] {
            [BaileysEvent::NewsletterView {
                server_id, count, ..
            }, BaileysEvent::NewsletterParticipantsUpdate {
                action, new_role, ..
            }] => {
                assert_eq!(server_id, "42");
                assert_eq!(*count, 17);
                assert_eq!(action, "promote");
                assert_eq!(new_role.as_deref(), Some("admin"));
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }
}
