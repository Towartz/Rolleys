use baileys_binary::jid::{jid_encode, S_WHATSAPP_NET};
use baileys_binary::BinaryNode;
use baileys_crypto::{
    aes_256_ctr_decrypt, aes_256_ctr_encrypt, aes_256_gcm_encrypt, calculate_agreement,
    derive_pairing_code_key, generate_key_pair, hkdf_sha256, KeyPair,
};
use baileys_store::AuthCreds;
use rand::RngCore;

use crate::auth::generate_pairing_code;
use crate::config::BaileysConfig;
use crate::CoreError;

pub fn get_companion_platform_id(browser_name: &str, platform: &str) -> &'static str {
    let name = browser_name.to_lowercase();
    let plat = platform.to_lowercase();

    if name.contains("chrome") {
        "1"
    } else if name.contains("edge") {
        "2"
    } else if name.contains("firefox") {
        "3"
    } else if name.contains("opera") {
        "5"
    } else if name.contains("safari") {
        "6"
    } else if name.contains("desktop") {
        if plat.contains("windows") {
            "8"
        } else {
            "7"
        }
    } else if plat.contains("android") {
        "0"
    } else {
        "1" // Default to Chrome (1) for maximum compatibility with WhatsApp Web gateway
    }
}

pub struct CompanionPairingState {
    pub pairing_code: String,
    pub ephemeral_key: KeyPair,
    pub phone_number: String,
}

impl CompanionPairingState {
    pub fn new(phone_number: &str) -> Self {
        Self {
            pairing_code: generate_pairing_code(),
            ephemeral_key: generate_key_pair(),
            phone_number: phone_number.to_string(),
        }
    }

    /// Builds the `link_code_companion_reg` with stage="companion_hello" to request linking with an 8-digit code.
    pub fn build_companion_hello_iq(
        &self,
        creds: &AuthCreds,
        config: &BaileysConfig,
    ) -> Result<BinaryNode, CoreError> {
        let mut salt = [0u8; 32];
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut iv);

        let key = derive_pairing_code_key(&self.pairing_code, &salt)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let ciphered = aes_256_ctr_encrypt(&self.ephemeral_key.public_key, &key, &iv)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let mut wrapped = Vec::with_capacity(salt.len() + iv.len() + ciphered.len());
        wrapped.extend_from_slice(&salt);
        wrapped.extend_from_slice(&iv);
        wrapped.extend_from_slice(&ciphered);

        let clean_phone = self.phone_number.replace(['+', ' ', '-'], "");
        let jid = jid_encode(&clean_phone, S_WHATSAPP_NET, None, None);
        let platform_id = get_companion_platform_id(&config.browser_name, &config.platform);
        // Baileys parity: `${browser[1]} (${browser[0]})` -> e.g. "Chrome (Mac OS)"
        let platform_display = format!("{} ({})", config.browser_name, config.platform);

        let msg_id = format!("{}.{}-1", rand::random::<u16>(), rand::random::<u16>());

        let hello_node = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("id", msg_id)
            .with_attr("xmlns", "md")
            .with_child(
                BinaryNode::new("link_code_companion_reg")
                    .with_attr("jid", jid)
                    .with_attr("stage", "companion_hello")
                    .with_attr("should_show_push_notification", "true")
                    .with_child(
                        BinaryNode::new("link_code_pairing_wrapped_companion_ephemeral_pub")
                            .with_content(wrapped),
                    )
                    .with_child(
                        BinaryNode::new("companion_server_auth_key_pub")
                            .with_content(creds.noise_key.public.to_vec()),
                    )
                    .with_child(
                        BinaryNode::new("companion_platform_id").with_string_content(platform_id),
                    )
                    .with_child(
                        BinaryNode::new("companion_platform_display")
                            .with_string_content(platform_display),
                    )
                    .with_child(
                        BinaryNode::new("link_code_pairing_nonce").with_string_content("0"),
                    ),
            );

        Ok(hello_node)
    }

    /// Handles incoming `link_code_companion_reg stage="companion_finish"`, derives shared keys, and builds reply.
    pub fn handle_companion_finish_iq(
        &self,
        node: &BinaryNode,
        creds: &mut AuthCreds,
    ) -> Result<BinaryNode, CoreError> {
        let reg_node = node
            .get_child("link_code_companion_reg")
            .ok_or_else(|| CoreError::Protocol("Missing link_code_companion_reg child".into()))?;

        let ref_bytes = reg_node
            .get_child("link_code_pairing_ref")
            .and_then(|c| c.content_as_bytes())
            .ok_or_else(|| CoreError::Protocol("Missing link_code_pairing_ref".into()))?;

        let primary_identity_pub = reg_node
            .get_child("primary_identity_pub")
            .and_then(|c| c.content_as_bytes())
            .ok_or_else(|| CoreError::Protocol("Missing primary_identity_pub".into()))?;

        let primary_wrapped = reg_node
            .get_child("link_code_pairing_wrapped_primary_ephemeral_pub")
            .and_then(|c| c.content_as_bytes())
            .ok_or_else(|| CoreError::Protocol("Missing primary_wrapped_ephemeral".into()))?;

        if primary_wrapped.len() < 48 {
            return Err(CoreError::Protocol("primary_wrapped too short".into()));
        }

        let salt = &primary_wrapped[0..32];
        let iv = &primary_wrapped[32..48];
        let ciphertext = &primary_wrapped[48..];

        let key = derive_pairing_code_key(&self.pairing_code, salt)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let code_pairing_public_key = aes_256_ctr_decrypt(ciphertext, &key, iv)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        if code_pairing_public_key.len() != 32 {
            return Err(CoreError::Crypto(
                "Invalid decrypted ephemeral public key length".into(),
            ));
        }

        let companion_shared_key =
            calculate_agreement(&self.ephemeral_key.private_key, &code_pairing_public_key)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let mut random = [0u8; 32];
        let mut link_code_salt = [0u8; 32];
        let mut encrypt_iv = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut random);
        rand::thread_rng().fill_bytes(&mut link_code_salt);
        rand::thread_rng().fill_bytes(&mut encrypt_iv);

        let link_code_pairing_expanded = hkdf_sha256(
            &companion_shared_key,
            32,
            Some(&link_code_salt),
            Some(b"link_code_pairing_key_bundle_encryption_key"),
        )
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let mut encrypt_payload = Vec::with_capacity(32 + primary_identity_pub.len() + 32);
        encrypt_payload.extend_from_slice(&creds.signed_identity_key.public);
        encrypt_payload.extend_from_slice(primary_identity_pub);
        encrypt_payload.extend_from_slice(&random);

        let encrypted = aes_256_gcm_encrypt(
            &encrypt_payload,
            &link_code_pairing_expanded,
            &encrypt_iv,
            &[],
        )
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let mut encrypted_payload =
            Vec::with_capacity(link_code_salt.len() + encrypt_iv.len() + encrypted.len());
        encrypted_payload.extend_from_slice(&link_code_salt);
        encrypted_payload.extend_from_slice(&encrypt_iv);
        encrypted_payload.extend_from_slice(&encrypted);

        let identity_shared_key =
            calculate_agreement(&creds.signed_identity_key.private, primary_identity_pub)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let mut identity_payload = Vec::with_capacity(
            companion_shared_key.len() + identity_shared_key.len() + random.len(),
        );
        identity_payload.extend_from_slice(&companion_shared_key);
        identity_payload.extend_from_slice(&identity_shared_key);
        identity_payload.extend_from_slice(&random);

        let adv_secret = hkdf_sha256(&identity_payload, 32, None, Some(b"adv_secret"))
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        creds.adv_secret_key = adv_secret;

        let clean_phone = self.phone_number.replace(['+', ' ', '-'], "");
        let jid = jid_encode(&clean_phone, S_WHATSAPP_NET, None, None);
        creds.me_jid = Some(jid.clone());
        let msg_id = format!("{}.{}-2", rand::random::<u16>(), rand::random::<u16>());

        let reply = BinaryNode::new("iq")
            .with_attr("to", S_WHATSAPP_NET)
            .with_attr("type", "set")
            .with_attr("id", msg_id)
            .with_attr("xmlns", "md")
            .with_child(
                BinaryNode::new("link_code_companion_reg")
                    .with_attr("jid", jid)
                    .with_attr("stage", "companion_finish")
                    .with_child(
                        BinaryNode::new("link_code_pairing_wrapped_key_bundle")
                            .with_content(encrypted_payload),
                    )
                    .with_child(
                        BinaryNode::new("companion_identity_public")
                            .with_content(creds.signed_identity_key.public.to_vec()),
                    )
                    .with_child(
                        BinaryNode::new("link_code_pairing_ref").with_content(ref_bytes.to_vec()),
                    ),
            );

        Ok(reply)
    }
}
