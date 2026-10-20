//! WhatsApp username validation, handle normalization, and PIN recovery parsing.
//!
//! Enforces WhatsApp Web handle constraints:
//! - Length: 3 to 35 characters
//! - Allowed characters: ASCII alphanumeric, underscores, and non-consecutive dots
//! - Restrictions: Must contain at least one letter; cannot start or end with a dot;
//!   cannot start with `www.`; cannot end with top-level/file domain extensions;
//!   cannot contain reserved Meta trademarks.

use thiserror::Error;

pub const MIN_USERNAME_LENGTH: usize = 3;
pub const MAX_USERNAME_LENGTH: usize = 35;
pub const USERNAME_KEY_LENGTH: usize = 4;

const DOMAIN_SUFFIXES: &[&str] = &[
    ".com", ".org", ".net", ".int", ".edu", ".gov", ".mil", ".arpa", ".html", ".htm", ".txt",
    ".xml",
];

const RESERVED_WORDS: &[&str] = &["whatsapp", "instagram", "facebook", "oculus"];

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum UsernameError {
    #[error("Username contains invalid characters (only a-z, 0-9, '_', '.' are permitted)")]
    InvalidCharacter,

    #[error("Username length must be between 3 and 35 characters")]
    InvalidLength,

    #[error("Username must contain at least one letter")]
    NoLetters,

    #[error("Username cannot start/end with a dot or contain consecutive dots ('..')")]
    InvalidPeriods,

    #[error("Username cannot start with 'www.'")]
    InvalidWwwPrefix,

    #[error("Username cannot end with a domain suffix")]
    InvalidDomainSuffix,

    #[error("Username cannot contain reserved brand terms")]
    ReservedWord,

    #[error("Username lookup key must be exactly 4 digits")]
    InvalidKey,
}

/// Validates a candidate username against WhatsApp Web handle rules.
pub fn validate_username(username: &str) -> Result<(), UsernameError> {
    let clean = normalize_username(username);

    for ch in clean.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '.' {
            return Err(UsernameError::InvalidCharacter);
        }
    }

    if clean.len() < MIN_USERNAME_LENGTH || clean.len() > MAX_USERNAME_LENGTH {
        return Err(UsernameError::InvalidLength);
    }

    if !clean.chars().any(|c| c.is_ascii_alphabetic()) {
        return Err(UsernameError::NoLetters);
    }

    if clean.starts_with('.') || clean.ends_with('.') || clean.contains("..") {
        return Err(UsernameError::InvalidPeriods);
    }

    let lowered = clean.to_ascii_lowercase();
    if lowered.starts_with("www.") {
        return Err(UsernameError::InvalidWwwPrefix);
    }

    for suffix in DOMAIN_SUFFIXES {
        if lowered.ends_with(suffix) {
            return Err(UsernameError::InvalidDomainSuffix);
        }
    }

    for reserved in RESERVED_WORDS {
        if lowered.contains(reserved) {
            return Err(UsernameError::ReservedWord);
        }
    }

    Ok(())
}

/// Returns true if the username satisfies all WhatsApp Web handle rules.
pub fn is_valid_username(username: &str) -> bool {
    validate_username(username).is_ok()
}

/// Normalizes a username by stripping any leading `@` and trimming whitespace.
pub fn normalize_username(username: &str) -> String {
    let trimmed = username.trim();
    if let Some(stripped) = trimmed.strip_prefix('@') {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Checks whether a 4-digit PIN key consists entirely of ASCII digits.
pub fn is_valid_username_key(key: &str) -> bool {
    key.len() == USERNAME_KEY_LENGTH && key.chars().all(|c| c.is_ascii_digit())
}

/// Splits an input string (`@user`, `user`, or `@user:1234`) into handle and optional 4-digit PIN.
pub fn split_username_handle(input: &str) -> (String, Option<String>) {
    let normalized = normalize_username(input);
    if let Some((user, pin)) = normalized.split_once(':') {
        (user.to_string(), Some(pin.to_string()))
    } else {
        (normalized, None)
    }
}

/// Resolved account identity resulting from a USync username lookup query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedUsername {
    pub username: String,
    pub jid: String,
    pub phone_jid: Option<String>,
    pub verified_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_usernames() {
        assert!(is_valid_username("alice"));
        assert!(is_valid_username("@alice_99"));
        assert!(is_valid_username("bob.smith_2026"));
        assert!(is_valid_username("user_one"));
    }

    #[test]
    fn test_invalid_usernames() {
        assert_eq!(validate_username("ab"), Err(UsernameError::InvalidLength));
        assert_eq!(validate_username("12345"), Err(UsernameError::NoLetters));
        assert_eq!(
            validate_username(".alice"),
            Err(UsernameError::InvalidPeriods)
        );
        assert_eq!(
            validate_username("alice."),
            Err(UsernameError::InvalidPeriods)
        );
        assert_eq!(
            validate_username("alice..smith"),
            Err(UsernameError::InvalidPeriods)
        );
        assert_eq!(
            validate_username("www.alice"),
            Err(UsernameError::InvalidWwwPrefix)
        );
        assert_eq!(
            validate_username("alice.com"),
            Err(UsernameError::InvalidDomainSuffix)
        );
        assert_eq!(
            validate_username("official_whatsapp_user"),
            Err(UsernameError::ReservedWord)
        );
        assert_eq!(
            validate_username("alice$"),
            Err(UsernameError::InvalidCharacter)
        );
    }

    #[test]
    fn test_split_and_key_validation() {
        let (u1, k1) = split_username_handle("@alice:1234");
        assert_eq!(u1, "alice");
        assert_eq!(k1, Some("1234".to_string()));
        assert!(is_valid_username_key("1234"));
        assert!(!is_valid_username_key("12a4"));
        assert!(!is_valid_username_key("123"));

        let (u2, k2) = split_username_handle("bob");
        assert_eq!(u2, "bob");
        assert_eq!(k2, None);
    }
}
