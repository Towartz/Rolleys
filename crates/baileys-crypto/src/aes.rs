use aes::cipher::{
    block_padding::{NoPadding, Pkcs7},
    BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit, StreamCipher,
};
use aes_gcm::{aead::Aead, Aes256Gcm, Key, Nonce};
use rand::RngCore;

use crate::CryptoError;

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type Aes256Ctr64BE = ctr::Ctr64BE<aes::Aes256>;

pub const GCM_TAG_LENGTH: usize = 16;
pub const CBC_BLOCK_SIZE: usize = 16;
pub const AES_256_KEY_SIZE: usize = 32;

/// Encrypts plaintext using AES-256-GCM.
/// Appends the 16-byte authentication tag to the end of the ciphertext, matching Baileys behavior.
pub fn aes_256_gcm_encrypt(
    plaintext: &[u8],
    key: &[u8],
    iv: &[u8],
    additional_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if key.len() != AES_256_KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength(key.len(), AES_256_KEY_SIZE));
    }
    if iv.len() != 12 {
        return Err(CryptoError::InvalidIvLength(iv.len(), 12));
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(iv);

    let payload = aes_gcm::aead::Payload {
        msg: plaintext,
        aad: additional_data,
    };

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    // aes-gcm crate already appends the 16-byte tag to the ciphertext
    Ok(ciphertext)
}

/// Decrypts ciphertext using AES-256-GCM.
/// Expects the 16-byte authentication tag appended at the end of the ciphertext, matching Baileys behavior.
pub fn aes_256_gcm_decrypt(
    ciphertext: &[u8],
    key: &[u8],
    iv: &[u8],
    additional_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if key.len() != AES_256_KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength(key.len(), AES_256_KEY_SIZE));
    }
    if iv.len() != 12 {
        return Err(CryptoError::InvalidIvLength(iv.len(), 12));
    }
    if ciphertext.len() < GCM_TAG_LENGTH {
        return Err(CryptoError::DecryptionFailed(
            "Ciphertext shorter than 16-byte GCM tag".into(),
        ));
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(iv);

    let payload = aes_gcm::aead::Payload {
        msg: ciphertext,
        aad: additional_data,
    };

    cipher
        .decrypt(nonce, payload)
        .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
}

/// Encrypts plaintext using AES-256-CBC with PKCS7 padding.
pub fn aes_256_cbc_encrypt(
    plaintext: &[u8],
    key: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if key.len() != AES_256_KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength(key.len(), AES_256_KEY_SIZE));
    }
    if iv.len() != CBC_BLOCK_SIZE {
        return Err(CryptoError::InvalidIvLength(iv.len(), CBC_BLOCK_SIZE));
    }

    let cipher = Aes256CbcEnc::new(key.into(), iv.into());
    Ok(cipher.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// Decrypts ciphertext using AES-256-CBC with PKCS7 unpadding.
pub fn aes_256_cbc_decrypt(
    ciphertext: &[u8],
    key: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if key.len() != AES_256_KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength(key.len(), AES_256_KEY_SIZE));
    }
    if iv.len() != CBC_BLOCK_SIZE {
        return Err(CryptoError::InvalidIvLength(iv.len(), CBC_BLOCK_SIZE));
    }
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(CBC_BLOCK_SIZE) {
        return Err(CryptoError::DecryptionFailed(
            "Ciphertext not a multiple of 16-byte CBC block size".into(),
        ));
    }

    let cipher = Aes256CbcDec::new(key.into(), iv.into());
    cipher
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|e| CryptoError::DecryptionFailed(format!("CBC unpad error: {:?}", e)))
}

/// Encrypts using AES-256-CBC with a randomly generated 16-byte IV prepended to the returned buffer.
/// Matches Baileys `aesEncrypt(buffer, key)`.
pub fn aes_256_cbc_encrypt_prefixed_iv(
    plaintext: &[u8],
    key: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let mut iv = [0u8; CBC_BLOCK_SIZE];
    rand::thread_rng().fill_bytes(&mut iv);

    let mut result = Vec::with_capacity(CBC_BLOCK_SIZE + plaintext.len() + CBC_BLOCK_SIZE);
    result.extend_from_slice(&iv);
    let ciphertext = aes_256_cbc_encrypt(plaintext, key, &iv)?;
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypts AES-256-CBC where the first 16 bytes of ciphertext are the IV.
/// Matches Baileys `aesDecrypt(buffer, key)`.
pub fn aes_256_cbc_decrypt_prefixed_iv(data: &[u8], key: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < CBC_BLOCK_SIZE {
        return Err(CryptoError::DecryptionFailed(
            "Data too short to contain 16-byte CBC IV".into(),
        ));
    }

    let (iv, ciphertext) = data.split_at(CBC_BLOCK_SIZE);
    aes_256_cbc_decrypt(ciphertext, key, iv)
}

/// Encrypts or decrypts data using AES-256-CTR mode (symmetric stream cipher).
/// Matches Baileys `aesEncryptCTR` and `aesDecryptCTR`.
pub fn aes_256_ctr(plaintext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if key.len() != AES_256_KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength(key.len(), AES_256_KEY_SIZE));
    }
    if iv.len() != 16 {
        return Err(CryptoError::InvalidIvLength(iv.len(), 16));
    }

    let mut cipher = Aes256Ctr64BE::new(key.into(), iv.into());
    let mut buffer = plaintext.to_vec();
    cipher.apply_keystream(&mut buffer);
    Ok(buffer)
}

pub fn aes_256_ctr_encrypt(
    plaintext: &[u8],
    key: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    aes_256_ctr(plaintext, key, iv)
}

pub fn aes_256_ctr_decrypt(
    ciphertext: &[u8],
    key: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    aes_256_ctr(ciphertext, key, iv)
}

/// Adds random padding up to 16 bytes according to WhatsApp / Signal protocol standards.
/// Appends `pad` bytes where each byte has the value `pad` (1 <= pad <= 16).
pub fn pad_random_max16(buf: &[u8]) -> Vec<u8> {
    let pad_len = (rand::random::<u8>() % 16) + 1;
    let mut out = Vec::with_capacity(buf.len() + pad_len as usize);
    out.extend_from_slice(buf);
    out.resize(buf.len() + pad_len as usize, pad_len);
    out
}

/// Removes WhatsApp / Signal random padding.
pub fn unpad_random_max16(buf: &[u8]) -> &[u8] {
    if buf.is_empty() {
        return buf;
    }
    let pad = buf[buf.len() - 1] as usize;
    if (1..=16).contains(&pad) && buf.len() >= pad {
        &buf[..buf.len() - pad]
    } else {
        buf
    }
}

/// Stateful AES-256-CBC streaming encryptor.
pub struct Aes256CbcStreamEncryptor {
    key: [u8; 32],
    current_iv: [u8; 16],
    buffer: Vec<u8>,
}

impl Aes256CbcStreamEncryptor {
    pub fn new(key: &[u8], iv: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != AES_256_KEY_SIZE {
            return Err(CryptoError::InvalidKeyLength(key.len(), AES_256_KEY_SIZE));
        }
        if iv.len() != CBC_BLOCK_SIZE {
            return Err(CryptoError::InvalidIvLength(iv.len(), CBC_BLOCK_SIZE));
        }
        let mut k = [0u8; 32];
        let mut i = [0u8; 16];
        k.copy_from_slice(key);
        i.copy_from_slice(iv);
        Ok(Self {
            key: k,
            current_iv: i,
            buffer: Vec::with_capacity(CBC_BLOCK_SIZE),
        })
    }

    /// Feeds plaintext bytes and returns any full encrypted ciphertext blocks.
    pub fn update(&mut self, data: &[u8]) -> Vec<u8> {
        self.buffer.extend_from_slice(data);
        let num_blocks = self.buffer.len() / CBC_BLOCK_SIZE;
        if num_blocks == 0 {
            return Vec::new();
        }
        let block_bytes = num_blocks * CBC_BLOCK_SIZE;
        let blocks_to_encrypt: Vec<u8> = self.buffer.drain(..block_bytes).collect();
        let cipher = Aes256CbcEnc::new((&self.key).into(), (&self.current_iv).into());
        let encrypted = cipher.encrypt_padded_vec_mut::<NoPadding>(&blocks_to_encrypt);
        if let Some(last_block) = encrypted.chunks_exact(CBC_BLOCK_SIZE).last() {
            self.current_iv.copy_from_slice(last_block);
        }
        encrypted
    }

    /// Finalizes the stream by applying PKCS#7 padding to any remaining buffered bytes and encrypting the final block(s).
    pub fn finalize(self) -> Vec<u8> {
        let cipher = Aes256CbcEnc::new((&self.key).into(), (&self.current_iv).into());
        cipher.encrypt_padded_vec_mut::<Pkcs7>(&self.buffer)
    }
}

/// Stateful AES-256-CBC streaming decryptor.
pub struct Aes256CbcStreamDecryptor {
    key: [u8; 32],
    current_iv: [u8; 16],
    buffer: Vec<u8>,
}

impl Aes256CbcStreamDecryptor {
    pub fn new(key: &[u8], iv: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != AES_256_KEY_SIZE {
            return Err(CryptoError::InvalidKeyLength(key.len(), AES_256_KEY_SIZE));
        }
        if iv.len() != CBC_BLOCK_SIZE {
            return Err(CryptoError::InvalidIvLength(iv.len(), CBC_BLOCK_SIZE));
        }
        let mut k = [0u8; 32];
        let mut i = [0u8; 16];
        k.copy_from_slice(key);
        i.copy_from_slice(iv);
        Ok(Self {
            key: k,
            current_iv: i,
            buffer: Vec::with_capacity(CBC_BLOCK_SIZE * 2),
        })
    }

    /// Feeds ciphertext bytes and returns decrypted plaintext for all blocks except the last block
    /// (which is retained until finalize to strip PKCS#7 padding).
    pub fn update(&mut self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.buffer.extend_from_slice(data);
        if self.buffer.len() <= CBC_BLOCK_SIZE {
            return Ok(Vec::new());
        }

        // Keep at least 16 bytes in buffer so finalize can unpad the last block
        let num_blocks = (self.buffer.len() - 1) / CBC_BLOCK_SIZE;
        if num_blocks == 0 {
            return Ok(Vec::new());
        }
        let bytes_to_decrypt = num_blocks * CBC_BLOCK_SIZE;
        let blocks: Vec<u8> = self.buffer.drain(..bytes_to_decrypt).collect();

        let cipher = Aes256CbcDec::new((&self.key).into(), (&self.current_iv).into());
        let decrypted = cipher
            .decrypt_padded_vec_mut::<NoPadding>(&blocks)
            .map_err(|e| {
                CryptoError::DecryptionFailed(format!("CBC block decrypt error: {:?}", e))
            })?;

        self.current_iv
            .copy_from_slice(&blocks[blocks.len() - CBC_BLOCK_SIZE..]);
        Ok(decrypted)
    }

    /// Finalizes the decryption stream by unpadding the remaining 16-byte block.
    pub fn finalize(self) -> Result<Vec<u8>, CryptoError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        if self.buffer.len() % CBC_BLOCK_SIZE != 0 {
            return Err(CryptoError::DecryptionFailed(
                "Trailing ciphertext not block aligned".into(),
            ));
        }
        let cipher = Aes256CbcDec::new((&self.key).into(), (&self.current_iv).into());
        cipher
            .decrypt_padded_vec_mut::<Pkcs7>(&self.buffer)
            .map_err(|e| CryptoError::DecryptionFailed(format!("CBC unpad error: {:?}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_256_gcm_roundtrip() {
        let key = [0x42u8; 32];
        let iv = [0x11u8; 12];
        let aad = b"additional authenticated data";
        let plaintext = b"Hello, WhatsApp Multiplatform Rust!";

        let encrypted = aes_256_gcm_encrypt(plaintext, &key, &iv, aad).unwrap();
        assert_eq!(encrypted.len(), plaintext.len() + 16);

        let decrypted = aes_256_gcm_decrypt(&encrypted, &key, &iv, aad).unwrap();
        assert_eq!(decrypted, plaintext);

        // Corrupt tag -> must fail
        let mut corrupted = encrypted.clone();
        *corrupted.last_mut().unwrap() ^= 0xFF;
        assert!(aes_256_gcm_decrypt(&corrupted, &key, &iv, aad).is_err());
    }

    #[test]
    fn test_aes_256_cbc_roundtrip() {
        let key = [0x33u8; 32];
        let iv = [0x77u8; 16];
        let plaintext = b"Sensitive WhatsApp AppState mutation data";

        let encrypted = aes_256_cbc_encrypt(plaintext, &key, &iv).unwrap();
        let decrypted = aes_256_cbc_decrypt(&encrypted, &key, &iv).unwrap();
        assert_eq!(decrypted, plaintext);

        // Test prefixed IV
        let prefixed = aes_256_cbc_encrypt_prefixed_iv(plaintext, &key).unwrap();
        assert_eq!(&prefixed[..16].len(), &16);
        let decrypted_prefixed = aes_256_cbc_decrypt_prefixed_iv(&prefixed, &key).unwrap();
        assert_eq!(decrypted_prefixed, plaintext);
    }

    #[test]
    fn test_aes_256_ctr_roundtrip() {
        let key = [0x55u8; 32];
        let iv = [0x99u8; 16];
        let plaintext = b"Companion pairing ephemeral public key 32-bytes";

        let ciphertext = aes_256_ctr_encrypt(plaintext, &key, &iv).unwrap();
        assert_eq!(ciphertext.len(), plaintext.len());
        assert_ne!(ciphertext, plaintext);

        let decrypted = aes_256_ctr_decrypt(&ciphertext, &key, &iv).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_256_ctr_cross_verify_node() {
        let key = hex::decode("be826c544b97c824ce0af400280935549a89135bc6318cf8c8c2139099b0384b")
            .unwrap();
        let iv = [2u8; 16];
        let plain = [0x42u8; 32];
        let enc = aes_256_ctr_encrypt(&plain, &key, &iv).unwrap();
        assert_eq!(
            hex::encode(enc),
            "4e3347fcfaa3c2db1833ea2e034244c61ee6bec45ef996fc7ba3ac813ff7c385"
        );
    }

    #[test]
    fn test_pad_unpad_random_max16() {
        let plain = b"WhatsApp encrypted payload bytes";
        let padded = pad_random_max16(plain);
        assert!(padded.len() > plain.len());
        assert!(padded.len() <= plain.len() + 16);
        let unpadded = unpad_random_max16(&padded);
        assert_eq!(unpadded, plain);
    }

    #[test]
    fn test_aes_256_cbc_streaming_chunked_roundtrip() {
        let key = [0x4Au8; 32];
        let iv = [0x2Bu8; 16];

        let test_payloads: Vec<Vec<u8>> = vec![
            b"".to_vec(),
            b"Short".to_vec(),
            b"ExactlySixteen!".to_vec(),
            b"SeventeenBytes!!X".to_vec(),
            b"A somewhat longer payload representing an encrypted media segment or message stanza chunk that spans several blocks."
                .to_vec(),
            (0..5000).map(|i| (i % 256) as u8).collect(),
        ];

        let chunk_sizes = [1, 3, 7, 15, 16, 17, 33, 64, 1024];

        for payload in test_payloads {
            // One-shot encryption baseline
            let baseline_ciphertext = aes_256_cbc_encrypt(&payload, &key, &iv).unwrap();
            let baseline_decrypted = aes_256_cbc_decrypt(&baseline_ciphertext, &key, &iv).unwrap();
            assert_eq!(baseline_decrypted, payload);

            for &enc_chunk_sz in &chunk_sizes {
                // Streaming encrypt
                let mut encryptor = Aes256CbcStreamEncryptor::new(&key, &iv).unwrap();
                let mut stream_ciphertext = Vec::new();
                for chunk in payload.chunks(enc_chunk_sz) {
                    stream_ciphertext.extend_from_slice(&encryptor.update(chunk));
                }
                stream_ciphertext.extend_from_slice(&encryptor.finalize());

                assert_eq!(
                    stream_ciphertext,
                    baseline_ciphertext,
                    "Streaming ciphertext mismatch for payload len {} with chunk sz {}",
                    payload.len(),
                    enc_chunk_sz
                );

                for &dec_chunk_sz in &chunk_sizes {
                    // Streaming decrypt
                    let mut decryptor = Aes256CbcStreamDecryptor::new(&key, &iv).unwrap();
                    let mut stream_decrypted = Vec::new();
                    for chunk in stream_ciphertext.chunks(dec_chunk_sz) {
                        stream_decrypted.extend_from_slice(&decryptor.update(chunk).unwrap());
                    }
                    stream_decrypted.extend_from_slice(&decryptor.finalize().unwrap());

                    assert_eq!(
                        stream_decrypted, payload,
                        "Streaming decrypted mismatch for payload len {} with enc chunk sz {} and dec chunk sz {}",
                        payload.len(),
                        enc_chunk_sz,
                        dec_chunk_sz
                    );
                }
            }
        }
    }
}
