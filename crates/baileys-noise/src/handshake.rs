use baileys_binary::{BinaryDecoder, BinaryNode};
use baileys_crypto::{
    aes_256_gcm_decrypt, aes_256_gcm_encrypt, calculate_agreement, hkdf_sha256, sha256,
    verify_ed25519, KeyPair,
};
use baileys_proto::proto::{cert_chain::noise_certificate::Details, CertChain};
use prost::Message;

use crate::constants::*;
use crate::transport::TransportState;
use crate::NoiseError;

pub struct NoiseHandler {
    key_pair: KeyPair,
    hash: [u8; 32],
    salt: [u8; 32],
    enc_key: [u8; 32],
    dec_key: [u8; 32],
    counter: u32,
    sent_intro: bool,
    intro_header: Vec<u8>,
    transport: Option<TransportState>,
}

impl NoiseHandler {
    pub fn new(key_pair: KeyPair, routing_info: Option<&[u8]>) -> Self {
        let mut hash = [0u8; 32];
        hash.copy_from_slice(NOISE_MODE);

        let salt = hash;
        let enc_key = hash;
        let dec_key = hash;

        let intro_header = if let Some(routing) = routing_info {
            let mut header = Vec::with_capacity(7 + routing.len() + NOISE_WA_HEADER.len());
            header.extend_from_slice(b"ED");
            header.push(0);
            header.push(1);
            header.push((routing.len() >> 16) as u8);
            header.push(((routing.len() >> 8) & 0xFF) as u8);
            header.push((routing.len() & 0xFF) as u8);
            header.extend_from_slice(routing);
            header.extend_from_slice(NOISE_WA_HEADER);
            header
        } else {
            NOISE_WA_HEADER.to_vec()
        };

        let mut handler = Self {
            key_pair,
            hash,
            salt,
            enc_key,
            dec_key,
            counter: 0,
            sent_intro: false,
            intro_header,
            transport: None,
        };

        // Authenticate header & public key
        handler.authenticate(NOISE_WA_HEADER);
        handler.authenticate(&handler.key_pair.public_key.clone());

        handler
    }

    pub fn is_transport_ready(&self) -> bool {
        self.transport.is_some()
    }

    pub fn authenticate(&mut self, data: &[u8]) {
        if self.transport.is_none() {
            let mut combined = Vec::with_capacity(32 + data.len());
            combined.extend_from_slice(&self.hash);
            combined.extend_from_slice(data);
            self.hash = sha256(&combined);
        }
    }

    fn generate_iv(counter: u32) -> [u8; 12] {
        let mut iv = [0u8; 12];
        iv[8..12].copy_from_slice(&counter.to_be_bytes());
        iv
    }

    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if let Some(transport) = &mut self.transport {
            return transport.encrypt(plaintext);
        }

        let iv = Self::generate_iv(self.counter);
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or(NoiseError::CounterOverflow)?;

        let result = aes_256_gcm_encrypt(plaintext, &self.enc_key, &iv, &self.hash)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;
        self.authenticate(&result);
        Ok(result)
    }

    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if let Some(transport) = &mut self.transport {
            return transport.decrypt(ciphertext);
        }

        let iv = Self::generate_iv(self.counter);
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or(NoiseError::CounterOverflow)?;

        let result = aes_256_gcm_decrypt(ciphertext, &self.dec_key, &iv, &self.hash)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;
        self.authenticate(ciphertext);
        Ok(result)
    }

    fn local_hkdf(&self, data: &[u8]) -> Result<([u8; 32], [u8; 32]), NoiseError> {
        let key = hkdf_sha256(data, 64, Some(&self.salt), None)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;

        let mut write = [0u8; 32];
        let mut read = [0u8; 32];
        write.copy_from_slice(&key[0..32]);
        read.copy_from_slice(&key[32..64]);
        Ok((write, read))
    }

    pub fn mix_into_key(&mut self, data: &[u8]) -> Result<(), NoiseError> {
        let (write, read) = self.local_hkdf(data)?;
        self.salt = write;
        self.enc_key = read;
        self.dec_key = read;
        self.counter = 0;
        Ok(())
    }

    /// Processes server Hello handshake payload, verifies certificate chain, and computes client finish static key.
    pub fn process_handshake(
        &mut self,
        server_ephemeral: &[u8],
        server_static_ciphertext: &[u8],
        server_payload_ciphertext: &[u8],
        noise_static_key: &KeyPair,
    ) -> Result<Vec<u8>, NoiseError> {
        self.authenticate(server_ephemeral);

        let shared1 = calculate_agreement(&self.key_pair.private_key, server_ephemeral)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;
        self.mix_into_key(&shared1)?;

        let dec_static_content = self.decrypt(server_static_ciphertext)?;
        let shared2 = calculate_agreement(&self.key_pair.private_key, &dec_static_content)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;
        self.mix_into_key(&shared2)?;

        let cert_decoded = self.decrypt(server_payload_ciphertext)?;
        let cert_chain = CertChain::decode(&cert_decoded[..])
            .map_err(|e| NoiseError::CertificateInvalid(e.to_string()))?;

        let leaf = cert_chain
            .leaf
            .ok_or_else(|| NoiseError::CertificateInvalid("Missing leaf certificate".into()))?;
        let intermediate = cert_chain.intermediate.ok_or_else(|| {
            NoiseError::CertificateInvalid("Missing intermediate certificate".into())
        })?;

        let leaf_details = leaf
            .details
            .as_deref()
            .ok_or_else(|| NoiseError::CertificateInvalid("Missing leaf details".into()))?;
        let leaf_sig = leaf
            .signature
            .as_deref()
            .ok_or_else(|| NoiseError::CertificateInvalid("Missing leaf signature".into()))?;

        let inter_details = intermediate
            .details
            .as_deref()
            .ok_or_else(|| NoiseError::CertificateInvalid("Missing intermediate details".into()))?;
        let inter_sig = intermediate.signature.as_deref().ok_or_else(|| {
            NoiseError::CertificateInvalid("Missing intermediate signature".into())
        })?;

        // 1. Verify intermediate certificate with WhatsApp root key
        let inter_valid = verify_ed25519(&WA_CERT_PUBLIC_KEY, inter_details, inter_sig)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;
        if !inter_valid {
            return Err(NoiseError::CertificateInvalid(
                "Intermediate cert signature invalid".into(),
            ));
        }

        // 2. Decode intermediate details to get issuer and intermediate public key
        let details = Details::decode(inter_details)
            .map_err(|e| NoiseError::CertificateInvalid(e.to_string()))?;

        if details.issuer_serial() != WA_CERT_SERIAL {
            return Err(NoiseError::CertificateInvalid(format!(
                "Cert serial mismatch: expected {}, got {}",
                WA_CERT_SERIAL,
                details.issuer_serial()
            )));
        }

        let inter_pubkey = details.key.as_deref().ok_or_else(|| {
            NoiseError::CertificateInvalid("Missing intermediate public key".into())
        })?;

        // 3. Verify leaf certificate with intermediate public key
        let leaf_valid = verify_ed25519(inter_pubkey, leaf_details, leaf_sig)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;
        if !leaf_valid {
            return Err(NoiseError::CertificateInvalid(
                "Leaf cert signature invalid".into(),
            ));
        }

        // 4. Encrypt client static noise public key
        let key_enc = self.encrypt(&noise_static_key.public_key)?;

        // 5. Final DH with static private key and server ephemeral
        let shared3 = calculate_agreement(&noise_static_key.private_key, server_ephemeral)
            .map_err(|e| NoiseError::Crypto(e.to_string()))?;
        self.mix_into_key(&shared3)?;

        Ok(key_enc)
    }

    /// Transitions session from handshake to transport state.
    pub fn finish_init(&mut self) -> Result<(), NoiseError> {
        let (write, read) = self.local_hkdf(&[])?;
        self.transport = Some(TransportState::new(write, read));
        Ok(())
    }

    /// Encodes a payload into a framed WhatsApp message chunk:
    /// `[introHeader (first frame only)] [3-byte big endian len] [ciphertext or handshake payload]`
    pub fn encode_frame(&mut self, payload: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let data = if let Some(transport) = &mut self.transport {
            transport.encrypt(payload)?
        } else {
            payload.to_vec()
        };
        let data_len = data.len();

        let intro_size = if self.sent_intro {
            0
        } else {
            self.intro_header.len()
        };
        let mut frame = Vec::with_capacity(intro_size + 3 + data_len);

        if !self.sent_intro {
            frame.extend_from_slice(&self.intro_header);
            self.sent_intro = true;
        }

        // 3-byte big-endian length prefix
        frame.push(((data_len >> 16) & 0xFF) as u8);
        frame.push(((data_len >> 8) & 0xFF) as u8);
        frame.push((data_len & 0xFF) as u8);

        frame.extend_from_slice(&data);
        Ok(frame)
    }

    /// Decrypts a received ciphertext frame and decodes it as a WABinary XML node.
    pub fn decode_frame(&mut self, ciphertext: &[u8]) -> Result<BinaryNode, NoiseError> {
        let plaintext = if let Some(transport) = &mut self.transport {
            transport.decrypt(ciphertext)?
        } else {
            ciphertext.to_vec()
        };
        BinaryDecoder::decode(&plaintext).map_err(|e| NoiseError::Binary(e.to_string()))
    }
}
