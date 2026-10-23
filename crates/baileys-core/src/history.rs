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

/// No new RECENT chunk for this long means the phone paused the sync (Baileys
/// `HISTORY_SYNC_PAUSED_TIMEOUT_MS`).
pub const HISTORY_SYNC_PAUSED_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Per-connection state behind `messaging-history.status` (Baileys `historySyncStatus`).
///
/// Pure: the caller arms a timer for the generation returned by [`Self::on_chunk`] and calls
/// [`Self::on_pause_timeout`] when it fires.
#[derive(Debug, Default)]
pub struct HistorySyncStatusTracker {
    initial_bootstrap_complete: bool,
    recent_sync_complete: bool,
    recent_generation: u64,
}

/// A `messaging-history.status` emission: `(sync_type, status, explicit)`.
pub type HistoryStatus = (i32, &'static str, bool);

impl HistorySyncStatusTracker {
    /// Handles one history notification. Returns the statuses to emit and, for an unfinished
    /// RECENT sync, the generation a pause timer should be armed for (older timers are stale).
    pub fn on_chunk(
        &mut self,
        sync_type: i32,
        progress: Option<u32>,
    ) -> (Vec<HistoryStatus>, Option<u64>) {
        let mut out = Vec::new();
        let bootstrap = HistorySyncType::InitialBootstrap as i32;
        let recent = HistorySyncType::Recent as i32;
        if sync_type == bootstrap && !self.initial_bootstrap_complete {
            self.initial_bootstrap_complete = true;
            out.push((sync_type, "complete", true));
        }
        if sync_type == recent && progress == Some(100) && !self.recent_sync_complete {
            self.recent_sync_complete = true;
            self.recent_generation += 1; // cancels any armed timer
            out.push((sync_type, "complete", true));
        }
        let arm = (sync_type == recent && !self.recent_sync_complete).then(|| {
            self.recent_generation += 1;
            self.recent_generation
        });
        (out, arm)
    }

    /// Called when a pause timer armed for `generation` fires.
    pub fn on_pause_timeout(&mut self, generation: u64) -> Option<HistoryStatus> {
        if generation != self.recent_generation || self.recent_sync_complete {
            return None;
        }
        self.recent_sync_complete = true;
        Some((HistorySyncType::Recent as i32, "paused", false))
    }
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

    #[test]
    fn status_tracker_follows_baileys() {
        let boot = HistorySyncType::InitialBootstrap as i32;
        let recent = HistorySyncType::Recent as i32;
        let mut t = HistorySyncStatusTracker::default();

        assert_eq!(t.on_chunk(boot, None).0, vec![(boot, "complete", true)]);
        assert!(
            t.on_chunk(boot, None).0.is_empty(),
            "bootstrap completes once"
        );

        // Unfinished RECENT chunks arm a timer; only the newest one may fire.
        let (_, g1) = t.on_chunk(recent, Some(40));
        let (_, g2) = t.on_chunk(recent, Some(60));
        assert!(t.on_pause_timeout(g1.unwrap()).is_none(), "stale timer");
        assert_eq!(
            t.on_pause_timeout(g2.unwrap()),
            Some((recent, "paused", false))
        );
        assert!(
            t.on_chunk(recent, Some(100)).0.is_empty(),
            "already finished"
        );

        // RECENT reaching 100 completes explicitly and cancels a pending timer.
        let mut t = HistorySyncStatusTracker::default();
        let (_, g) = t.on_chunk(recent, Some(10));
        let (ev, arm) = t.on_chunk(recent, Some(100));
        assert_eq!(ev, vec![(recent, "complete", true)]);
        assert!(arm.is_none());
        assert!(t.on_pause_timeout(g.unwrap()).is_none());
    }
}
