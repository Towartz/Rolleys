//! History sync processing, mirroring Baileys' `processHistoryMessage` (Utils/history.ts).
//!
//! Pure functions: they turn a decoded [`HistorySync`] into the contacts, flat message list and
//! LID/PN mappings carried by `messaging-history.set`, without touching the store.

use baileys_proto::proto::{history_sync::HistorySyncType, HistorySync, WebMessageInfo};
use serde::{Deserialize, Serialize};

/// A contact learned from a history sync chunk.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HistoryContact {
    pub jid: String,
    pub name: Option<String>,
    pub username: Option<String>,
    pub lid: Option<String>,
    pub phone_number: Option<String>,
    /// Push name (`PUSH_NAME` chunks).
    pub notify: Option<String>,
}

/// A LID ↔ phone-number pair.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct LidPnMapping {
    pub lid: String,
    pub pn: String,
}

/// What [`process_history_sync`] extracts from one chunk.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessedHistory {
    pub contacts: Vec<HistoryContact>,
    pub messages: Vec<WebMessageInfo>,
    pub lid_pn_mappings: Vec<LidPnMapping>,
}

fn is_lid(jid: &str) -> bool {
    jid.ends_with("@lid") || jid.ends_with("@hosted.lid")
}

fn is_pn(jid: &str) -> bool {
    jid.ends_with("@s.whatsapp.net") || jid.ends_with("@hosted")
}

/// Fallback when a LID chat has no `pnJid`: the recipient of one of our outgoing messages.
fn pn_from_receipts(messages: &[WebMessageInfo]) -> Option<String> {
    messages.iter().find_map(|m| {
        if !m.key.as_ref().and_then(|k| k.from_me).unwrap_or(false) {
            return None;
        }
        let jid = m.user_receipt.first()?.user_jid.as_deref()?;
        is_pn(jid).then(|| jid.to_string())
    })
}

/// Extracts contacts, messages and LID/PN mappings from `history`, and trims every
/// conversation in it to its most recent message (Baileys keeps only that one on the chat;
/// the full list goes to `messages`).
pub fn process_history_sync(history: &mut HistorySync) -> ProcessedHistory {
    let mut out = ProcessedHistory::default();

    for m in &history.phone_number_to_lid_mappings {
        if let (Some(lid), Some(pn)) = (&m.lid_jid, &m.pn_jid) {
            out.lid_pn_mappings.push(LidPnMapping {
                lid: lid.clone(),
                pn: pn.clone(),
            });
        }
    }

    let sync_type = history
        .sync_type
        .and_then(|t| HistorySyncType::try_from(t).ok());
    match sync_type {
        Some(
            HistorySyncType::InitialBootstrap
            | HistorySyncType::Recent
            | HistorySyncType::Full
            | HistorySyncType::OnDemand,
        ) => {
            for chat in &mut history.conversations {
                let Some(id) = chat.id.clone() else {
                    continue;
                };
                out.contacts.push(HistoryContact {
                    jid: id.clone(),
                    name: chat
                        .display_name
                        .clone()
                        .or_else(|| chat.name.clone())
                        .or_else(|| chat.username.clone()),
                    username: chat.username.clone(),
                    lid: chat.lid_jid.clone().or_else(|| chat.account_lid.clone()),
                    phone_number: chat.pn_jid.clone(),
                    notify: None,
                });

                let msgs: Vec<WebMessageInfo> =
                    chat.messages.drain(..).filter_map(|h| h.message).collect();

                if is_lid(&id) {
                    if let Some(pn) = chat.pn_jid.clone().or_else(|| pn_from_receipts(&msgs)) {
                        out.lid_pn_mappings.push(LidPnMapping {
                            lid: id.clone(),
                            pn,
                        });
                    }
                } else if is_pn(&id) {
                    if let Some(lid) = chat.lid_jid.clone() {
                        out.lid_pn_mappings.push(LidPnMapping {
                            lid,
                            pn: id.clone(),
                        });
                    }
                }

                // Keep only the most recent message on the chat itself.
                if let Some(first) = msgs.first() {
                    chat.messages.push(baileys_proto::proto::HistorySyncMsg {
                        message: Some(first.clone()),
                        msg_order_id: None,
                    });
                }
                out.messages.extend(msgs);
            }
        }
        Some(HistorySyncType::PushName) => {
            for p in &history.pushnames {
                if let (Some(id), Some(name)) = (&p.id, &p.pushname) {
                    out.contacts.push(HistoryContact {
                        jid: id.clone(),
                        notify: Some(name.clone()),
                        ..Default::default()
                    });
                }
            }
        }
        _ => {}
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_proto::proto::{
        Conversation, HistorySyncMsg, MessageKey, PhoneNumberToLidMapping, Pushname, UserReceipt,
    };

    fn msg(id: &str, from_me: bool, receipt_to: Option<&str>) -> HistorySyncMsg {
        HistorySyncMsg {
            message: Some(WebMessageInfo {
                key: Some(MessageKey {
                    remote_jid: Some("x".into()),
                    from_me: Some(from_me),
                    id: Some(id.into()),
                    participant: None,
                }),
                user_receipt: receipt_to
                    .map(|j| {
                        vec![UserReceipt {
                            user_jid: Some(j.into()),
                            ..Default::default()
                        }]
                    })
                    .unwrap_or_default(),
                ..Default::default()
            }),
            msg_order_id: None,
        }
    }

    #[test]
    fn recent_chunk_extracts_contacts_messages_and_mappings() {
        let mut h = HistorySync {
            sync_type: Some(HistorySyncType::Recent as i32),
            phone_number_to_lid_mappings: vec![PhoneNumberToLidMapping {
                pn_jid: Some("1@s.whatsapp.net".into()),
                lid_jid: Some("11@lid".into()),
            }],
            conversations: vec![
                // PN chat that knows its LID.
                Conversation {
                    id: Some("2@s.whatsapp.net".into()),
                    display_name: Some("Two".into()),
                    lid_jid: Some("22@lid".into()),
                    messages: vec![msg("A", false, None), msg("B", false, None)],
                    ..Default::default()
                },
                // LID chat without pnJid: PN comes from a receipt on our outgoing message.
                Conversation {
                    id: Some("33@lid".into()),
                    username: Some("three".into()),
                    messages: vec![
                        msg("C", false, None),
                        msg("D", true, Some("3@s.whatsapp.net")),
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let out = process_history_sync(&mut h);

        assert_eq!(out.messages.len(), 4, "every message is in the flat list");
        assert!(
            h.conversations.iter().all(|c| c.messages.len() == 1),
            "chats keep only their most recent message"
        );
        assert_eq!(out.contacts.len(), 2);
        assert_eq!(out.contacts[0].name.as_deref(), Some("Two"));
        assert_eq!(out.contacts[0].lid.as_deref(), Some("22@lid"));
        assert_eq!(out.contacts[1].name.as_deref(), Some("three"));
        assert_eq!(
            out.lid_pn_mappings,
            vec![
                LidPnMapping {
                    lid: "11@lid".into(),
                    pn: "1@s.whatsapp.net".into()
                },
                LidPnMapping {
                    lid: "22@lid".into(),
                    pn: "2@s.whatsapp.net".into()
                },
                LidPnMapping {
                    lid: "33@lid".into(),
                    pn: "3@s.whatsapp.net".into()
                },
            ]
        );
    }

    #[test]
    fn push_name_chunk_yields_notify_contacts_only() {
        let mut h = HistorySync {
            sync_type: Some(HistorySyncType::PushName as i32),
            pushnames: vec![Pushname {
                id: Some("4@s.whatsapp.net".into()),
                pushname: Some("Four".into()),
            }],
            ..Default::default()
        };
        let out = process_history_sync(&mut h);
        assert_eq!(out.contacts.len(), 1);
        assert_eq!(out.contacts[0].notify.as_deref(), Some("Four"));
        assert!(out.messages.is_empty());
    }
}
