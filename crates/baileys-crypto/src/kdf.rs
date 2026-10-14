use hkdf::Hkdf;
pub use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha2::{Sha256, Sha512};

use crate::CryptoError;

pub type HmacSha256 = Hmac<Sha256>;
pub type HmacSha512 = Hmac<Sha512>;

/// Calculates SHA-256 hash of input buffer.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Calculates MD5 hash of input buffer.
pub fn md5(data: &[u8]) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Calculates HMAC-SHA256 signature.
pub fn hmac_sha256(data: &[u8], key: &[u8]) -> Result<[u8; 32], CryptoError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|_| CryptoError::InvalidKeyLength(key.len(), 32))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().into())
}

/// Calculates HMAC-SHA512 signature.
pub fn hmac_sha512(data: &[u8], key: &[u8]) -> Result<[u8; 64], CryptoError> {
    let mut mac = HmacSha512::new_from_slice(key)
        .map_err(|_| CryptoError::InvalidKeyLength(key.len(), 64))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().into())
}

/// Derives arbitrary length key material using HKDF with SHA-256.
pub fn hkdf_sha256(
    ikm: &[u8],
    length: usize,
    salt: Option<&[u8]>,
    info: Option<&[u8]>,
) -> Result<Vec<u8>, CryptoError> {
    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut okm = vec![0u8; length];
    hk.expand(info.unwrap_or(b""), &mut okm)
        .map_err(|e| CryptoError::KdfFailed(format!("HKDF expand failed: {:?}", e)))?;
    Ok(okm)
}

/// Derives a 32-byte key from an 8-character pairing code using PBKDF2-HMAC-SHA256 with 131,072 iterations.
/// Matches Baileys `derivePairingCodeKey`.
pub fn derive_pairing_code_key(pairing_code: &str, salt: &[u8]) -> Result<[u8; 32], CryptoError> {
    let clean_code = pairing_code.replace(['-', ' '], "");
    let password = clean_code.as_bytes();
    let mut salt_and_block = Vec::with_capacity(salt.len() + 4);
    salt_and_block.extend_from_slice(salt);
    salt_and_block.extend_from_slice(&1u32.to_be_bytes()); // block index = 1

    let mut u = hmac_sha256(&salt_and_block, password)?;
    let mut out = u;

    // 131,072 iterations total (first iteration is u_1)
    const ITERATIONS: usize = 131_072;
    for _ in 1..ITERATIONS {
        u = hmac_sha256(&u, password)?;
        for i in 0..32 {
            out[i] ^= u[i];
        }
    }

    Ok(out)
}

/// WhatsApp AppState 5-key bundle derived from a 32-byte sync key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedAppStateKeys {
    pub index_key: [u8; 32],
    pub value_encryption_key: [u8; 32],
    pub value_mac_key: [u8; 32],
    pub snapshot_mac_key: [u8; 32],
    pub patch_mac_key: [u8; 32],
}

/// Expands a 32-byte AppState sync key into the 5 separate mutation keys used for WhatsApp AppState sync.
/// info = "WhatsApp Mutation Keys", output length = 160 bytes.
pub fn expand_app_state_keys(key_data: &[u8]) -> Result<ExpandedAppStateKeys, CryptoError> {
    let expanded = hkdf_sha256(key_data, 160, None, Some(b"WhatsApp Mutation Keys"))?;

    let mut index_key = [0u8; 32];
    let mut value_encryption_key = [0u8; 32];
    let mut value_mac_key = [0u8; 32];
    let mut snapshot_mac_key = [0u8; 32];
    let mut patch_mac_key = [0u8; 32];

    index_key.copy_from_slice(&expanded[0..32]);
    value_encryption_key.copy_from_slice(&expanded[32..64]);
    value_mac_key.copy_from_slice(&expanded[64..96]);
    snapshot_mac_key.copy_from_slice(&expanded[96..128]);
    patch_mac_key.copy_from_slice(&expanded[128..160]);

    Ok(ExpandedAppStateKeys {
        index_key,
        value_encryption_key,
        value_mac_key,
        snapshot_mac_key,
        patch_mac_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_and_md5() {
        let input = b"WhatsApp Rust Core";
        let sha = sha256(input);
        assert_eq!(sha.len(), 32);

        let md5_hash = md5(input);
        assert_eq!(md5_hash.len(), 16);
    }

    #[test]
    fn test_hmac_sha256() {
        let key = b"my-secret-key-32-bytes-long-now!";
        let data = b"stanza payload data";
        let hmac = hmac_sha256(data, key).unwrap();
        assert_eq!(hmac.len(), 32);
    }

    #[test]
    fn test_expand_app_state_keys() {
        let key_data = [0x55u8; 32];
        let keys = expand_app_state_keys(&key_data).unwrap();
        assert_ne!(keys.index_key, [0u8; 32]);
        assert_ne!(keys.value_encryption_key, [0u8; 32]);
        assert_ne!(keys.value_mac_key, [0u8; 32]);
        assert_ne!(keys.snapshot_mac_key, [0u8; 32]);
        assert_ne!(keys.patch_mac_key, [0u8; 32]);
    }

    #[test]
    fn test_derive_pairing_code_key() {
        let expected = "be826c544b97c824ce0af400280935549a89135bc6318cf8c8c2139099b0384b";
        let derived_raw = derive_pairing_code_key("73NREGY3", &[1u8; 32]).unwrap();
        let derived_hyphen = derive_pairing_code_key("73NR-EGY3", &[1u8; 32]).unwrap();
        assert_eq!(hex::encode(derived_raw), expected);
        assert_eq!(hex::encode(derived_hyphen), expected);
    }
}
