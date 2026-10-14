#![allow(clippy::chunks_exact_to_as_chunks, clippy::manual_is_multiple_of)]

//! Cryptographic primitives for WhatsApp Web Protocol.
//!
//! Provides pure-Rust implementations of:
//! - AES-256-GCM and AES-256-CBC
//! - Curve25519 (X25519) and Ed25519
//! - HKDF, HMAC, SHA256, MD5, and PBKDF2 pairing code derivation
//! - WhatsApp LTHash160 anti-tampering verification

pub mod aes;
pub mod curve;
pub mod kdf;
pub mod lthash;

pub use aes::*;
pub use curve::*;
pub use kdf::*;
pub use lthash::*;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    #[error("Invalid key length: expected {1}, got {0}")]
    InvalidKeyLength(usize, usize),

    #[error("Invalid IV/nonce length: expected {1}, got {0}")]
    InvalidIvLength(usize, usize),

    #[error("Invalid signature length: got {0}, expected 64")]
    InvalidSignatureLength(usize),

    #[error("Invalid buffer length: {0}")]
    InvalidLength(String),

    #[error("Invalid public key format: {0}")]
    InvalidPublicKey(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Key derivation failed: {0}")]
    KdfFailed(String),
}
