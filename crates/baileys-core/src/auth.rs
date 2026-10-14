use baileys_proto::proto::{
    client_payload::{
        user_agent::{AppVersion, Platform, ReleaseChannel},
        ConnectReason, ConnectType, UserAgent,
    },
    ClientPayload, HandshakeMessage,
};
use baileys_store::AuthCreds;

use crate::config::BaileysConfig;

pub fn build_client_hello(ephemeral_public: &[u8]) -> HandshakeMessage {
    HandshakeMessage {
        client_hello: Some(baileys_proto::proto::handshake_message::ClientHello {
            ephemeral: Some(ephemeral_public.to_vec()),
            ..Default::default()
        }),
        server_hello: None,
        client_finish: None,
    }
}

pub fn build_client_finish(static_key_enc: Vec<u8>, payload_enc: Vec<u8>) -> HandshakeMessage {
    HandshakeMessage {
        client_hello: None,
        server_hello: None,
        client_finish: Some(baileys_proto::proto::handshake_message::ClientFinish {
            r#static: Some(static_key_enc),
            payload: Some(payload_enc),
            ..Default::default()
        }),
    }
}

pub fn build_client_payload(creds: &AuthCreds, config: &BaileysConfig) -> ClientPayload {
    let platform = if config.platform.to_lowercase().contains("android") {
        Platform::Android as i32
    } else {
        Platform::Web as i32
    };

    let user_agent = UserAgent {
        app_version: Some(AppVersion {
            primary: Some(config.app_version.0),
            secondary: Some(config.app_version.1),
            tertiary: Some(config.app_version.2),
            ..Default::default()
        }),
        platform: Some(platform),
        release_channel: Some(ReleaseChannel::Release as i32),
        mcc: Some("000".to_string()),
        mnc: Some("000".to_string()),
        os_version: Some("0.1".to_string()),
        device: Some(config.browser_name.clone()),
        os_build_number: Some("0.1".to_string()),
        locale_language_iso6391: Some("en".to_string()),
        locale_country_iso31661_alpha2: Some("US".to_string()),
        ..Default::default()
    };

    let (username, device) = match &creds.me_jid {
        Some(jid) => {
            if let Some(decoded) = baileys_binary::jid_decode(jid) {
                (
                    decoded.user.parse::<u64>().ok(),
                    decoded.device.map(|d| d as u32),
                )
            } else {
                (None, None)
            }
        }
        None => (None, None),
    };

    ClientPayload {
        username,
        passive: Some(true),
        user_agent: Some(user_agent),
        push_name: creds.me_name.clone(),
        session_id: Some(rand::random::<i32>()),
        connect_type: Some(ConnectType::WifiUnknown as i32),
        connect_reason: Some(ConnectReason::UserActivated as i32),
        device,
        pull: Some(true),
        ..Default::default()
    }
}

pub const CROCKFORD_CHARACTERS: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTVWXYZ";

/// Converts binary bytes into Crockford Base32 string (matching upstream Baileys bytesToCrockford).
pub fn bytes_to_crockford(bytes: &[u8]) -> String {
    let mut value: u64 = 0;
    let mut bit_count = 0;
    let mut crockford = String::with_capacity(8);

    for &b in bytes {
        value = (value << 8) | (b as u64);
        bit_count += 8;

        while bit_count >= 5 {
            let idx = ((value >> (bit_count - 5)) & 31) as usize;
            crockford.push(CROCKFORD_CHARACTERS[idx] as char);
            bit_count -= 5;
        }
    }

    if bit_count > 0 {
        let idx = ((value << (5 - bit_count)) & 31) as usize;
        crockford.push(CROCKFORD_CHARACTERS[idx] as char);
    }

    crockford
}

/// Generates a clean 8-character Crockford Base32 pairing code without hyphens (e.g. "73NREGY3").
pub fn generate_pairing_code() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 5];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes_to_crockford(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_crockford() {
        let input = [1u8, 2, 3, 4, 5];
        assert_eq!(bytes_to_crockford(&input), "15217216");
    }

    #[test]
    fn test_generate_pairing_code_format() {
        let code = generate_pairing_code();
        assert_eq!(code.len(), 8);
        assert!(code
            .chars()
            .all(|c| CROCKFORD_CHARACTERS.contains(&(c as u8))));
    }
}
