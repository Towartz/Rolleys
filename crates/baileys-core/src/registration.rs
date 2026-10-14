use baileys_binary::jid::S_WHATSAPP_NET;
use baileys_binary::BinaryNode;
use baileys_crypto::{generate_key_pair, hmac_sha256, md5, sign_ed25519, verify_ed25519, KeyPair};
use baileys_proto::proto::{
    client_payload::*, device_props::*, AdvDeviceIdentity, AdvEncryptionType,
    AdvSignedDeviceIdentity, AdvSignedDeviceIdentityHmac, ClientPayload, DeviceProps,
};
use baileys_store::{AuthCreds, BaileysStore, KeyEntry, SerializableKeyPair};
use prost::Message;
use std::sync::Arc;

use crate::config::BaileysConfig;
use crate::socket::WhatsAppSocket;
use crate::CoreError;

pub const KEY_BUNDLE_TYPE: u8 = 5;
pub const WA_ADV_ACCOUNT_SIG_PREFIX: &[u8] = &[6, 0];
pub const WA_ADV_DEVICE_SIG_PREFIX: &[u8] = &[6, 1];
pub const WA_ADV_HOSTED_ACCOUNT_SIG_PREFIX: &[u8] = &[6, 5];

/// Encodes an unsigned integer as big-endian bytes of specific length.
pub fn encode_big_endian(val: u64, len: usize) -> Vec<u8> {
    let be = val.to_be_bytes();
    be[8 - len..].to_vec()
}

/// Builds the UserAgent protobuf structure matching Baileys & WhatsApp Web standards.
pub fn get_user_agent(config: &BaileysConfig) -> UserAgent {
    let is_android = config.platform.to_lowercase().contains("android")
        || config.browser_name.to_lowercase().contains("android");

    let platform = if is_android {
        user_agent::Platform::Android as i32
    } else {
        user_agent::Platform::Web as i32
    };

    UserAgent {
        platform: Some(platform),
        app_version: Some(user_agent::AppVersion {
            primary: Some(config.app_version.0),
            secondary: Some(config.app_version.1),
            tertiary: Some(config.app_version.2),
            ..Default::default()
        }),
        mcc: Some("000".to_string()),
        mnc: Some("000".to_string()),
        os_version: Some("0.1.0".to_string()),
        device: Some("Desktop".to_string()),
        os_build_number: Some("0.1.0".to_string()),
        locale_language_iso6391: Some("en".to_string()),
        locale_country_iso31661_alpha2: Some("US".to_string()),
        release_channel: Some(user_agent::ReleaseChannel::Release as i32),
        ..Default::default()
    }
}

/// Builds the DeviceProps binary payload embedded in the registration handshake.
pub fn get_device_props(config: &BaileysConfig) -> Vec<u8> {
    let is_android = config.platform.to_lowercase().contains("android")
        || config.browser_name.to_lowercase().contains("android");

    let platform_type = if is_android {
        PlatformType::AndroidPhone as i32
    } else {
        PlatformType::Chrome as i32
    };

    let props = DeviceProps {
        os: Some(config.platform.clone()),
        version: Some(AppVersion {
            primary: Some(10),
            secondary: Some(15),
            tertiary: Some(7),
            ..Default::default()
        }),
        platform_type: Some(platform_type),
        require_full_sync: Some(false),
        history_sync_config: Some(HistorySyncConfig {
            storage_quota_mb: Some(10240),
            inline_initial_payload_in_e2_ee_msg: Some(true),
            support_call_log_history: Some(false),
            support_bot_user_agent_chat_history: Some(true),
            support_cag_reactions_and_polls: Some(true),
            support_biz_hosted_msg: Some(true),
            support_recent_sync_chunk_message_count_tuning: Some(true),
            support_hosted_group_msg: Some(true),
            ..Default::default()
        }),
    };

    props.encode_to_vec()
}

/// Generates the ClientPayload for an unauthenticated device initiating pairing registration.
pub fn generate_registration_node(creds: &AuthCreds, config: &BaileysConfig) -> ClientPayload {
    let version_str = format!(
        "{}.{}.{}",
        config.app_version.0, config.app_version.1, config.app_version.2
    );
    let app_version_hash = md5(version_str.as_bytes());
    let device_props_bytes = get_device_props(config);

    let pairing_data = DevicePairingRegistrationData {
        e_regid: Some(encode_big_endian(creds.registration_id as u64, 4)),
        e_keytype: Some(vec![KEY_BUNDLE_TYPE]),
        e_ident: Some(creds.signed_identity_key.public.to_vec()),
        e_skey_id: Some(encode_big_endian(creds.signed_pre_key.key_id as u64, 3)),
        e_skey_val: Some(creds.signed_pre_key.key_pair.public.to_vec()),
        e_skey_sig: Some(creds.signed_pre_key.signature.to_vec()),
        build_hash: Some(app_version_hash.to_vec()),
        device_props: Some(device_props_bytes),
    };

    let is_android = config.platform.to_lowercase().contains("android")
        || config.browser_name.to_lowercase().contains("android");

    let web_info = if !is_android {
        Some(WebInfo {
            web_sub_platform: Some(web_info::WebSubPlatform::WebBrowser as i32),
            ..Default::default()
        })
    } else {
        None
    };

    ClientPayload {
        connect_type: Some(ConnectType::WifiUnknown as i32),
        connect_reason: Some(ConnectReason::UserActivated as i32),
        user_agent: Some(get_user_agent(config)),
        web_info,
        passive: Some(false),
        pull: Some(false),
        device_pairing_data: Some(pairing_data),
        push_name: Some(config.browser_name.clone()),
        ..Default::default()
    }
}

/// Generates the ClientPayload for an authenticated device reconnecting with existing session.
pub fn generate_login_node(user_jid: &str, config: &BaileysConfig) -> ClientPayload {
    // Parse user number and device from JID (e.g. 1234567890:0@s.whatsapp.net or 1234567890@s.whatsapp.net)
    let (user_part, _) = user_jid.split_once('@').unwrap_or((user_jid, ""));
    let (user_str, device_str) = user_part.split_once(':').unwrap_or((user_part, "0"));

    let username: u64 = user_str.parse().unwrap_or(0);
    let device: u32 = device_str.parse().unwrap_or(0);

    let is_android = config.platform.to_lowercase().contains("android")
        || config.browser_name.to_lowercase().contains("android");

    let web_info = if !is_android {
        Some(WebInfo {
            web_sub_platform: Some(web_info::WebSubPlatform::WebBrowser as i32),
            ..Default::default()
        })
    } else {
        None
    };

    ClientPayload {
        username: Some(username),
        passive: Some(true),
        pull: Some(true),
        device: Some(device),
        connect_type: Some(ConnectType::WifiUnknown as i32),
        connect_reason: Some(ConnectReason::UserActivated as i32),
        user_agent: Some(get_user_agent(config)),
        web_info,
        push_name: Some(config.browser_name.clone()),
        lid_db_migrated: Some(false),
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
pub struct UpdatedPairingCreds {
    pub me_jid: String,
    pub lid: Option<String>,
    pub platform: Option<String>,
    pub biz_name: Option<String>,
    pub account: Option<AdvSignedDeviceIdentity>,
}

/// Handles the `<pair-success>` stanza received from WhatsApp Web Gateway after user scans QR or enters code.
/// Decodes and verifies server signatures, signs the companion identity, and constructs the `<pair-device-sign>` reply.
pub fn configure_successful_pairing(
    stanza: &BinaryNode,
    creds: &AuthCreds,
) -> Result<(BinaryNode, UpdatedPairingCreds), CoreError> {
    let msg_id = stanza
        .attrs
        .get("id")
        .cloned()
        .unwrap_or_else(|| "pair_ack".to_string());

    let pair_success_node = stanza.get_child("pair-success").ok_or_else(|| {
        CoreError::Protocol("Missing pair-success child in pair-device stanza".into())
    })?;

    let device_identity_node = pair_success_node
        .get_child("device-identity")
        .ok_or_else(|| CoreError::Protocol("Missing device-identity in pair-success".into()))?;

    let device_node = pair_success_node
        .get_child("device")
        .ok_or_else(|| CoreError::Protocol("Missing device in pair-success".into()))?;

    let jid = device_node
        .attrs
        .get("jid")
        .cloned()
        .ok_or_else(|| CoreError::Protocol("Missing jid attribute on device node".into()))?;

    let lid = device_node.attrs.get("lid").cloned();
    let platform = pair_success_node
        .get_child("platform")
        .and_then(|p| p.attrs.get("name").cloned());
    let biz_name = pair_success_node
        .get_child("biz")
        .and_then(|b| b.attrs.get("name").cloned());

    let identity_bytes = device_identity_node.content_as_bytes().ok_or_else(|| {
        CoreError::Protocol("Missing content bytes in device-identity node".into())
    })?;

    // 1. Decode AdvSignedDeviceIdentityHmac
    let adv_hmac = AdvSignedDeviceIdentityHmac::decode(identity_bytes).map_err(|e| {
        CoreError::Protocol(format!(
            "Failed to decode AdvSignedDeviceIdentityHmac: {}",
            e
        ))
    })?;

    let details = adv_hmac.details.ok_or_else(|| {
        CoreError::Protocol("Missing details in AdvSignedDeviceIdentityHmac".into())
    })?;

    let hmac = adv_hmac
        .hmac
        .ok_or_else(|| CoreError::Protocol("Missing hmac in AdvSignedDeviceIdentityHmac".into()))?;

    // 2. Validate HMAC using adv_secret_key
    let mut hmac_prefix = Vec::new();
    if adv_hmac.account_type == Some(AdvEncryptionType::Hosted as i32) {
        hmac_prefix.extend_from_slice(WA_ADV_HOSTED_ACCOUNT_SIG_PREFIX);
    }

    let mut hmac_msg = hmac_prefix;
    hmac_msg.extend_from_slice(&details);

    let expected_hmac = hmac_sha256(&hmac_msg, &creds.adv_secret_key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;
    if subtle::ConstantTimeEq::ct_eq(&expected_hmac[..], &hmac[..]).unwrap_u8() != 1 {
        return Err(CoreError::Crypto(
            "Invalid AdvSignedDeviceIdentity HMAC signature".into(),
        ));
    }

    // 3. Decode AdvSignedDeviceIdentity
    let mut account = AdvSignedDeviceIdentity::decode(&details[..]).map_err(|e| {
        CoreError::Protocol(format!("Failed to decode AdvSignedDeviceIdentity: {}", e))
    })?;

    let account_signature_key = account.account_signature_key.as_ref().ok_or_else(|| {
        CoreError::Protocol("Missing accountSignatureKey in AdvSignedDeviceIdentity".into())
    })?;

    let account_signature = account.account_signature.as_ref().ok_or_else(|| {
        CoreError::Protocol("Missing accountSignature in AdvSignedDeviceIdentity".into())
    })?;

    let device_details_bytes = account
        .details
        .as_ref()
        .ok_or_else(|| CoreError::Protocol("Missing details in AdvSignedDeviceIdentity".into()))?;

    let device_identity = AdvDeviceIdentity::decode(&device_details_bytes[..])
        .map_err(|e| CoreError::Protocol(format!("Failed to decode AdvDeviceIdentity: {}", e)))?;

    // 4. Verify account signature with account_signature_key
    let account_sig_prefix =
        if device_identity.device_type == Some(AdvEncryptionType::Hosted as i32) {
            WA_ADV_HOSTED_ACCOUNT_SIG_PREFIX
        } else {
            WA_ADV_ACCOUNT_SIG_PREFIX
        };

    let mut account_msg =
        Vec::with_capacity(account_sig_prefix.len() + device_details_bytes.len() + 32);
    account_msg.extend_from_slice(account_sig_prefix);
    account_msg.extend_from_slice(device_details_bytes);
    account_msg.extend_from_slice(&creds.signed_identity_key.public);

    let sig_valid = verify_ed25519(account_signature_key, &account_msg, account_signature)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;
    if !sig_valid {
        return Err(CoreError::Crypto(
            "Failed to verify account signature in companion registration".into(),
        ));
    }

    // 5. Sign device payload with client signed_identity_key.private
    let mut device_msg = Vec::with_capacity(
        WA_ADV_DEVICE_SIG_PREFIX.len()
            + device_details_bytes.len()
            + 32
            + account_signature_key.len(),
    );
    device_msg.extend_from_slice(WA_ADV_DEVICE_SIG_PREFIX);
    device_msg.extend_from_slice(device_details_bytes);
    device_msg.extend_from_slice(&creds.signed_identity_key.public);
    device_msg.extend_from_slice(account_signature_key);

    let priv_key: &[u8; 32] = creds
        .signed_identity_key
        .private
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Crypto("Invalid private identity key length".into()))?;
    let device_signature = sign_ed25519(priv_key, &device_msg);

    account.device_signature = Some(device_signature.to_vec());

    // 6. Encode signed device identity for the reply (accountSignatureKey omitted in reply per spec)
    let reply_account = AdvSignedDeviceIdentity {
        details: account.details.clone(),
        account_signature_key: None,
        account_signature: account.account_signature.clone(),
        device_signature: account.device_signature.clone(),
    };
    let reply_enc = reply_account.encode_to_vec();

    let key_index = device_identity.key_index.unwrap_or(0);

    // 7. Build <pair-device-sign> reply
    let reply = BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("type", "result")
        .with_attr("id", msg_id)
        .with_child(
            BinaryNode::new("pair-device-sign").with_child(
                BinaryNode::new("device-identity")
                    .with_attr("key-index", key_index.to_string())
                    .with_content(reply_enc),
            ),
        );

    let updated_creds = UpdatedPairingCreds {
        me_jid: jid,
        lid,
        platform,
        biz_name,
        account: Some(account),
    };

    Ok((reply, updated_creds))
}

/// Generates and uploads a batch of one-time PreKeys to the WhatsApp server.
pub async fn upload_pre_keys(
    creds: &mut AuthCreds,
    store: &Arc<dyn BaileysStore>,
    socket: &Arc<WhatsAppSocket>,
    count: usize,
) -> Result<(), CoreError> {
    let mut new_keys = Vec::with_capacity(count);
    let mut store_entries = Vec::with_capacity(count);

    let start_id = creds.next_pre_key_id;
    for i in 0..count {
        let key_id = start_id + i as u32;
        let kp = generate_key_pair();
        let skp = SerializableKeyPair::from(&kp);
        let serialized = serde_json::to_vec(&skp).map_err(|e| {
            CoreError::Store(format!("Failed to serialize prekey {}: {}", key_id, e))
        })?;

        store_entries.push(KeyEntry {
            key_type: "pre-key".to_string(),
            id: key_id.to_string(),
            data: serialized,
        });

        new_keys.push((key_id, kp));
    }

    // Persist pre-keys into store
    store
        .set_keys(store_entries)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;

    // Build the upload IQ node
    let iq_node = build_upload_prekeys_iq(creds, &new_keys);
    socket.send_node(iq_node).await?;

    // Update credentials counter
    creds.next_pre_key_id += count as u32;
    creds.first_unuploaded_pre_key_id = creds.next_pre_key_id;
    store
        .save_creds(creds)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;

    Ok(())
}

/// Builds the PreKeys upload IQ stanza (<iq xmlns="encrypt" type="set">).
pub fn build_upload_prekeys_iq(creds: &AuthCreds, new_prekeys: &[(u32, KeyPair)]) -> BinaryNode {
    let mut list_children = Vec::with_capacity(new_prekeys.len());
    for (key_id, kp) in new_prekeys {
        list_children.push(
            BinaryNode::new("key")
                .with_child(
                    BinaryNode::new("id").with_content(encode_big_endian(*key_id as u64, 3)),
                )
                .with_child(BinaryNode::new("value").with_content(kp.public_key.to_vec())),
        );
    }

    let skey = BinaryNode::new("skey")
        .with_child(
            BinaryNode::new("id")
                .with_content(encode_big_endian(creds.signed_pre_key.key_id as u64, 3)),
        )
        .with_child(
            BinaryNode::new("value").with_content(creds.signed_pre_key.key_pair.public.clone()),
        )
        .with_child(
            BinaryNode::new("signature").with_content(creds.signed_pre_key.signature.clone()),
        );

    let msg_id = format!("3EB0{:X}", rand::random::<u64>());

    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "encrypt")
        .with_attr("type", "set")
        .with_attr("id", msg_id)
        .with_child(
            BinaryNode::new("registration")
                .with_content(encode_big_endian(creds.registration_id as u64, 4)),
        )
        .with_child(BinaryNode::new("type").with_content(vec![KEY_BUNDLE_TYPE]))
        .with_child(
            BinaryNode::new("identity").with_content(creds.signed_identity_key.public.clone()),
        )
        .with_child(BinaryNode::new("list").with_children(list_children))
        .with_child(skey)
}

/// Builds the passive stream activation IQ stanza (<iq xmlns="passive" type="set"><active/></iq>).
pub fn build_passive_iq(tag: &str) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    BinaryNode::new("iq")
        .with_attr("to", S_WHATSAPP_NET)
        .with_attr("xmlns", "passive")
        .with_attr("type", "set")
        .with_attr("id", msg_id)
        .with_child(BinaryNode::new(tag))
}

/// Builds an authentic ACK stanza for any incoming stanza (notification, message, receipt, etc.).
pub fn build_ack_node(
    node: &BinaryNode,
    error_code: Option<u32>,
    me_jid: Option<&str>,
) -> BinaryNode {
    let from = node.get_attr("from").unwrap_or(S_WHATSAPP_NET);
    let id = node.get_attr("id").unwrap_or("");
    let mut ack = BinaryNode::new("ack")
        .with_attr("id", id)
        .with_attr("to", from)
        .with_attr("class", &node.tag);

    if let Some(err) = error_code {
        ack = ack.with_attr("error", err.to_string());
    }
    if let Some(participant) = node.get_attr("participant") {
        ack = ack.with_attr("participant", participant);
    }
    if let Some(recipient) = node.get_attr("recipient") {
        ack = ack.with_attr("recipient", recipient);
    }
    if let Some(t) = node.get_attr("type") {
        ack = ack.with_attr("type", t);
    }
    if node.tag == "message" {
        if let Some(me) = me_jid {
            ack = ack.with_attr("from", me);
        }
    }
    ack
}

/// Decodes big-endian bytes into an unsigned 64-bit integer.
pub fn decode_big_endian(bytes: &[u8]) -> u64 {
    let mut val: u64 = 0;
    for &b in bytes {
        val = (val << 8) | (b as u64);
    }
    val
}

/// Builds an authentic retry receipt node when message decryption fails,
/// requesting the sender to re-transmit the key bundle.
pub fn build_retry_receipt(
    node: &BinaryNode,
    registration_id: u32,
    retry_count: u32,
    keys: Option<BinaryNode>,
) -> BinaryNode {
    let from = node.get_attr("from").unwrap_or(S_WHATSAPP_NET);
    let id = node.get_attr("id").unwrap_or("");
    let timestamp_str = node
        .get_attr("t")
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string()
        });

    let mut receipt = BinaryNode::new("receipt")
        .with_attr("id", id)
        .with_attr("to", from)
        .with_attr("type", "retry");

    if let Some(participant) = node.get_attr("participant") {
        receipt = receipt.with_attr("participant", participant);
    }
    if let Some(recipient) = node.get_attr("recipient") {
        receipt = receipt.with_attr("recipient", recipient);
    }

    let retry_child = BinaryNode::new("retry")
        .with_attr("count", retry_count.to_string())
        .with_attr("id", id)
        .with_attr("t", &timestamp_str)
        .with_attr("v", "1")
        .with_attr("error", "0");

    let reg_child =
        BinaryNode::new("registration").with_content(encode_big_endian(registration_id as u64, 4));

    receipt = receipt.with_child(retry_child).with_child(reg_child);

    if let Some(keys_node) = keys {
        receipt = receipt.with_child(keys_node);
    }

    receipt
}

/// Builds the <keys> node for retry receipts containing a fresh PreKey and credentials.
pub async fn create_retry_keys_node(
    creds: &mut AuthCreds,
    store: &Arc<dyn BaileysStore>,
) -> Result<BinaryNode, CoreError> {
    let key_id = creds.next_pre_key_id;
    let kp = generate_key_pair();
    let skp = SerializableKeyPair::from(&kp);
    let serialized = serde_json::to_vec(&skp).map_err(|e| {
        CoreError::Store(format!(
            "Failed to serialize retry prekey {}: {}",
            key_id, e
        ))
    })?;

    // Save fresh pre-key into store
    store
        .set_keys(vec![KeyEntry {
            key_type: "pre-key".to_string(),
            id: key_id.to_string(),
            data: serialized,
        }])
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;

    // Advance pre-key counter
    creds.next_pre_key_id += 1;
    creds.first_unuploaded_pre_key_id = creds.next_pre_key_id;
    store
        .save_creds(creds)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;

    let key_child = BinaryNode::new("key")
        .with_child(BinaryNode::new("id").with_content(encode_big_endian(key_id as u64, 3)))
        .with_child(BinaryNode::new("value").with_content(kp.public_key.to_vec()));

    let skey_child = BinaryNode::new("skey")
        .with_child(
            BinaryNode::new("id")
                .with_content(encode_big_endian(creds.signed_pre_key.key_id as u64, 3)),
        )
        .with_child(
            BinaryNode::new("value").with_content(creds.signed_pre_key.key_pair.public.clone()),
        )
        .with_child(
            BinaryNode::new("signature").with_content(creds.signed_pre_key.signature.clone()),
        );

    let mut keys_children = vec![
        BinaryNode::new("type").with_content(vec![KEY_BUNDLE_TYPE]),
        BinaryNode::new("identity").with_content(creds.signed_identity_key.public.clone()),
        key_child,
        skey_child,
    ];

    if let Some(ref account_bytes) = creds.account {
        keys_children.push(BinaryNode::new("device-identity").with_content(account_bytes.clone()));
    }

    Ok(BinaryNode::new("keys").with_children(keys_children))
}
