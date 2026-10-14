//! WhatsApp E2EE Message Retry Pipeline.
//!
//! Handles incoming `<receipt type="retry">` stanzas from recipients who could not decrypt
//! sent messages (e.g. due to missing SenderKeys in groups, ratchet desynchronization,
//! or device re-installations). Injects fresh pre-key bundles and re-transmits the message
//! targeted to the requesting participant with the required keys.

use prost::Message as ProstMessage;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use baileys_binary::jid::S_WHATSAPP_NET;
use baileys_binary::BinaryNode;
use baileys_crypto::{pad_random_max16, KeyPair};
use baileys_proto::proto::message::SenderKeyDistributionMessage as ProtoSenderKeyDistributionMessage;
use baileys_proto::proto::{Message, WebMessageInfo};
use baileys_signal::{SessionBuilder, SessionRecord, SignalRepository};
use baileys_store::{AuthCreds, BaileysStore};

use crate::registration::decode_big_endian;
use crate::socket::WhatsAppSocket;
use crate::CoreError;

pub const RETRY_KEY_BUNDLE_TYPE: u8 = 0x05;

/// Pre-key and identity bundle extracted from a `<keys>` node within a retry receipt.
#[derive(Debug, Clone)]
pub struct RetryKeyBundle {
    pub registration_id: u32,
    pub identity_key: Vec<u8>,
    pub signed_pre_key_id: u32,
    pub signed_pre_key_pub: Vec<u8>,
    pub signed_pre_key_sig: Vec<u8>,
    pub one_time_pre_key: Option<(Vec<u8>, u32)>,
}

/// Parses `<keys>` and `<registration>` child nodes from a retry receipt stanza.
pub fn extract_session_from_retry_keys(receipt: &BinaryNode) -> Option<RetryKeyBundle> {
    let keys_node = receipt.get_child("keys")?;

    let type_bytes = keys_node
        .get_child("type")
        .and_then(|t| t.content_as_bytes())?;
    if type_bytes.len() != 1 || type_bytes[0] != RETRY_KEY_BUNDLE_TYPE {
        return None;
    }

    let identity = keys_node
        .get_child("identity")
        .and_then(|i| i.content_as_bytes())?;
    let identity_clean = if identity.len() == 33 && identity[0] == 0x05 {
        identity[1..].to_vec()
    } else if identity.len() == 32 {
        identity.to_vec()
    } else {
        return None;
    };

    let reg_bytes = receipt
        .get_child("registration")
        .and_then(|r| r.content_as_bytes())?;
    let registration_id = decode_big_endian(reg_bytes) as u32;

    let skey_node = keys_node.get_child("skey")?;
    let skey_val = skey_node
        .get_child("value")
        .and_then(|v| v.content_as_bytes())?;
    let skey_val_clean = if skey_val.len() == 33 && skey_val[0] == 0x05 {
        skey_val[1..].to_vec()
    } else if skey_val.len() == 32 {
        skey_val.to_vec()
    } else {
        return None;
    };

    let skey_sig = skey_node
        .get_child("signature")
        .and_then(|s| s.content_as_bytes())?
        .to_vec();
    let skey_id_bytes = skey_node
        .get_child("id")
        .and_then(|i| i.content_as_bytes())?;
    let signed_pre_key_id = decode_big_endian(skey_id_bytes) as u32;

    let one_time_pre_key = if let Some(key_node) = keys_node.get_child("key") {
        let pk_id_bytes = key_node.get_child("id").and_then(|i| i.content_as_bytes());
        let pk_val = key_node
            .get_child("value")
            .and_then(|v| v.content_as_bytes());
        match (pk_id_bytes, pk_val) {
            (Some(id_bytes), Some(val_bytes)) => {
                let pk_id = decode_big_endian(id_bytes) as u32;
                let clean_val = if val_bytes.len() == 33 && val_bytes[0] == 0x05 {
                    val_bytes[1..].to_vec()
                } else if val_bytes.len() == 32 {
                    val_bytes.to_vec()
                } else {
                    return None;
                };
                Some((clean_val, pk_id))
            }
            _ => None,
        }
    } else {
        None
    };

    Some(RetryKeyBundle {
        registration_id,
        identity_key: identity_clean,
        signed_pre_key_id,
        signed_pre_key_pub: skey_val_clean,
        signed_pre_key_sig: skey_sig,
        one_time_pre_key,
    })
}

/// Injects a fresh Signal Protocol session for `target_jid` from an extracted `RetryKeyBundle`.
pub async fn inject_retry_session(
    signal: &SignalRepository,
    target_jid: &str,
    bundle: &RetryKeyBundle,
    creds: &AuthCreds,
) -> Result<(), CoreError> {
    let our_identity = KeyPair::try_from(&creds.signed_identity_key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut new_session = SessionRecord::new();
    let onetime = bundle
        .one_time_pre_key
        .as_ref()
        .map(|(v, id)| (v.as_slice(), *id));

    SessionBuilder::init_outgoing(
        &mut new_session,
        &our_identity,
        &bundle.identity_key,
        &bundle.signed_pre_key_pub,
        bundle.signed_pre_key_id,
        onetime,
        bundle.registration_id,
    )
    .map_err(|e| CoreError::Crypto(e.to_string()))?;

    signal
        .save_session(target_jid, &new_session)
        .await
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    debug!(
        "Injected fresh Signal session from retry receipt for '{}'",
        target_jid
    );
    Ok(())
}

/// Asserts that a Signal Double Ratchet session exists for the target JID.
/// If not present, queries WhatsApp gateway for the recipient's prekey bundle.
async fn assert_session_internal(
    signal: &SignalRepository,
    socket: &Mutex<Option<Arc<WhatsAppSocket>>>,
    pending_iq: &Mutex<HashMap<String, tokio::sync::oneshot::Sender<BinaryNode>>>,
    target_jid: &str,
    creds: &AuthCreds,
) -> Result<(), CoreError> {
    let sig_addr = signal.jid_to_signal_address(target_jid);
    if let Ok(sess) = signal.load_session(&sig_addr).await {
        if !sess.is_empty() {
            return Ok(());
        }
    }

    let iq_id = format!("{}.{}-1", rand::random::<u16>(), rand::random::<u16>());
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut lock = pending_iq.lock().await;
        lock.insert(iq_id.clone(), tx);
    }

    let query_iq = BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "encrypt")
        .with_attr("type", "get")
        .with_attr("id", &iq_id)
        .with_child(
            BinaryNode::new("key").with_child(BinaryNode::new("user").with_attr("jid", target_jid)),
        );

    let sock_opt = {
        let lock = socket.lock().await;
        lock.clone()
    };
    let sock = sock_opt.ok_or_else(|| CoreError::Network("Socket not connected".into()))?;
    sock.send_node(query_iq).await?;

    let resp = match tokio::time::timeout(Duration::from_secs(10), rx).await {
        Ok(Ok(r)) => r,
        _ => {
            let mut lock = pending_iq.lock().await;
            lock.remove(&iq_id);
            return Err(CoreError::Network(format!(
                "Timeout or error fetching prekey bundle for {}",
                target_jid
            )));
        }
    };

    let list_node = resp
        .get_child("list")
        .ok_or_else(|| CoreError::Protocol("Missing <list> node in PreKey response".into()))?;

    let user_node = list_node
        .get_child("user")
        .ok_or_else(|| CoreError::Protocol("Missing <user> node in PreKey response".into()))?;

    let reg_id_bytes = user_node
        .get_child("registration")
        .and_then(|r| r.content_as_bytes())
        .ok_or_else(|| CoreError::Protocol("Missing registration id in PreKey bundle".into()))?;
    let registration_id = decode_big_endian(reg_id_bytes) as u32;

    let identity_bytes = user_node
        .get_child("identity")
        .and_then(|i| i.content_as_bytes())
        .ok_or_else(|| CoreError::Protocol("Missing identity pubkey in PreKey bundle".into()))?;

    let skey_node = user_node
        .get_child("skey")
        .ok_or_else(|| CoreError::Protocol("Missing skey node in PreKey bundle".into()))?;

    let skey_id_bytes = skey_node
        .get_child("id")
        .and_then(|i| i.content_as_bytes())
        .ok_or_else(|| CoreError::Protocol("Missing skey id in PreKey bundle".into()))?;
    let skey_id = decode_big_endian(skey_id_bytes) as u32;

    let skey_val = skey_node
        .get_child("value")
        .and_then(|v| v.content_as_bytes())
        .ok_or_else(|| CoreError::Protocol("Missing skey value in PreKey bundle".into()))?;

    let onetime_key = if let Some(key_node) = user_node.get_child("key") {
        let k_id = key_node
            .get_child("id")
            .and_then(|i| i.content_as_bytes())
            .map(|b| decode_big_endian(b) as u32);
        let k_val = key_node
            .get_child("value")
            .and_then(|v| v.content_as_bytes());
        match (k_val, k_id) {
            (Some(val), Some(id)) => Some((val, id)),
            _ => None,
        }
    } else {
        None
    };

    let our_identity = KeyPair::try_from(&creds.signed_identity_key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut new_session = SessionRecord::new();
    baileys_signal::SessionBuilder::init_outgoing(
        &mut new_session,
        &our_identity,
        identity_bytes,
        skey_val,
        skey_id,
        onetime_key,
        registration_id,
    )
    .map_err(|e| CoreError::Crypto(e.to_string()))?;

    signal
        .save_session(&sig_addr, &new_session)
        .await
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    debug!(
        "Successfully asserted Signal session for retry target '{}'",
        target_jid
    );
    Ok(())
}

fn are_jids_same_user_loose(j1: &str, j2: &str) -> bool {
    if j1.is_empty() || j2.is_empty() {
        return false;
    }
    let u1 = j1
        .split('@')
        .next()
        .unwrap_or(j1)
        .split(':')
        .next()
        .unwrap_or(j1);
    let u2 = j2
        .split('@')
        .next()
        .unwrap_or(j2)
        .split(':')
        .next()
        .unwrap_or(j2);
    u1 == u2
}

/// Handles an incoming `<receipt type="retry">` stanza by re-encrypting and resending
/// the requested message with updated session / sender keys.
pub async fn handle_outbound_retry_receipt(
    node: &BinaryNode,
    creds: &AuthCreds,
    store: &dyn BaileysStore,
    signal: &SignalRepository,
    socket: &Mutex<Option<Arc<WhatsAppSocket>>>,
    pending_iq: &Mutex<HashMap<String, tokio::sync::oneshot::Sender<BinaryNode>>>,
    recent_messages: &Mutex<HashMap<String, WebMessageInfo>>,
    outbound_retries: &Mutex<HashMap<String, u32>>,
) -> Result<(), CoreError> {
    let msg_id = node.get_attr("id").unwrap_or("");
    let from = node.get_attr("from").unwrap_or("");
    if msg_id.is_empty() || from.is_empty() {
        return Ok(());
    }

    let participant = node.get_attr("participant");
    let target_jid = participant.unwrap_or(from);

    let retry_count = node
        .get_child("retry")
        .and_then(|r| r.get_attr("count"))
        .and_then(|c| c.parse::<u32>().ok())
        .unwrap_or(1);

    // Prevent infinite ping-pong retry loops (cap at 5 attempts per msg:target)
    let retry_key = format!("{}:{}", msg_id, target_jid);
    {
        let mut retries = outbound_retries.lock().await;
        let count = retries.entry(retry_key).or_insert(0);
        *count += 1;
        if *count > 5 {
            warn!(
                "Max retry attempts (5) reached for outbound msg {} to {}. Dropping retry.",
                msg_id, target_jid
            );
            return Ok(());
        }
    }

    debug!(
        "Handling outbound retry receipt for msg {} from {} (participant: {:?}, attempt: {})...",
        msg_id, from, participant, retry_count
    );

    // 1. If the peer provided a fresh PreKey bundle in <keys>, inject the updated session.
    // Otherwise, assert that an established Signal session exists with target_jid.
    if let Some(bundle) = extract_session_from_retry_keys(node) {
        if let Err(e) = inject_retry_session(signal, target_jid, &bundle, creds).await {
            warn!(
                "Failed to inject session from retry bundle for {}: {}",
                target_jid, e
            );
        }
    } else {
        if let Err(e) = assert_session_internal(signal, socket, pending_iq, target_jid, creds).await
        {
            warn!(
                "Failed to assert session for retry target {}: {}",
                target_jid, e
            );
        }
    }

    // 2. Retrieve original message from in-memory cache or fallback to SQLite store
    let cached_proto = {
        let cache = recent_messages.lock().await;
        cache.get(msg_id).and_then(|m| m.message.clone())
    };

    let proto_msg = if let Some(p) = cached_proto {
        p
    } else if let Ok(Some(stored)) = store.get_message(msg_id).await {
        match Message::decode(&stored.proto_bytes[..]) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Failed to decode stored message {} protobuf for retry: {}",
                    msg_id, e
                );
                return Ok(());
            }
        }
    } else {
        debug!("Outbound retry requested for message {} to {}, but message is no longer in cache or store", msg_id, target_jid);
        return Ok(());
    };

    let is_group = from.ends_with("@g.us") || from == "status@broadcast";
    let our_identity = KeyPair::try_from(&creds.signed_identity_key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    // 3. Prepare payload and encrypt (exact Baileys messages-send.ts retry parity)
    let me_pn = creds.me_jid.as_deref().unwrap_or("");
    let me_lid = creds.me_lid.as_deref().unwrap_or("");
    let is_me =
        are_jids_same_user_loose(target_jid, me_pn) || are_jids_same_user_loose(target_jid, me_lid);

    let mut message_to_send = proto_msg;
    if is_group {
        let is_target_lid = target_jid.ends_with("@lid");
        let group_sender_identity = if is_target_lid && !me_lid.is_empty() {
            me_lid
        } else if !me_pn.is_empty() {
            me_pn
        } else {
            "s.whatsapp.net"
        };

        if let Ok(Some(skdm)) = signal
            .get_sender_key_distribution_message(from, group_sender_identity)
            .await
        {
            message_to_send.sender_key_distribution_message =
                Some(ProtoSenderKeyDistributionMessage {
                    group_id: Some(from.to_string()),
                    axolotl_sender_key_distribution_message: Some(skdm.serialize().to_vec()),
                });
        }
    }

    let final_proto = if is_me {
        Message {
            device_sent_message: Some(Box::new(baileys_proto::proto::message::DeviceSentMessage {
                destination_jid: Some(from.to_string()),
                message: Some(Box::new(message_to_send)),
                phash: None,
            })),
            ..Default::default()
        }
    } else {
        message_to_send
    };

    let mut msg_bytes = Vec::new();
    final_proto
        .encode(&mut msg_bytes)
        .map_err(|e| CoreError::Protocol(format!("Failed to encode retry protobuf: {}", e)))?;
    let padded = pad_random_max16(&msg_bytes);

    let (enc_type, ciphertext) = signal
        .encrypt_message(target_jid, &padded, &our_identity)
        .await
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut retry_node = BinaryNode::new("message")
        .with_attr("id", msg_id)
        .with_attr("type", "text");

    if is_group {
        retry_node = retry_node
            .with_attr("to", from)
            .with_attr("participant", target_jid);
    } else if is_me {
        retry_node = retry_node
            .with_attr("to", target_jid)
            .with_attr("recipient", from);
    } else {
        retry_node = retry_node.with_attr("to", target_jid);
    }

    retry_node = retry_node.with_child(
        BinaryNode::new("enc")
            .with_attr("v", "2")
            .with_attr("type", &enc_type)
            .with_attr("count", retry_count.to_string())
            .with_bytes_content(ciphertext),
    );

    if enc_type == "pkmsg" {
        if let Some(ref acc) = creds.account {
            retry_node = retry_node
                .with_child(BinaryNode::new("device-identity").with_bytes_content(acc.clone()));
        }
    }

    // 4. Send retry node over socket
    let socket_opt = {
        let lock = socket.lock().await;
        lock.clone()
    };

    if let Some(s) = socket_opt {
        s.send_node(retry_node).await?;
        info!(
            "Resent retry message {} to {} (participant: {:?}, attempt: {})",
            msg_id, from, participant, retry_count
        );
    } else {
        warn!(
            "Cannot send retry message {}: socket is disconnected",
            msg_id
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registration::encode_big_endian;
    use baileys_crypto::generate_key_pair;

    #[test]
    fn test_extract_session_from_retry_keys() {
        let identity_kp = generate_key_pair();
        let skey_kp = generate_key_pair();
        let onetime_kp = generate_key_pair();

        let keys_node = BinaryNode::new("keys")
            .with_child(BinaryNode::new("type").with_content(vec![RETRY_KEY_BUNDLE_TYPE]))
            .with_child(BinaryNode::new("identity").with_content(identity_kp.public_key.to_vec()))
            .with_child(
                BinaryNode::new("skey")
                    .with_child(BinaryNode::new("id").with_content(encode_big_endian(1, 3)))
                    .with_child(BinaryNode::new("value").with_content(skey_kp.public_key.to_vec()))
                    .with_child(BinaryNode::new("signature").with_content(vec![0xAA; 64])),
            )
            .with_child(
                BinaryNode::new("key")
                    .with_child(BinaryNode::new("id").with_content(encode_big_endian(10, 3)))
                    .with_child(
                        BinaryNode::new("value").with_content(onetime_kp.public_key.to_vec()),
                    ),
            );

        let receipt_node = BinaryNode::new("receipt")
            .with_attr("type", "retry")
            .with_attr("id", "3EB0TEST123")
            .with_attr("from", "62812345678@s.whatsapp.net")
            .with_child(BinaryNode::new("registration").with_content(encode_big_endian(9999, 4)))
            .with_child(keys_node);

        let bundle =
            extract_session_from_retry_keys(&receipt_node).expect("Failed to extract bundle");
        assert_eq!(bundle.registration_id, 9999);
        assert_eq!(bundle.identity_key, identity_kp.public_key);
        assert_eq!(bundle.signed_pre_key_id, 1);
        assert_eq!(bundle.signed_pre_key_pub, skey_kp.public_key);
        assert_eq!(bundle.signed_pre_key_sig, vec![0xAA; 64]);
        assert_eq!(
            bundle.one_time_pre_key,
            Some((onetime_kp.public_key.to_vec(), 10))
        );
    }

    #[tokio::test]
    async fn test_handle_outbound_retry_receipt_session_injection() {
        use baileys_proto::proto::MessageKey;
        use baileys_store::{MemoryStore, SignalStoreAdapter};

        let store: Arc<dyn BaileysStore> = Arc::new(MemoryStore::new());
        let adapter = Arc::new(SignalStoreAdapter::new(Arc::clone(&store)));
        let signal = Arc::new(SignalRepository::new(adapter));

        let creds = AuthCreds::init();
        let socket = Arc::new(Mutex::new(None));
        let recent_messages = Arc::new(Mutex::new(HashMap::new()));
        let outbound_retries = Arc::new(Mutex::new(HashMap::new()));

        let msg_id = "3EB0MSGRETRY999";
        let target_jid = "628987654321@s.whatsapp.net";

        // 1. Put original message into recent_messages cache
        let original_msg = WebMessageInfo {
            key: Some(MessageKey {
                remote_jid: Some(target_jid.to_string()),
                from_me: Some(true),
                id: Some(msg_id.to_string()),
                participant: None,
            }),
            message: Some(Message {
                conversation: Some("Hello from Estella/Baileys!".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        recent_messages
            .lock()
            .await
            .insert(msg_id.to_string(), original_msg);

        // Verify recipient currently has NO Signal session
        let initial_session = signal.load_session(target_jid).await.unwrap();
        assert!(initial_session.is_empty());

        // 2. Build incoming retry receipt from recipient with fresh PreKey bundle
        let peer_identity = generate_key_pair();
        let peer_skey = generate_key_pair();
        let peer_onetime = generate_key_pair();

        let keys_node = BinaryNode::new("keys")
            .with_child(BinaryNode::new("type").with_content(vec![RETRY_KEY_BUNDLE_TYPE]))
            .with_child(BinaryNode::new("identity").with_content(peer_identity.public_key.to_vec()))
            .with_child(
                BinaryNode::new("skey")
                    .with_child(BinaryNode::new("id").with_content(encode_big_endian(5, 3)))
                    .with_child(
                        BinaryNode::new("value").with_content(peer_skey.public_key.to_vec()),
                    )
                    .with_child(BinaryNode::new("signature").with_content(vec![0xBB; 64])),
            )
            .with_child(
                BinaryNode::new("key")
                    .with_child(BinaryNode::new("id").with_content(encode_big_endian(12, 3)))
                    .with_child(
                        BinaryNode::new("value").with_content(peer_onetime.public_key.to_vec()),
                    ),
            );

        let retry_node = BinaryNode::new("receipt")
            .with_attr("type", "retry")
            .with_attr("id", msg_id)
            .with_attr("from", target_jid)
            .with_child(
                BinaryNode::new("retry")
                    .with_attr("count", "1")
                    .with_attr("id", msg_id),
            )
            .with_child(BinaryNode::new("registration").with_content(encode_big_endian(7777, 4)))
            .with_child(keys_node);

        // 3. Process outbound retry receipt
        let pending_iq = Arc::new(Mutex::new(HashMap::new()));
        handle_outbound_retry_receipt(
            &retry_node,
            &creds,
            store.as_ref(),
            &signal,
            &socket,
            &pending_iq,
            &recent_messages,
            &outbound_retries,
        )
        .await
        .expect("Failed to handle outbound retry receipt");

        // 4. Verify that fresh Signal session is now active and saved
        let updated_session = signal.load_session(target_jid).await.unwrap();
        assert!(
            !updated_session.is_empty(),
            "Signal session was not established from retry receipt!"
        );
        assert_eq!(
            updated_session.get_open_session().unwrap().registration_id,
            7777
        );

        // 5. Verify retry attempt counter was tracked
        let retry_count = outbound_retries
            .lock()
            .await
            .get(&format!("{}:{}", msg_id, target_jid))
            .cloned();
        assert_eq!(retry_count, Some(1));
    }
}
