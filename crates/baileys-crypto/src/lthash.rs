use crate::kdf::hkdf_sha256;
use crate::CryptoError;

pub const LTHASH_SIZE: usize = 128;
pub const LTHASH_WORDS: usize = LTHASH_SIZE / 2; // 64 u16 words
pub const LTHASH_SALT: &[u8] = b"WhatsApp Patch Integrity";

/// WhatsApp LTHash (Lightweight Tamper-Resistant Hash) anti-tampering verification algorithm.
/// Operates on 128-byte hash buffers treated as 64 16-bit little-endian integers under modular arithmetic.
pub struct LTHashAntiTampering;

impl LTHashAntiTampering {
    /// Derives the 128-byte pointwise mutation mask for a given value MAC using HKDF-SHA256.
    pub fn derive_mask(value_mac: &[u8]) -> Result<[u16; LTHASH_WORDS], CryptoError> {
        let derived_bytes = hkdf_sha256(value_mac, LTHASH_SIZE, None, Some(LTHASH_SALT))?;
        let mut words = [0u16; LTHASH_WORDS];
        for i in 0..LTHASH_WORDS {
            words[i] = u16::from_le_bytes([derived_bytes[i * 2], derived_bytes[i * 2 + 1]]);
        }
        Ok(words)
    }

    /// Performs `subtract` followed by `add` on the base 128-byte hash using the provided MAC lists.
    /// Matches `LT_HASH_ANTI_TAMPERING.subtractThenAdd(base, subtract, add)`.
    pub fn subtract_then_add(
        base: &[u8],
        subtract: &[&[u8]],
        add: &[&[u8]],
    ) -> Result<Vec<u8>, CryptoError> {
        if base.len() != LTHASH_SIZE {
            return Err(CryptoError::InvalidLength(format!(
                "LTHash base buffer must be 128 bytes, got {}",
                base.len()
            )));
        }

        let mut current_words = [0u16; LTHASH_WORDS];
        for i in 0..LTHASH_WORDS {
            current_words[i] = u16::from_le_bytes([base[i * 2], base[i * 2 + 1]]);
        }

        // 1. Subtract items
        for sub_item in subtract {
            let mask = Self::derive_mask(sub_item)?;
            for i in 0..LTHASH_WORDS {
                current_words[i] = current_words[i].wrapping_sub(mask[i]);
            }
        }

        // 2. Add items
        for add_item in add {
            let mask = Self::derive_mask(add_item)?;
            for i in 0..LTHASH_WORDS {
                current_words[i] = current_words[i].wrapping_add(mask[i]);
            }
        }

        let mut out = vec![0u8; LTHASH_SIZE];
        for i in 0..LTHASH_WORDS {
            let bytes = current_words[i].to_le_bytes();
            out[i * 2] = bytes[0];
            out[i * 2 + 1] = bytes[1];
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lthash_add_subtract_inverse() {
        let base = vec![0u8; LTHASH_SIZE];
        let mac1 = b"mutation-mac-1-payload";
        let mac2 = b"mutation-mac-2-payload";

        // Add mac1, mac2
        let after_add = LTHashAntiTampering::subtract_then_add(&base, &[], &[mac1, mac2]).unwrap();
        assert_ne!(after_add, base);

        // Subtract mac1, mac2 -> should return exactly back to base!
        let after_sub =
            LTHashAntiTampering::subtract_then_add(&after_add, &[mac1, mac2], &[]).unwrap();
        assert_eq!(after_sub, base);
    }
}
