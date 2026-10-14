use serde::{Deserialize, Serialize};

pub const S_WHATSAPP_NET: &str = "s.whatsapp.net";
pub const OFFICIAL_BIZ_JID: &str = "16505361212@c.us";
pub const SERVER_JID: &str = "server@c.us";
pub const PSA_WID: &str = "0@c.us";
pub const STORIES_JID: &str = "status@broadcast";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum WaJidDomain {
    WhatsApp = 0,
    Lid = 1,
    Hosted = 128,
    HostedLid = 129,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FullJid {
    pub user: String,
    pub server: String,
    pub device: Option<u8>,
    pub domain_type: Option<u8>,
}

impl std::fmt::Display for FullJid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            jid_encode(&self.user, &self.server, self.device, self.domain_type)
        )
    }
}

/// A strongly-typed WhatsApp JID (Jabber ID) domain identifier.
///
/// Wraps the underlying canonical JID string while providing zero-allocation
/// inspection, classification, and conversion utilities.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Jid(String);

impl Jid {
    /// Creates a new `Jid` from any string-like value without validation.
    #[inline]
    pub fn new<S: Into<String>>(jid: S) -> Self {
        Self(jid.into())
    }

    /// Borrows the underlying JID string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the `Jid` and returns the inner `String`.
    #[inline]
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Returns `true` if this JID represents an individual user (`@s.whatsapp.net`).
    #[inline]
    pub fn is_user(&self) -> bool {
        is_jid_user(&self.0)
    }

    /// Returns `true` if this JID represents a WhatsApp Group (`@g.us`).
    #[inline]
    pub fn is_group(&self) -> bool {
        is_jid_group(&self.0)
    }

    /// Returns `true` if this JID represents a WhatsApp Channel / Newsletter (`@newsletter`).
    #[inline]
    pub fn is_newsletter(&self) -> bool {
        is_jid_newsletter(&self.0)
    }

    /// Returns `true` if this JID represents a broadcast list (`@broadcast`).
    #[inline]
    pub fn is_broadcast(&self) -> bool {
        is_jid_broadcast(&self.0)
    }

    /// Returns `true` if this JID represents the WhatsApp Status broadcast (`status@broadcast`).
    #[inline]
    pub fn is_status(&self) -> bool {
        self.0 == STORIES_JID
    }

    /// Returns `true` if this JID represents a WhatsApp LID (`@lid`).
    #[inline]
    pub fn is_lid(&self) -> bool {
        is_lid_user(&self.0)
    }

    /// Returns the user/phone component of the JID before the `@` symbol,
    /// excluding device or agent suffixes.
    pub fn user(&self) -> &str {
        let user_part = match self.0.find('@') {
            Some(idx) => &self.0[..idx],
            None => &self.0,
        };
        let without_device = match user_part.find(':') {
            Some(idx) => &user_part[..idx],
            None => user_part,
        };
        match without_device.find('_') {
            Some(idx) => &without_device[..idx],
            None => without_device,
        }
    }

    /// Returns the server/domain component after the `@` symbol.
    pub fn server(&self) -> &str {
        match self.0.find('@') {
            Some(idx) => &self.0[idx + 1..],
            None => "",
        }
    }

    /// Decodes into a structured `FullJid`.
    #[inline]
    pub fn decode(&self) -> Option<FullJid> {
        jid_decode(&self.0)
    }

    /// Normalizes the JID into a base `user@server` form (removing device/agent suffixes).
    #[inline]
    pub fn normalized_user(&self) -> String {
        jid_normalized_user(&self.0)
    }
}

impl std::ops::Deref for Jid {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Jid {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for Jid {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Jid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for Jid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Jid(\"{}\")", self.0)
    }
}

impl From<&str> for Jid {
    #[inline]
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for Jid {
    #[inline]
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&String> for Jid {
    #[inline]
    fn from(s: &String) -> Self {
        Self(s.clone())
    }
}

impl From<FullJid> for Jid {
    #[inline]
    fn from(f: FullJid) -> Self {
        Self(f.to_string())
    }
}

impl From<Jid> for String {
    #[inline]
    fn from(j: Jid) -> Self {
        j.0
    }
}

impl PartialEq<str> for Jid {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Jid {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for Jid {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

/// Encodes a JID from its constituent parts.
pub fn jid_encode(user: &str, server: &str, device: Option<u8>, agent: Option<u8>) -> String {
    let mut out = String::with_capacity(user.len() + server.len() + 8);
    out.push_str(user);
    if let Some(a) = agent {
        if a > 0 {
            out.push('_');
            out.push_str(&a.to_string());
        }
    }
    if let Some(d) = device {
        out.push(':');
        out.push_str(&d.to_string());
    }
    out.push('@');
    out.push_str(server);
    out
}

/// Decodes a string JID into a `FullJid`.
pub fn jid_decode(jid: &str) -> Option<FullJid> {
    let sep_idx = jid.find('@')?;
    let (user_part, server_part) = jid.split_at(sep_idx);
    let server = &server_part[1..];

    let (user_agent, device_str) = match user_part.find(':') {
        Some(pos) => (&user_part[..pos], Some(&user_part[pos + 1..])),
        None => (user_part, None),
    };

    let (user, agent_str) = match user_agent.find('_') {
        Some(pos) => (&user_agent[..pos], Some(&user_agent[pos + 1..])),
        None => (user_agent, None),
    };

    let device = device_str.and_then(|d| d.parse::<u8>().ok());
    let domain_type = if server == "lid" {
        Some(WaJidDomain::Lid as u8)
    } else if server == "hosted" {
        Some(WaJidDomain::Hosted as u8)
    } else if server == "hosted.lid" {
        Some(WaJidDomain::HostedLid as u8)
    } else if let Some(a) = agent_str {
        a.parse::<u8>().ok()
    } else {
        Some(WaJidDomain::WhatsApp as u8)
    };

    Some(FullJid {
        user: user.to_string(),
        server: server.to_string(),
        device,
        domain_type,
    })
}

pub fn is_jid_user(jid: &str) -> bool {
    jid.ends_with("@s.whatsapp.net")
}

pub fn is_jid_group(jid: &str) -> bool {
    jid.ends_with("@g.us")
}

pub fn is_lid_user(jid: &str) -> bool {
    jid.ends_with("@lid")
}

pub fn is_jid_broadcast(jid: &str) -> bool {
    jid.ends_with("@broadcast")
}

pub fn is_jid_newsletter(jid: &str) -> bool {
    jid.ends_with("@newsletter")
}

pub fn are_jids_same_user(jid1: &str, jid2: &str) -> bool {
    match (jid_decode(jid1), jid_decode(jid2)) {
        (Some(a), Some(b)) => a.user == b.user,
        _ => false,
    }
}

pub fn jid_normalized_user(jid: &str) -> String {
    match jid_decode(jid) {
        Some(full) => format!("{}@{}", full.user, full.server),
        None => jid.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jid_encode_decode() {
        let jid = "123456789:2@s.whatsapp.net";
        let decoded = jid_decode(jid).unwrap();
        assert_eq!(decoded.user, "123456789");
        assert_eq!(decoded.server, "s.whatsapp.net");
        assert_eq!(decoded.device, Some(2));

        let re_encoded = decoded.to_string();
        assert_eq!(re_encoded, jid);

        assert!(is_jid_user(jid));
        assert!(!is_jid_group(jid));
        assert_eq!(jid_normalized_user(jid), "123456789@s.whatsapp.net");
    }

    #[test]
    fn test_group_and_lid() {
        let group = "12036302@g.us";
        assert!(is_jid_group(group));

        let lid = "987654321:0@lid";
        assert!(is_lid_user(lid));
    }

    #[test]
    fn test_jid_struct_behavior() {
        let jid = Jid::new("628123456789:1@s.whatsapp.net");
        assert!(jid.is_user());
        assert!(!jid.is_group());
        assert!(!jid.is_newsletter());
        assert_eq!(jid.user(), "628123456789");
        assert_eq!(jid.server(), "s.whatsapp.net");
        assert_eq!(jid.as_str(), "628123456789:1@s.whatsapp.net");
        assert_eq!(jid, "628123456789:1@s.whatsapp.net");
        assert_eq!(jid.to_string(), "628123456789:1@s.whatsapp.net");

        let group_jid = Jid::from("120363415495325069@g.us");
        assert!(group_jid.is_group());
        assert!(!group_jid.is_user());
        assert_eq!(group_jid.user(), "120363415495325069");
        assert_eq!(group_jid.server(), "g.us");

        let news_jid = Jid::from("120363415495325069@newsletter");
        assert!(news_jid.is_newsletter());
        assert!(!news_jid.is_group());

        let status_jid = Jid::from("status@broadcast");
        assert!(status_jid.is_status());
        assert!(status_jid.is_broadcast());

        let lid_jid = Jid::from("987654321@lid");
        assert!(lid_jid.is_lid());
    }
}
