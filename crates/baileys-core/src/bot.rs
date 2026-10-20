//! WhatsApp Meta AI and in-chat bot invocation protocol.
//!
//! Provides bot identities, Bloks bundle versioning hash, and capability flags.
//! Meta AI requires a valid Bloks version hash and rich response capability metadata;
//! otherwise, prompt mentions are silently dropped by WhatsApp servers.

pub const META_AI_PN: &str = "13135550002@s.whatsapp.net";
pub const META_AI_FBID: &str = "867051314767696@bot";
pub const META_AI_TEE_FBID: &str = "1273596044787272@bot";
pub const META_AI_SIDECHAT_FBID: &str = "867051314767696555@bot";

/// Official Bloks versioning hash required to prevent Meta AI prompt drops.
pub const WA_BLOKS_VERSIONING_ID: &str =
    "98153664e5d904059a94ea08a59fc7eeaf977544cec64ab3ab793c0ce001eb92";

pub const WA_BOT_MSG_SECRET_BYTES: usize = 32;

/// Checks whether a JID belongs to a Meta AI or third-party WhatsApp bot.
pub fn is_bot_jid(jid: &str) -> bool {
    jid.ends_with("@bot") || jid == META_AI_PN
}

/// Resolves a phone-number-based bot JID into its canonical FBID bot JID.
pub fn resolve_bot_fbid_jid(jid: &str) -> &str {
    if jid == META_AI_PN {
        META_AI_FBID
    } else {
        jid
    }
}

/// Rich response capability flags supported by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotCapability {
    Heading,
    NestedList,
    Table,
    Code,
    StructuredResponse,
    SubHeading,
    Latex,
    InlineReels,
}

impl BotCapability {
    pub fn all() -> &'static [BotCapability] {
        &[
            BotCapability::Heading,
            BotCapability::NestedList,
            BotCapability::Table,
            BotCapability::Code,
            BotCapability::StructuredResponse,
            BotCapability::SubHeading,
            BotCapability::Latex,
            BotCapability::InlineReels,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bot_jid_resolution() {
        assert!(is_bot_jid(META_AI_PN));
        assert!(is_bot_jid(META_AI_FBID));
        assert!(is_bot_jid("custom_bot@bot"));
        assert!(!is_bot_jid("62812345678@s.whatsapp.net"));

        assert_eq!(resolve_bot_fbid_jid(META_AI_PN), META_AI_FBID);
        assert_eq!(resolve_bot_fbid_jid(META_AI_FBID), META_AI_FBID);
    }

    #[test]
    fn test_bot_capabilities() {
        assert_eq!(BotCapability::all().len(), 8);
    }
}
