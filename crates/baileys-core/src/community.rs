use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::CoreError;
use baileys_binary::{BinaryNode, BinaryNodeContent};

/// Full metadata and configuration of a WhatsApp Community parent group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommunityMetadata {
    pub id: String,
    pub subject: String,
    pub subject_owner: Option<String>,
    pub subject_time: u64,
    pub size: u32,
    pub creation: u64,
    pub owner: Option<String>,
    pub description: Option<String>,
    pub description_id: Option<String>,
    pub linked_parent: Option<String>,
    pub is_community: bool,
    pub is_community_announce: bool,
    pub join_approval_mode: bool,
    pub member_add_mode: bool,
    pub participants: Vec<CommunityParticipant>,
}

/// Participant in a WhatsApp Community or Group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommunityParticipant {
    pub jid: String,
    pub admin: Option<String>, // "admin", "superadmin", or None
}

/// A subgroup linked under a parent WhatsApp Community.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommunitySubgroup {
    pub id: String,
    pub subject: String,
    pub creation: Option<u64>,
    pub creator: Option<String>,
    pub size: Option<u32>,
}

/// Builds the `<create>` stanza payload to instantiate a new WhatsApp Community.
pub fn build_create_community_node(subject: &str, description: Option<&str>) -> BinaryNode {
    let desc_id = format!("{:012x}", rand::random::<u64>() & 0xffffffffffff);
    let mut children = Vec::new();

    if let Some(desc) = description {
        children.push(BinaryNode {
            tag: "description".to_string(),
            attrs: [("id".to_string(), desc_id)].into_iter().collect(),
            content: Some(BinaryNodeContent::Nodes(vec![BinaryNode {
                tag: "body".to_string(),
                attrs: HashMap::new(),
                content: Some(BinaryNodeContent::Bytes(desc.as_bytes().to_vec())),
            }])),
        });
    }

    children.push(BinaryNode {
        tag: "parent".to_string(),
        attrs: [(
            "default_membership_approval_mode".to_string(),
            "request_required".to_string(),
        )]
        .into_iter()
        .collect(),
        content: None,
    });

    children.push(BinaryNode {
        tag: "allow_non_admin_sub_group_creation".to_string(),
        attrs: HashMap::new(),
        content: None,
    });

    children.push(BinaryNode {
        tag: "create_general_chat".to_string(),
        attrs: HashMap::new(),
        content: None,
    });

    BinaryNode {
        tag: "create".to_string(),
        attrs: [("subject".to_string(), subject.to_string())]
            .into_iter()
            .collect(),
        content: Some(BinaryNodeContent::Nodes(children)),
    }
}

/// Builds the `<create>` stanza payload to instantiate a new linked subgroup inside a Community.
pub fn build_create_community_subgroup_node(
    subject: &str,
    participants: &[String],
    parent_community_jid: &str,
) -> BinaryNode {
    let key = format!("{:016x}", rand::random::<u64>());
    let mut children = Vec::new();

    for jid in participants {
        children.push(BinaryNode {
            tag: "participant".to_string(),
            attrs: [("jid".to_string(), jid.clone())].into_iter().collect(),
            content: None,
        });
    }

    children.push(BinaryNode {
        tag: "linked_parent".to_string(),
        attrs: [("jid".to_string(), parent_community_jid.to_string())]
            .into_iter()
            .collect(),
        content: None,
    });

    BinaryNode {
        tag: "create".to_string(),
        attrs: [
            ("subject".to_string(), subject.to_string()),
            ("key".to_string(), key),
        ]
        .into_iter()
        .collect(),
        content: Some(BinaryNodeContent::Nodes(children)),
    }
}

/// Builds the `<links>` stanza to link an existing WhatsApp group into a parent Community.
pub fn build_link_subgroup_node(group_jid: &str) -> BinaryNode {
    BinaryNode {
        tag: "links".to_string(),
        attrs: HashMap::new(),
        content: Some(BinaryNodeContent::Nodes(vec![BinaryNode {
            tag: "link".to_string(),
            attrs: [("link_type".to_string(), "sub_group".to_string())]
                .into_iter()
                .collect(),
            content: Some(BinaryNodeContent::Nodes(vec![BinaryNode {
                tag: "group".to_string(),
                attrs: [("jid".to_string(), group_jid.to_string())]
                    .into_iter()
                    .collect(),
                content: None,
            }])),
        }])),
    }
}

/// Builds the `<unlink>` stanza to remove a linked subgroup from a parent Community.
pub fn build_unlink_subgroup_node(group_jid: &str) -> BinaryNode {
    BinaryNode {
        tag: "unlink".to_string(),
        attrs: [("unlink_type".to_string(), "sub_group".to_string())]
            .into_iter()
            .collect(),
        content: Some(BinaryNodeContent::Nodes(vec![BinaryNode {
            tag: "group".to_string(),
            attrs: [("jid".to_string(), group_jid.to_string())]
                .into_iter()
                .collect(),
            content: None,
        }])),
    }
}

/// Builds the `<sub_groups/>` query to list all subgroups under a Community.
pub fn build_fetch_subgroups_node() -> BinaryNode {
    BinaryNode {
        tag: "sub_groups".to_string(),
        attrs: HashMap::new(),
        content: None,
    }
}

/// Builds the `<query request="interactive"/>` node for community metadata.
pub fn build_community_metadata_query_node() -> BinaryNode {
    BinaryNode {
        tag: "query".to_string(),
        attrs: [("request".to_string(), "interactive".to_string())]
            .into_iter()
            .collect(),
        content: None,
    }
}

/// Builds the `<participating>` node to fetch all communities where the user is a member.
pub fn build_community_participating_node() -> BinaryNode {
    BinaryNode {
        tag: "participating".to_string(),
        attrs: HashMap::new(),
        content: Some(BinaryNodeContent::Nodes(vec![
            BinaryNode {
                tag: "participants".to_string(),
                attrs: HashMap::new(),
                content: None,
            },
            BinaryNode {
                tag: "description".to_string(),
                attrs: HashMap::new(),
                content: None,
            },
        ])),
    }
}

/// Builds the `<leave>` node to exit a Community.
pub fn build_community_leave_node(community_id: &str) -> BinaryNode {
    BinaryNode {
        tag: "leave".to_string(),
        attrs: HashMap::new(),
        content: Some(BinaryNodeContent::Nodes(vec![BinaryNode {
            tag: "community".to_string(),
            attrs: [("id".to_string(), community_id.to_string())]
                .into_iter()
                .collect(),
            content: None,
        }])),
    }
}

/// Builds the `<invite/>` query to retrieve a Community invite code.
pub fn build_community_invite_query_node() -> BinaryNode {
    BinaryNode {
        tag: "invite".to_string(),
        attrs: HashMap::new(),
        content: None,
    }
}

/// Builds the `<invite code="..."/>` node to accept an invite and join a Community.
pub fn build_community_accept_invite_node(code: &str) -> BinaryNode {
    BinaryNode {
        tag: "invite".to_string(),
        attrs: [("code".to_string(), code.to_string())]
            .into_iter()
            .collect(),
        content: None,
    }
}

/// Parses `<community ...>` node into `CommunityMetadata`.
pub fn parse_community_metadata(node: &BinaryNode) -> Result<CommunityMetadata, CoreError> {
    let community = if node.tag == "community" {
        node
    } else if let Some(child) = node.get_child("community") {
        child
    } else if let Some(child) = node.get_child("group") {
        child
    } else {
        return Err(CoreError::Protocol(
            "Missing <community> or <group> node in response".to_string(),
        ));
    };

    let id = community
        .get_attr("id")
        .map(|s| {
            if s.contains('@') {
                s.to_string()
            } else {
                format!("{}@g.us", s)
            }
        })
        .unwrap_or_default();

    let subject = community.get_attr("subject").unwrap_or("").to_string();
    let subject_owner = community.get_attr("s_o").map(|s| s.to_string());
    let subject_time = community
        .get_attr("s_t")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let creation = community
        .get_attr("creation")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let owner = community.get_attr("creator").map(|s| s.to_string());

    let mut description = None;
    let mut description_id = None;
    if let Some(desc_node) = community.get_child("description") {
        description_id = desc_node.get_attr("id").map(|s| s.to_string());
        if let Some(body_node) = desc_node.get_child("body") {
            if let Some(bytes) = body_node.content_as_bytes() {
                description = Some(String::from_utf8_lossy(bytes).to_string());
            }
        }
    }

    let linked_parent = community
        .get_child("linked_parent")
        .and_then(|n| n.get_attr("jid"))
        .map(|s| s.to_string());

    let is_community = community.get_child("parent").is_some();
    let is_community_announce = community.get_child("default_sub_community").is_some();
    let join_approval_mode = community.get_child("membership_approval_mode").is_some();
    let member_add_mode = community
        .get_child("member_add_mode")
        .and_then(|n| n.content_as_string())
        == Some("all_member_add");

    let mut participants = Vec::new();
    for p_node in community.get_children() {
        if p_node.tag == "participant" {
            if let Some(jid) = p_node.get_attr("jid") {
                participants.push(CommunityParticipant {
                    jid: jid.to_string(),
                    admin: p_node.get_attr("type").map(|s| s.to_string()),
                });
            }
        }
    }

    let size = if participants.is_empty() {
        community
            .get_attr("size")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    } else {
        participants.len() as u32
    };

    Ok(CommunityMetadata {
        id,
        subject,
        subject_owner,
        subject_time,
        size,
        creation,
        owner,
        description,
        description_id,
        linked_parent,
        is_community,
        is_community_announce,
        join_approval_mode,
        member_add_mode,
        participants,
    })
}

/// Parses `<sub_groups>` result node into a vector of `CommunitySubgroup`.
pub fn parse_linked_subgroups(node: &BinaryNode) -> Result<Vec<CommunitySubgroup>, CoreError> {
    let sub_groups_node = if node.tag == "sub_groups" {
        node
    } else if let Some(child) = node.get_child("sub_groups") {
        child
    } else {
        return Ok(Vec::new());
    };

    let mut list = Vec::new();
    for g_node in sub_groups_node.get_children() {
        if g_node.tag == "group" {
            let id = g_node
                .get_attr("id")
                .map(|s| {
                    if s.contains('@') {
                        s.to_string()
                    } else {
                        format!("{}@g.us", s)
                    }
                })
                .unwrap_or_default();
            let subject = g_node.get_attr("subject").unwrap_or("").to_string();
            let creation = g_node
                .get_attr("creation")
                .and_then(|s| s.parse::<u64>().ok());
            let creator = g_node.get_attr("creator").map(|s| s.to_string());
            let size = g_node.get_attr("size").and_then(|s| s.parse::<u32>().ok());

            list.push(CommunitySubgroup {
                id,
                subject,
                creation,
                creator,
                size,
            });
        }
    }

    Ok(list)
}

/// Builds an IQ stanza to revoke the invite code of a Community.
pub fn build_community_revoke_invite_iq(community_jid: &str) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    BinaryNode::new("iq")
        .with_attr("to", community_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", msg_id)
        .with_child(BinaryNode::new("invite"))
}

/// Builds an IQ stanza to inspect invite info before joining a Community.
pub fn build_community_get_invite_info_iq(code: &str) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    BinaryNode::new("iq")
        .with_attr("to", "@g.us")
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "get")
        .with_attr("id", msg_id)
        .with_child(BinaryNode::new("invite").with_attr("code", code))
}

/// Builds an IQ stanza to configure ephemeral disappearing message timers on a Community.
pub fn build_community_toggle_ephemeral_iq(
    community_jid: &str,
    ephemeral_expiration: u32,
) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let child = if ephemeral_expiration > 0 {
        BinaryNode::new("ephemeral").with_attr("expiration", ephemeral_expiration.to_string())
    } else {
        BinaryNode::new("not_ephemeral")
    };

    BinaryNode::new("iq")
        .with_attr("to", community_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", msg_id)
        .with_child(child)
}

/// Builds an IQ stanza to update community permissions (announcement, locked, etc.).
pub fn build_community_setting_update_iq(community_jid: &str, setting: &str) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    BinaryNode::new("iq")
        .with_attr("to", community_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", msg_id)
        .with_child(BinaryNode::new(setting))
}

/// Builds an IQ stanza to update community member add mode ("admin_add" or "all_member_add").
pub fn build_community_member_add_mode_iq(community_jid: &str, mode: &str) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    BinaryNode::new("iq")
        .with_attr("to", community_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", msg_id)
        .with_child(BinaryNode::new("member_add_mode").with_bytes_content(mode.as_bytes().to_vec()))
}

/// Builds an IQ stanza to update community join approval mode ("on" or "off").
pub fn build_community_join_approval_mode_iq(community_jid: &str, mode: &str) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let join_node = BinaryNode::new("community_join").with_attr("state", mode);
    BinaryNode::new("iq")
        .with_attr("to", community_jid)
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "set")
        .with_attr("id", msg_id)
        .with_child(BinaryNode::new("membership_approval_mode").with_child(join_node))
}

/// Builds an IQ stanza to fetch all participating communities.
pub fn build_community_fetch_all_participating_iq() -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let participating = BinaryNode::new("participating")
        .with_child(BinaryNode::new("participants"))
        .with_child(BinaryNode::new("description"));

    BinaryNode::new("iq")
        .with_attr("to", "@g.us")
        .with_attr("xmlns", "w:g2")
        .with_attr("type", "get")
        .with_attr("id", msg_id)
        .with_child(participating)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommunityInviteInfo {
    pub jid: String,
    pub subject: String,
    pub creator: Option<String>,
    pub size: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_create_community_node() {
        let node = build_create_community_node("My Community", Some("Welcome everyone!"));
        assert_eq!(node.tag, "create");
        assert_eq!(node.get_attr("subject"), Some("My Community"));

        let children = node.get_children();
        assert!(children.iter().any(|c| c.tag == "parent"));
        assert!(children.iter().any(|c| c.tag == "create_general_chat"));
        assert!(children.iter().any(|c| c.tag == "description"));
    }

    #[test]
    fn test_build_link_and_unlink_subgroup_node() {
        let link = build_link_subgroup_node("12345@g.us");
        assert_eq!(link.tag, "links");
        let l = link.get_child("link").unwrap();
        assert_eq!(l.get_attr("link_type"), Some("sub_group"));
        assert_eq!(
            l.get_child("group").unwrap().get_attr("jid"),
            Some("12345@g.us")
        );

        let unlink = build_unlink_subgroup_node("12345@g.us");
        assert_eq!(unlink.tag, "unlink");
        assert_eq!(unlink.get_attr("unlink_type"), Some("sub_group"));
        assert_eq!(
            unlink.get_child("group").unwrap().get_attr("jid"),
            Some("12345@g.us")
        );
    }

    #[test]
    fn test_parse_community_metadata() {
        let mut comm = BinaryNode::new("community")
            .with_attr("id", "120363000000000001")
            .with_attr("subject", "Alpha Community")
            .with_attr("creation", "1700000000")
            .with_attr("creator", "admin@s.whatsapp.net");

        comm = comm.with_child(BinaryNode::new("parent"));
        comm = comm.with_child(BinaryNode::new("default_sub_community"));
        comm = comm.with_child(
            BinaryNode::new("participant")
                .with_attr("jid", "admin@s.whatsapp.net")
                .with_attr("type", "superadmin"),
        );

        let parsed = parse_community_metadata(&comm).unwrap();
        assert_eq!(parsed.id, "120363000000000001@g.us");
        assert_eq!(parsed.subject, "Alpha Community");
        assert!(parsed.is_community);
        assert!(parsed.is_community_announce);
        assert_eq!(parsed.participants.len(), 1);
        assert_eq!(parsed.participants[0].admin.as_deref(), Some("superadmin"));
    }

    #[test]
    fn test_parse_linked_subgroups() {
        let sub1 = BinaryNode::new("group")
            .with_attr("id", "sub1")
            .with_attr("subject", "Announcements")
            .with_attr("creation", "1700000010")
            .with_attr("creator", "admin@s.whatsapp.net")
            .with_attr("size", "25");

        let sub2 = BinaryNode::new("group")
            .with_attr("id", "sub2")
            .with_attr("subject", "Dev Chat")
            .with_attr("creation", "1700000020")
            .with_attr("creator", "dev@s.whatsapp.net")
            .with_attr("size", "10");

        let node = BinaryNode::new("sub_groups").with_children(vec![sub1, sub2]);
        let parsed = parse_linked_subgroups(&node).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "sub1@g.us");
        assert_eq!(parsed[0].subject, "Announcements");
        assert_eq!(parsed[0].size, Some(25));
        assert_eq!(parsed[1].id, "sub2@g.us");
        assert_eq!(parsed[1].subject, "Dev Chat");
        assert_eq!(parsed[1].size, Some(10));
    }
}
