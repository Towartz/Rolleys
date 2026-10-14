//! WhatsApp End-to-End Encryption engine (Signal Protocol & Group Sender Keys).

pub mod group;
pub mod lid;
pub mod repository;
pub mod session;

pub use group::*;
pub use lid::*;
pub use repository::*;
pub use session::*;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SignalError {
    #[error("Cryptographic error in Signal protocol: {0}")]
    Crypto(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Invalid Signal message length")]
    InvalidMessageLength,

    #[error("Unsupported Signal message version: {0}")]
    UnsupportedVersion(u8),

    #[error("Malformed or invalid Signal message structure")]
    InvalidMessage,

    #[error("Signal message MAC/signature verification failed")]
    InvalidSignature,

    #[error("Requested PreKey {0} not found in store")]
    PreKeyNotFound(u32),

    #[error("Invalid key length")]
    InvalidKeyLength,

    #[error("No established Signal session found")]
    NoSession,

    #[error("Received message with duplicate or old counter: current {0}, received {1}")]
    DuplicateOrOldCounter(u32, u32),

    #[error("Received message counter is over 2000 messages into the future")]
    CounterTooFarAhead,
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_crypto::{generate_key_pair, KeyPair};
    use std::sync::Arc;

    #[test]
    fn test_1on1_signal_double_ratchet_exchange() {
        let alice_identity = generate_key_pair();
        let bob_identity = generate_key_pair();
        let bob_signed_prekey = generate_key_pair();
        let bob_onetime_prekey = generate_key_pair();

        let mut alice_record = SessionRecord::new();
        let mut bob_record = SessionRecord::new();

        // Alice initiates session using Bob's prekey bundle
        SessionBuilder::init_outgoing(
            &mut alice_record,
            &alice_identity,
            &bob_identity.public_key,
            &bob_signed_prekey.public_key,
            1,
            Some((&bob_onetime_prekey.public_key, 10)),
            12345,
        )
        .expect("Alice init_outgoing failed");

        // Alice encrypts first message (pkmsg)
        let msg1 = b"Hello Bob from Alice!";
        let (msg1_type, msg1_cipher) =
            SessionCipher::encrypt_message(&mut alice_record, msg1, &alice_identity)
                .expect("Alice encryption failed");
        assert_eq!(msg1_type, 3); // PreKeyWhisperMessage

        // Bob decrypts pkmsg
        let (decrypted1, consumed_prekey) = SessionCipher::decrypt_pre_key_whisper_message(
            &mut bob_record,
            &msg1_cipher,
            &bob_identity,
            &bob_signed_prekey,
            Some(&bob_onetime_prekey),
        )
        .expect("Bob decryption failed");
        assert_eq!(decrypted1, msg1);
        assert_eq!(consumed_prekey, Some(10));

        // Bob replies to Alice, confirming the session and stepping the ratchet
        let bob_reply = b"Hello Alice, this is Bob!";
        let (bob_msg_type, bob_cipher) =
            SessionCipher::encrypt_message(&mut bob_record, bob_reply, &bob_identity)
                .expect("Bob encryption failed");
        assert_eq!(bob_msg_type, 1); // Bob sends standard WhisperMessage

        let alice_decrypted =
            SessionCipher::decrypt_whisper_message(&mut alice_record, &bob_cipher, &alice_identity)
                .expect("Alice decryption of Bob's reply failed");
        assert_eq!(alice_decrypted, bob_reply);

        // Subsequent messages from Alice (now msg / type 1)
        for i in 1..=3 {
            let text = format!("Ongoing message #{} from Alice", i);
            let (msg_type, cipher) =
                SessionCipher::encrypt_message(&mut alice_record, text.as_bytes(), &alice_identity)
                    .expect("Alice encryption failed");
            assert_eq!(msg_type, 1); // WhisperMessage

            let dec =
                SessionCipher::decrypt_whisper_message(&mut bob_record, &cipher, &bob_identity)
                    .expect("Bob decryption failed");
            assert_eq!(dec, text.as_bytes());
        }
    }

    #[test]
    fn test_group_cipher_full_exchange() {
        let mut alice_record = SenderKeyRecord::new();
        let mut bob_record = SenderKeyRecord::new();

        // 1. Alice creates group session with key_id 100
        let skdm =
            GroupCipher::create_session(&mut alice_record, 100).expect("Session creation failed");

        // 2. Bob receives Alice's distribution message
        GroupCipher::process_distribution_message(&mut bob_record, &skdm)
            .expect("Process SKDM failed");

        // 3. Alice encrypts messages, Bob decrypts them
        for i in 0..5 {
            let plaintext = format!("Group message payload from Alice #{}", i);
            let ciphertext = GroupCipher::encrypt(&mut alice_record, plaintext.as_bytes())
                .expect("Encryption failed");

            let decrypted =
                GroupCipher::decrypt(&mut bob_record, &ciphertext).expect("Decryption failed");
            assert_eq!(decrypted, plaintext.as_bytes());
        }

        // 4. Charlie joins late after 5 messages, receives Alice's current distribution message (iteration 5)
        let skdm_charlie = alice_record
            .get_distribution_message()
            .expect("Failed to get SKDM for Charlie");
        assert_eq!(skdm_charlie.iteration(), 5);
        let mut charlie_record = SenderKeyRecord::new();
        GroupCipher::process_distribution_message(&mut charlie_record, &skdm_charlie)
            .expect("Process SKDM failed");

        // Alice encrypts message 5; both Bob (original member) and Charlie (late member) decrypt it
        let plaintext_5 = b"Message 5 for Bob and Charlie";
        let ct_5 = GroupCipher::encrypt(&mut alice_record, plaintext_5).expect("Encrypt 5 failed");
        let dec_bob_5 = GroupCipher::decrypt(&mut bob_record, &ct_5).expect("Bob decrypt 5 failed");
        let dec_charlie_5 =
            GroupCipher::decrypt(&mut charlie_record, &ct_5).expect("Charlie decrypt 5 failed");
        assert_eq!(dec_bob_5, plaintext_5);
        assert_eq!(dec_charlie_5, plaintext_5);
    }

    #[test]
    fn test_lid_mapping_store() {
        let store = LidMappingStore::new();
        store.store_mapping("123456789@s.whatsapp.net", "987654321@lid");

        assert_eq!(
            store.get_lid_for_pn("123456789@s.whatsapp.net"),
            Some("987654321@lid".to_string())
        );
        assert_eq!(
            store.get_pn_for_lid("987654321@lid"),
            Some("123456789@s.whatsapp.net".to_string())
        );
    }

    #[test]
    fn test_jid_to_signal_address_normalization() {
        struct MockStore;
        #[async_trait::async_trait]
        impl SignalStore for MockStore {
            async fn get_session(&self, _a: &str) -> Result<Option<Vec<u8>>, SignalError> {
                Ok(None)
            }
            async fn set_session(&self, _a: &str, _s: &[u8]) -> Result<(), SignalError> {
                Ok(())
            }
            async fn get_sender_key(&self, _k: &str) -> Result<Option<Vec<u8>>, SignalError> {
                Ok(None)
            }
            async fn set_sender_key(&self, _k: &str, _s: &[u8]) -> Result<(), SignalError> {
                Ok(())
            }
            async fn get_pre_key(&self, _p: u32) -> Result<Option<KeyPair>, SignalError> {
                Ok(None)
            }
            async fn remove_pre_key(&self, _p: u32) -> Result<(), SignalError> {
                Ok(())
            }
        }

        let repo = SignalRepository::new(Arc::new(MockStore));
        assert_eq!(
            repo.jid_to_signal_address("62812345678@s.whatsapp.net"),
            "62812345678:0"
        );
        assert_eq!(
            repo.jid_to_signal_address("62812345678:2@s.whatsapp.net"),
            "62812345678:2"
        );
        assert_eq!(
            repo.jid_to_signal_address("62812345678.3@s.whatsapp.net"),
            "62812345678:3"
        );
        assert_eq!(repo.jid_to_signal_address("62812345678.0"), "62812345678:0");
        assert_eq!(repo.jid_to_signal_address("62812345678:1"), "62812345678:1");
    }

    #[test]
    fn test_sender_key_record_distribution_message() {
        let mut record = SenderKeyRecord::new();
        assert!(record.get_distribution_message().is_none());

        let skdm1 = GroupCipher::create_session(&mut record, 42).expect("Session creation failed");
        let skdm2 = record
            .get_distribution_message()
            .expect("Failed to get distribution message");

        assert_eq!(skdm1.id(), skdm2.id());
        assert_eq!(skdm1.iteration(), skdm2.iteration());
        assert_eq!(skdm1.chain_key(), skdm2.chain_key());
        assert_eq!(skdm1.signing_key(), skdm2.signing_key());
    }

    #[test]
    fn test_cross_verify_with_baileys_upstream() {
        let skdm_bytes = hex::decode("3308d18484d90710001a203db59341fbed73ff24a51cabe70d26a78a0f2ceb05c5c46b0d43b5b5af0ba69d222105f417283517c24f2a973a5454e40dcb580c0ab23de68fe3f4c749a70fb9a20157").unwrap();
        let ct_bytes = hex::decode("3308d18484d90710001a206f1c7c56e994be6b4ef526ba6fcc7f834c718a7ebb1c36e5260569c94b14656c6c08c9ff8787e2f9d7f91d76468d2c2c31216ade686f770220222ac153c715a8d9f2bbd246c04a8494b023a94a0810a4e991fc427d3ea2ef0f665f22eb572985").unwrap();

        let skdm = SenderKeyDistributionMessage::deserialize(&skdm_bytes)
            .expect("Deserialize SKDM from Baileys failed");

        let mut record = SenderKeyRecord::new();
        GroupCipher::process_distribution_message(&mut record, &skdm)
            .expect("Process SKDM from Baileys failed");

        let decrypted = GroupCipher::decrypt(&mut record, &ct_bytes)
            .expect("Decrypt ciphertext from Baileys failed");

        assert_eq!(decrypted, b"Hello from Baileys upstream TS!");
    }

    #[test]
    fn test_export_rust_vector() {
        let mut alice = SenderKeyRecord::new();
        let skdm = GroupCipher::create_session(&mut alice, 7777).unwrap();
        let plaintext = b"Hello from Rust Rose-Baileys to Upstream Baileys!";
        let ciphertext = GroupCipher::encrypt(&mut alice, plaintext).unwrap();

        println!("RUST_VECTOR_SKDM: {}", hex::encode(skdm.serialize()));
        println!("RUST_VECTOR_CT: {}", hex::encode(ciphertext));
    }
}
