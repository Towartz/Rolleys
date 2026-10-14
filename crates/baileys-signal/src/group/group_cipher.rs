use crate::group::sender_key_distribution_message::SenderKeyDistributionMessage;
use crate::group::sender_key_message::SenderKeyMessage;
use crate::group::sender_key_record::SenderKeyRecord;
use crate::group::sender_key_state::SenderKeyState;
use crate::group::sender_message_key::SenderMessageKey;
use crate::SignalError;
use baileys_crypto::{
    aes_256_cbc_decrypt, aes_256_cbc_encrypt, generate_key_pair, generate_signing_key_pair,
};

pub struct GroupCipher;

impl GroupCipher {
    /// Initializes a new group sender key in the record and returns a distribution message to send to group participants.
    pub fn create_session(
        record: &mut SenderKeyRecord,
        key_id: u32,
    ) -> Result<SenderKeyDistributionMessage, SignalError> {
        let signing_key = generate_signing_key_pair();
        let chain_key_pair = generate_key_pair();

        record.set_sender_key_state(
            key_id,
            0,
            chain_key_pair.private_key.to_vec(),
            signing_key.public_key,
            Some(signing_key.private_key),
        );

        SenderKeyDistributionMessage::new(
            key_id,
            0,
            chain_key_pair.private_key.to_vec(),
            baileys_crypto::format_signal_pubkey(&signing_key.public_key),
        )
    }

    /// Processes an incoming SenderKeyDistributionMessage from another participant.
    pub fn process_distribution_message(
        record: &mut SenderKeyRecord,
        skdm: &SenderKeyDistributionMessage,
    ) -> Result<(), SignalError> {
        let mut signing_pub = [0u8; 32];
        let key_bytes = if skdm.signing_key().len() == 33 && skdm.signing_key()[0] == 0x05 {
            &skdm.signing_key()[1..]
        } else if skdm.signing_key().len() == 32 {
            skdm.signing_key()
        } else {
            return Err(SignalError::InvalidKeyLength);
        };
        signing_pub.copy_from_slice(key_bytes);

        record.add_sender_key_state(
            skdm.id(),
            skdm.iteration(),
            skdm.chain_key().to_vec(),
            signing_pub,
        );

        Ok(())
    }

    /// Encrypts a message using our group sender key.
    pub fn encrypt(record: &mut SenderKeyRecord, plaintext: &[u8]) -> Result<Vec<u8>, SignalError> {
        let state = record
            .get_sender_key_state_mut()
            .ok_or(SignalError::NoSession)?;

        let priv_key = state.signing_key_private().copied().ok_or_else(|| {
            SignalError::Crypto("No private signing key in sender key state".into())
        })?;

        let chain_key = state.sender_chain_key().clone();
        let sender_key = chain_key.get_sender_message_key()?;
        state.set_sender_chain_key(chain_key.get_next()?);

        let ciphertext = aes_256_cbc_encrypt(plaintext, sender_key.cipher_key(), sender_key.iv())
            .map_err(|e| SignalError::Crypto(e.to_string()))?;

        let msg = SenderKeyMessage::new(
            state.key_id(),
            sender_key.iteration(),
            ciphertext,
            &priv_key,
        )?;

        Ok(msg.serialize().to_vec())
    }

    /// Decrypts a group message using the sender's key state in the record.
    pub fn decrypt(
        record: &mut SenderKeyRecord,
        sender_key_message_bytes: &[u8],
    ) -> Result<Vec<u8>, SignalError> {
        let msg = SenderKeyMessage::deserialize(sender_key_message_bytes)?;

        let state = record
            .get_sender_key_state_by_id_mut(msg.key_id())
            .ok_or(SignalError::NoSession)?;

        let valid_sig = msg.verify_signature(state.signing_key_public())?;
        if !valid_sig {
            return Err(SignalError::InvalidSignature);
        }

        let sender_key = Self::get_sender_key(state, msg.iteration())?;

        let plaintext =
            aes_256_cbc_decrypt(msg.ciphertext(), sender_key.cipher_key(), sender_key.iv())
                .map_err(|e| SignalError::Crypto(e.to_string()))?;

        Ok(plaintext)
    }

    fn get_sender_key(
        state: &mut SenderKeyState,
        iteration: u32,
    ) -> Result<SenderMessageKey, SignalError> {
        if state.sender_chain_key().iteration() > iteration {
            if let Some(key) = state.remove_sender_message_key(iteration) {
                return Ok(key);
            }
            return Err(SignalError::DuplicateOrOldCounter(
                state.sender_chain_key().iteration(),
                iteration,
            ));
        }

        if iteration.saturating_sub(state.sender_chain_key().iteration()) > 2000 {
            return Err(SignalError::CounterTooFarAhead);
        }

        let mut chain_key = state.sender_chain_key().clone();
        while chain_key.iteration() < iteration {
            state.add_sender_message_key(chain_key.get_sender_message_key()?);
            chain_key = chain_key.get_next()?;
        }

        state.set_sender_chain_key(chain_key.get_next()?);
        chain_key.get_sender_message_key()
    }
}
