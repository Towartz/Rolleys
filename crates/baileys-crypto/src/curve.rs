use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::CryptoError;

pub const KEY_BUNDLE_TYPE: u8 = 0x05;

/// Represents a 32-byte public and private keypair.
#[derive(Clone)]
pub struct KeyPair {
    pub public_key: [u8; 32],
    pub private_key: [u8; 32],
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPair")
            .field("public_key", &hex::encode(self.public_key))
            .field("private_key", &"[REDACTED]")
            .finish()
    }
}

/// Generates a new random X25519 KeyPair for Diffie-Hellman key exchange.
pub fn generate_key_pair() -> KeyPair {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = X25519PublicKey::from(&secret);
    KeyPair {
        public_key: *public.as_bytes(),
        private_key: secret.to_bytes(),
    }
}

/// Generates a new random Curve25519 KeyPair for digital signatures and identity.
pub fn generate_signing_key_pair() -> KeyPair {
    generate_key_pair()
}

/// Computes X25519 Diffie-Hellman shared agreement between private key and public key.
/// If public key is 33 bytes (prefixed with 0x05), the prefix is stripped.
pub fn calculate_agreement(private_key: &[u8], public_key: &[u8]) -> Result<[u8; 32], CryptoError> {
    if private_key.len() != 32 {
        return Err(CryptoError::InvalidKeyLength(private_key.len(), 32));
    }

    let pub_slice = if public_key.len() == 33 && public_key[0] == KEY_BUNDLE_TYPE {
        &public_key[1..]
    } else if public_key.len() == 32 {
        public_key
    } else {
        return Err(CryptoError::InvalidKeyLength(public_key.len(), 32));
    };

    let secret_bytes: [u8; 32] = private_key.try_into().unwrap();
    let secret = StaticSecret::from(secret_bytes);

    let pub_bytes: [u8; 32] = pub_slice.try_into().unwrap();
    let pub_point = X25519PublicKey::from(pub_bytes);

    let shared = secret.diffie_hellman(&pub_point);
    Ok(*shared.as_bytes())
}

/// Formats public key with the 0x05 Signal key bundle prefix (total 33 bytes).
pub fn format_signal_pubkey(pub_key: &[u8]) -> Vec<u8> {
    if pub_key.len() == 33 && pub_key[0] == KEY_BUNDLE_TYPE {
        pub_key.to_vec()
    } else {
        let mut out = Vec::with_capacity(1 + pub_key.len());
        out.push(KEY_BUNDLE_TYPE);
        out.extend_from_slice(pub_key);
        out
    }
}

/// Signs a message using Curve25519 private key (XEdDSA compatible with libsignal/curve25519-js).
pub fn sign_ed25519(private_key: &[u8; 32], message: &[u8]) -> [u8; 64] {
    use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
    use curve25519_dalek::scalar::Scalar;
    use sha2::{Digest, Sha512};

    // 1. Clamp private key according to Curve25519 / XEdDSA spec
    let mut a = *private_key;
    a[0] &= 248;
    a[31] &= 127;
    a[31] |= 64;

    let scalar_a = Scalar::from_bytes_mod_order(a);
    let big_a = ED25519_BASEPOINT_TABLE * &scalar_a;
    let compressed_a = big_a.compress().to_bytes();
    let sign_bit = compressed_a[31] & 0x80;

    // 2. Nonce derivation r = SHA512(0xFE, 0xFF * 31, a, message)
    let mut hasher = Sha512::new();
    hasher.update([0xfe]);
    hasher.update([0xff; 31]);
    hasher.update(a);
    hasher.update(message);
    let r_hash: [u8; 64] = hasher.finalize().into();
    let r = Scalar::from_bytes_mod_order_wide(&r_hash);

    // 3. R = r * B
    let big_r = ED25519_BASEPOINT_TABLE * &r;
    let compressed_r = big_r.compress().to_bytes();

    // 4. h = SHA512(R, A, message)
    let mut hasher2 = Sha512::new();
    hasher2.update(compressed_r);
    hasher2.update(compressed_a);
    hasher2.update(message);
    let h_hash: [u8; 64] = hasher2.finalize().into();
    let h = Scalar::from_bytes_mod_order_wide(&h_hash);

    // 5. s = r + h * a
    let s = r + h * scalar_a;

    // 6. Signature (R, S) with sign_bit embedded in last byte
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&compressed_r);
    sig[32..].copy_from_slice(s.as_bytes());
    sig[63] |= sign_bit;
    sig
}

/// Verifies an Ed25519 / XEdDSA signature (compatible with WhatsApp certificates and Curve25519 identity keys).
pub fn verify_ed25519(
    public_key: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<bool, CryptoError> {
    let pub_slice = if public_key.len() == 33 && public_key[0] == KEY_BUNDLE_TYPE {
        &public_key[1..]
    } else if public_key.len() == 32 {
        public_key
    } else {
        return Err(CryptoError::InvalidKeyLength(public_key.len(), 32));
    };

    if signature.len() != 64 {
        return Err(CryptoError::InvalidSignatureLength(signature.len()));
    }

    let pub_bytes: [u8; 32] = pub_slice.try_into().unwrap();
    let sig_bytes: [u8; 64] = signature.try_into().unwrap();

    // 1. Try XEd25519 / Curve25519 verification (WhatsApp Noise certs and Signal Curve25519 identities)
    let sign_bit = (sig_bytes[63] >> 7) & 1;
    let mut clean_sig = sig_bytes;
    clean_sig[63] &= 0x7F;

    let mp = curve25519_dalek::montgomery::MontgomeryPoint(pub_bytes);
    if let Some(ed_point) = mp.to_edwards(sign_bit) {
        let ed_bytes = ed_point.compress().to_bytes();
        if let Ok(verifying_key) = VerifyingKey::from_bytes(&ed_bytes) {
            let sig = Signature::from_bytes(&clean_sig);
            if verifying_key.verify(message, &sig).is_ok() {
                return Ok(true);
            }
        }
    }

    // 2. Fallback to standard Ed25519 verification (direct Edwards public key)
    if let Ok(verifying_key) = VerifyingKey::from_bytes(&pub_bytes) {
        let sig = Signature::from_bytes(&sig_bytes);
        if verifying_key.verify(message, &sig).is_ok() {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x25519_key_exchange() {
        let alice = generate_key_pair();
        let bob = generate_key_pair();

        let shared_alice = calculate_agreement(&alice.private_key, &bob.public_key).unwrap();
        let shared_bob = calculate_agreement(&bob.private_key, &alice.public_key).unwrap();

        assert_eq!(shared_alice, shared_bob);
        assert_ne!(shared_alice, [0u8; 32]);

        // Also test with 33-byte prefixed public key
        let bob_prefixed = format_signal_pubkey(&bob.public_key);
        assert_eq!(bob_prefixed.len(), 33);
        assert_eq!(bob_prefixed[0], 0x05);
        let shared_alice_prefixed = calculate_agreement(&alice.private_key, &bob_prefixed).unwrap();
        assert_eq!(shared_alice, shared_alice_prefixed);
    }

    #[test]
    fn test_ed25519_signing_and_verification() {
        let sk = [7u8; 32];
        let secret = StaticSecret::from(sk);
        let pk = X25519PublicKey::from(&secret);

        let msg = b"hello test";
        let sig = sign_ed25519(&sk, msg);

        let valid = verify_ed25519(pk.as_bytes(), msg, &sig).unwrap();
        assert!(valid);

        // Prefixed with 0x05 Signal byte
        let pk_prefixed = format_signal_pubkey(pk.as_bytes());
        let valid_prefixed = verify_ed25519(&pk_prefixed, msg, &sig).unwrap();
        assert!(valid_prefixed);

        // Corrupted message should fail
        let invalid = verify_ed25519(pk.as_bytes(), b"corrupted", &sig).unwrap();
        assert!(!invalid);
    }

    #[test]
    fn test_cross_verify_curve25519_js() {
        let pub_bytes =
            hex::decode("a4e09292b651c278b9772c569f5fa9bb13d906b46ab68c9df9dc2b4409f8a209")
                .unwrap();
        let sig_bytes = hex::decode("0e0e07c643783edeebf679f8ef84f6a4308b533c0d6338b37b43f94ad68ec09b34661e0e039c15674fa04445392044b53dfe543a2c968c592bd84e542b8bd003").unwrap();
        let msg = b"hello world";

        let sign_bit = (sig_bytes[63] >> 7) & 1;
        let mut clean_sig = [0u8; 64];
        clean_sig.copy_from_slice(&sig_bytes);
        clean_sig[63] &= 0x7F;

        let mut mont_bytes = [0u8; 32];
        mont_bytes.copy_from_slice(&pub_bytes);
        let mp = curve25519_dalek::montgomery::MontgomeryPoint(mont_bytes);
        let ed_point = mp
            .to_edwards(sign_bit)
            .expect("Failed to convert to edwards");
        let ed_bytes = ed_point.compress().to_bytes();

        let verifying_key =
            ed25519_dalek::VerifyingKey::from_bytes(&ed_bytes).expect("Invalid verifying key");
        let sig = ed25519_dalek::Signature::from_bytes(&clean_sig);
        assert!(verifying_key.verify(msg, &sig).is_ok());
    }
}
