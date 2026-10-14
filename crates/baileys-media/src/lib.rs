//! WhatsApp Media Encryption, Decryption, and Streaming Engine.
//!
//! Implements WhatsApp media standards:
//! - HKDF media key derivation (112 bytes: IV, Cipher Key, Mac Key, Ref Key)
//! - AES-256-CBC decryption with 10-byte truncated HMAC-SHA256 authentication
//! - CDN direct_path streaming download & upload pipeline

use baileys_crypto::{
    aes_256_cbc_decrypt, aes_256_cbc_encrypt, aes_256_gcm_decrypt, aes_256_gcm_encrypt,
    hkdf_sha256, hmac_sha256, sha256,
};
use baileys_proto::proto::{MediaRetryNotification, Message, ServerErrorReceipt};
use prost::Message as ProstMessage;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;
use tracing::debug;

pub const DEF_MEDIA_HOST: &str = "mmg.whatsapp.net";
pub const DEFAULT_ORIGIN: &str = "https://web.whatsapp.com";
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

pub mod preview;
pub use preview::*;

pub mod dimensions;
pub use dimensions::*;

pub mod streaming;
pub use streaming::*;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    #[error("Invalid media key length: expected 32, got {0}")]
    InvalidMediaKeyLength(usize),

    #[error("Encrypted payload too short (minimum 10 bytes MAC required)")]
    PayloadTooShort,

    #[error("Media HMAC-SHA256 signature verification failed")]
    InvalidMacSignature,

    #[error("Media decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Media encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("HTTP download failed: {0}")]
    HttpError(String),

    #[error("No media content found in Message protobuf")]
    NoMediaInMessage,

    #[error("Missing direct_path or url in media metadata")]
    MissingUrl,

    #[error("File I/O error: {0}")]
    IoError(String),

    #[error("Crypto error: {0}")]
    CryptoError(String),

    #[error("Protobuf error: {0}")]
    ProtobufError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Document,
    Sticker,
    History,
}

impl MediaType {
    pub fn hkdf_info(&self) -> &'static [u8] {
        match self {
            MediaType::Image => b"WhatsApp Image Keys",
            MediaType::Video => b"WhatsApp Video Keys",
            MediaType::Audio => b"WhatsApp Audio Keys",
            MediaType::Document => b"WhatsApp Document Keys",
            MediaType::Sticker => b"WhatsApp Image Keys",
            MediaType::History => b"WhatsApp History Keys",
        }
    }

    pub fn default_extension(&self, mimetype: Option<&str>) -> &'static str {
        if let Some(mime) = mimetype {
            let lower = mime.to_lowercase();
            if lower.contains("jpeg") || lower.contains("jpg") {
                return "jpg";
            } else if lower.contains("png") {
                return "png";
            } else if lower.contains("webp") {
                return "webp";
            } else if lower.contains("gif") {
                return "gif";
            } else if lower.contains("svg") {
                return "svg";
            } else if lower.contains("ogg") || lower.contains("opus") {
                return "ogg";
            } else if lower.contains("mpeg") || lower.contains("mp3") {
                return "mp3";
            } else if lower.contains("aac") {
                return "aac";
            } else if lower.contains("m4a") {
                return "m4a";
            } else if lower.contains("wav") {
                return "wav";
            } else if lower.contains("amr") {
                return "amr";
            } else if lower.contains("mp4") {
                return "mp4";
            } else if lower.contains("webm") {
                return "webm";
            } else if lower.contains("3gp") || lower.contains("3gpp") {
                return "3gp";
            } else if lower.contains("quicktime") || lower.contains("mov") {
                return "mov";
            } else if lower.contains("x-msvideo") || lower.contains("avi") {
                return "avi";
            } else if lower.contains("x-matroska") || lower.contains("mkv") {
                return "mkv";
            } else if lower.contains("pdf") {
                return "pdf";
            } else if lower.contains("wordprocessingml") || lower.contains("docx") {
                return "docx";
            } else if lower.contains("msword") || lower.contains("doc") {
                return "doc";
            } else if lower.contains("spreadsheetml") || lower.contains("xlsx") {
                return "xlsx";
            } else if lower.contains("ms-excel") || lower.contains("xls") {
                return "xls";
            } else if lower.contains("presentationml") || lower.contains("pptx") {
                return "pptx";
            } else if lower.contains("ms-powerpoint") || lower.contains("ppt") {
                return "ppt";
            } else if lower.contains("json") {
                return "json";
            } else if lower.contains("zip") {
                return "zip";
            } else if lower.contains("rar") {
                return "rar";
            } else if lower.contains("7z") {
                return "7z";
            } else if lower.contains("tar") {
                return "tar";
            } else if lower.contains("gzip") || lower.contains("gz") {
                return "gz";
            } else if lower.contains("android.package-archive") || lower.contains("apk") {
                return "apk";
            } else if lower.contains("text/plain") || lower.contains("txt") {
                return "txt";
            }
        }
        match self {
            MediaType::Image => "jpg",
            MediaType::Video => "mp4",
            MediaType::Audio => "ogg",
            MediaType::Document => "bin",
            MediaType::Sticker => "webp",
            MediaType::History => "gz",
        }
    }

    pub fn upload_path(&self) -> &'static str {
        match self {
            MediaType::Image => "/mms/image",
            MediaType::Video => "/mms/video",
            MediaType::Audio => "/mms/audio",
            MediaType::Document => "/mms/document",
            MediaType::Sticker => "/mms/image",
            MediaType::History => "/mms/md-app-state",
        }
    }

    pub fn newsletter_upload_path(&self) -> &'static str {
        match self {
            MediaType::Image => "/newsletter/newsletter-image",
            MediaType::Video => "/newsletter/newsletter-video",
            MediaType::Audio => "/newsletter/newsletter-audio",
            MediaType::Document => "/newsletter/newsletter-document",
            MediaType::Sticker => "/newsletter/newsletter-image",
            MediaType::History => "/newsletter/newsletter-document",
        }
    }
}

impl std::str::FromStr for MediaType {
    type Err = MediaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "image" | "img" | "photo" | "picture" => Ok(MediaType::Image),
            "video" | "vid" => Ok(MediaType::Video),
            "audio" | "voice" | "ptt" => Ok(MediaType::Audio),
            "document" | "doc" | "file" => Ok(MediaType::Document),
            "sticker" => Ok(MediaType::Sticker),
            "history" => Ok(MediaType::History),
            _ => Err(MediaError::ProtobufError(format!(
                "Unknown media type: '{}'",
                s
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaKeys {
    pub iv: [u8; 16],
    pub cipher_key: [u8; 32],
    pub mac_key: [u8; 32],
    pub ref_key: [u8; 32],
}

/// Derives the 112-byte expanded media key bundle (IV, cipher key, MAC key, Ref key) via HKDF-SHA256.
pub fn derive_media_keys(media_key: &[u8], media_type: MediaType) -> Result<MediaKeys, MediaError> {
    if media_key.len() != 32 {
        return Err(MediaError::InvalidMediaKeyLength(media_key.len()));
    }

    let expanded = hkdf_sha256(media_key, 112, None, Some(media_type.hkdf_info()))
        .map_err(|e| MediaError::DecryptionFailed(e.to_string()))?;

    let mut iv = [0u8; 16];
    let mut cipher_key = [0u8; 32];
    let mut mac_key = [0u8; 32];
    let mut ref_key = [0u8; 32];

    iv.copy_from_slice(&expanded[0..16]);
    cipher_key.copy_from_slice(&expanded[16..48]);
    mac_key.copy_from_slice(&expanded[48..80]);
    ref_key.copy_from_slice(&expanded[80..112]);

    Ok(MediaKeys {
        iv,
        cipher_key,
        mac_key,
        ref_key,
    })
}

/// Decrypts downloaded WhatsApp media bytes and verifies the 10-byte truncated HMAC tag.
/// If no 10-byte MAC tag is present, decrypts block-aligned ciphertext directly.
pub fn decrypt_media_bytes(
    encrypted: &[u8],
    media_key: &[u8],
    media_type: MediaType,
) -> Result<Vec<u8>, MediaError> {
    if encrypted.len() < 16 {
        return Err(MediaError::PayloadTooShort);
    }

    let keys = derive_media_keys(media_key, media_type)?;

    // Standard WhatsApp media: [Ciphertext (N*16 bytes)] + [10 bytes MAC]
    if encrypted.len() > 10 && (encrypted.len() - 10).is_multiple_of(16) {
        let proto_len = encrypted.len() - 10;
        let ciphertext = &encrypted[..proto_len];
        let received_mac = &encrypted[proto_len..];

        let mut mac_input = Vec::with_capacity(16 + ciphertext.len());
        mac_input.extend_from_slice(&keys.iv);
        mac_input.extend_from_slice(ciphertext);

        let full_mac = hmac_sha256(&mac_input, &keys.mac_key)
            .map_err(|e| MediaError::DecryptionFailed(e.to_string()))?;

        if subtle::ConstantTimeEq::ct_eq(&full_mac[..10], received_mac).unwrap_u8() != 1 {
            return Err(MediaError::InvalidMacSignature);
        }

        let plaintext = aes_256_cbc_decrypt(ciphertext, &keys.cipher_key, &keys.iv)
            .map_err(|e| MediaError::DecryptionFailed(e.to_string()))?;

        Ok(plaintext)
    } else {
        // Direct decryption for block-aligned raw ciphertext without trailing MAC
        let block_len = (encrypted.len() / 16) * 16;
        let ciphertext = &encrypted[..block_len];
        aes_256_cbc_decrypt(ciphertext, &keys.cipher_key, &keys.iv)
            .map_err(|e| MediaError::DecryptionFailed(e.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct EncryptedMedia {
    pub encrypted_bytes: Vec<u8>,
    pub media_key: [u8; 32],
    pub file_sha256: [u8; 32],
    pub file_enc_sha256: [u8; 32],
}

/// Encrypts media bytes using AES-256-CBC with WhatsApp's 10-byte HMAC tag.
pub fn encrypt_media_bytes(
    plaintext: &[u8],
    media_key_opt: Option<&[u8]>,
    media_type: MediaType,
) -> Result<EncryptedMedia, MediaError> {
    let mut media_key = [0u8; 32];
    if let Some(mk) = media_key_opt {
        if mk.len() != 32 {
            return Err(MediaError::InvalidMediaKeyLength(mk.len()));
        }
        media_key.copy_from_slice(mk);
    } else {
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut media_key);
    }

    let keys = derive_media_keys(&media_key, media_type)?;

    let ciphertext = aes_256_cbc_encrypt(plaintext, &keys.cipher_key, &keys.iv)
        .map_err(|e| MediaError::EncryptionFailed(e.to_string()))?;

    let mut mac_input = Vec::with_capacity(16 + ciphertext.len());
    mac_input.extend_from_slice(&keys.iv);
    mac_input.extend_from_slice(&ciphertext);

    let full_mac = hmac_sha256(&mac_input, &keys.mac_key)
        .map_err(|e| MediaError::EncryptionFailed(e.to_string()))?;

    let mut encrypted_bytes = Vec::with_capacity(ciphertext.len() + 10);
    encrypted_bytes.extend_from_slice(&ciphertext);
    encrypted_bytes.extend_from_slice(&full_mac[..10]);

    let file_sha256 = sha256(plaintext);
    let mut enc_hasher = Sha256::new();
    enc_hasher.update(&encrypted_bytes);
    let file_enc_sha256: [u8; 32] = enc_hasher.finalize().into();

    Ok(EncryptedMedia {
        encrypted_bytes,
        media_key,
        file_sha256,
        file_enc_sha256,
    })
}

#[derive(Debug, Clone)]
pub struct MediaAttachment {
    pub media_type: MediaType,
    pub direct_path: Option<String>,
    pub url: Option<String>,
    pub media_key: Vec<u8>,
    pub mimetype: Option<String>,
    pub file_length: Option<u64>,
    pub file_sha256: Option<Vec<u8>>,
    pub caption: Option<String>,
    pub filename: Option<String>,
    pub is_view_once: bool,
}

impl MediaAttachment {
    pub fn download_url(&self) -> Result<String, MediaError> {
        let custom_host = self.url.as_deref().and_then(|u| {
            if let Some(rest) = u
                .strip_prefix("https://")
                .or_else(|| u.strip_prefix("http://"))
            {
                let host = rest.split('/').next().unwrap_or(rest);
                let host_clean = host.split(':').next().unwrap_or(host);
                if !host_clean.is_empty()
                    && host_clean != "a.whatsapp.net"
                    && !host_clean.starts_with("a.")
                    && (host_clean.contains("whatsapp.net")
                        || host_clean.contains("fbcdn.net")
                        || host_clean.contains("fbsbx.com"))
                {
                    Some(host_clean)
                } else {
                    None
                }
            } else {
                None
            }
        });

        let host = custom_host.unwrap_or(DEF_MEDIA_HOST);

        if let Some(ref dp) = self.direct_path {
            if dp.starts_with('/') {
                Ok(format!("https://{}{}", host, dp))
            } else {
                Ok(format!("https://{}/{}", host, dp))
            }
        } else if let Some(ref u) = self.url {
            Ok(u.clone())
        } else {
            Err(MediaError::MissingUrl)
        }
    }
}

/// Recursively unwraps container envelopes (ephemeral, view-once, document-with-caption, edits, etc.).
/// Returns a tuple of (inner_message_ref, is_view_once).
pub fn unwrap_message(mut msg: &Message) -> (&Message, bool) {
    let mut is_view_once = false;
    loop {
        if let Some(ref eph) = msg.ephemeral_message {
            if let Some(ref inner) = eph.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref vo) = msg.view_once_message {
            if let Some(ref inner) = vo.message {
                is_view_once = true;
                msg = inner;
                continue;
            }
        }
        if let Some(ref vo2) = msg.view_once_message_v2 {
            if let Some(ref inner) = vo2.message {
                is_view_once = true;
                msg = inner;
                continue;
            }
        }
        if let Some(ref vo2e) = msg.view_once_message_v2_extension {
            if let Some(ref inner) = vo2e.message {
                is_view_once = true;
                msg = inner;
                continue;
            }
        }
        if let Some(ref dwc) = msg.document_with_caption_message {
            if let Some(ref inner) = dwc.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref edit) = msg.edited_message {
            if let Some(ref inner) = edit.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref dev) = msg.device_sent_message {
            if let Some(ref inner) = dev.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref bot) = msg.bot_invoke_message {
            if let Some(ref inner) = bot.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref gm) = msg.group_mentioned_message {
            if let Some(ref inner) = gm.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref lsm) = msg.lottie_sticker_message {
            if let Some(ref inner) = lsm.message {
                msg = inner;
                continue;
            }
        }
        break;
    }
    (msg, is_view_once)
}

/// Extracts media attachment metadata from a `baileys_proto::proto::Message`,
/// automatically unwrapping any container wrappers (ephemeral, view-once, document-with-caption).
pub fn extract_media_from_message(msg: &Message) -> Option<MediaAttachment> {
    let (effective_msg, is_view_once) = unwrap_message(msg);

    if let Some(ref img) = effective_msg.image_message {
        img.media_key.as_ref().map(|mk| MediaAttachment {
            media_type: MediaType::Image,
            direct_path: img.direct_path.clone(),
            url: img.url.clone(),
            media_key: mk.clone(),
            mimetype: img.mimetype.clone(),
            file_length: img.file_length,
            file_sha256: img.file_sha256.clone(),
            caption: img.caption.clone(),
            filename: None,
            is_view_once,
        })
    } else if let Some(vid) = effective_msg
        .video_message
        .as_ref()
        .or(effective_msg.ptv_message.as_ref())
    {
        vid.media_key.as_ref().map(|mk| MediaAttachment {
            media_type: MediaType::Video,
            direct_path: vid.direct_path.clone(),
            url: vid.url.clone(),
            media_key: mk.clone(),
            mimetype: vid.mimetype.clone(),
            file_length: vid.file_length,
            file_sha256: vid.file_sha256.clone(),
            caption: vid.caption.clone(),
            filename: None,
            is_view_once,
        })
    } else if let Some(ref aud) = effective_msg.audio_message {
        aud.media_key.as_ref().map(|mk| MediaAttachment {
            media_type: MediaType::Audio,
            direct_path: aud.direct_path.clone(),
            url: aud.url.clone(),
            media_key: mk.clone(),
            mimetype: aud.mimetype.clone(),
            file_length: aud.file_length,
            file_sha256: aud.file_sha256.clone(),
            caption: None,
            filename: None,
            is_view_once,
        })
    } else if let Some(ref doc) = effective_msg.document_message {
        doc.media_key.as_ref().map(|mk| MediaAttachment {
            media_type: MediaType::Document,
            direct_path: doc.direct_path.clone(),
            url: doc.url.clone(),
            media_key: mk.clone(),
            mimetype: doc.mimetype.clone(),
            file_length: doc.file_length,
            file_sha256: doc.file_sha256.clone(),
            caption: doc.caption.clone(),
            filename: doc.file_name.clone(),
            is_view_once,
        })
    } else if let Some(ref stk) = effective_msg.sticker_message {
        stk.media_key.as_ref().map(|mk| MediaAttachment {
            media_type: MediaType::Sticker,
            direct_path: stk.direct_path.clone(),
            url: stk.url.clone(),
            media_key: mk.clone(),
            mimetype: stk.mimetype.clone(),
            file_length: stk.file_length,
            file_sha256: stk.file_sha256.clone(),
            caption: None,
            filename: None,
            is_view_once,
        })
    } else {
        None
    }
}

/// Downloads and decrypts WhatsApp media content directly from WhatsApp CDN.
pub async fn download_and_decrypt_media(
    download_url: &str,
    media_key: &[u8],
    media_type: MediaType,
) -> Result<Vec<u8>, MediaError> {
    debug!("Downloading encrypted media from '{}'...", download_url);

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| MediaError::HttpError(e.to_string()))?;

    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(400 * (1 << attempt))).await;
        }

        let response = match client
            .get(download_url)
            .header("Origin", DEFAULT_ORIGIN)
            .header("User-Agent", DEFAULT_USER_AGENT)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("Failed to connect to media CDN: {}", e);
                continue;
            }
        };

        if response.status().is_success() {
            let encrypted_bytes = response.bytes().await.map_err(|e| {
                MediaError::HttpError(format!("Failed to download media bytes: {}", e))
            })?;

            return decrypt_media_bytes(&encrypted_bytes, media_key, media_type);
        } else {
            let status = response.status();
            last_err = format!("Server returned status {}", status);
            if status.as_u16() == 404 || status.as_u16() == 410 {
                break;
            }
        }
    }

    Err(MediaError::HttpError(last_err))
}

/// Downloads media from an attachment and saves it into the target directory.
/// Returns the saved absolute file path.
pub async fn download_media_to_dir(
    attachment: &MediaAttachment,
    dest_dir: &Path,
    file_prefix: &str,
) -> Result<std::path::PathBuf, MediaError> {
    tokio::fs::create_dir_all(dest_dir)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;

    let download_url = attachment.download_url()?;
    let plaintext = match download_and_decrypt_media(
        &download_url,
        &attachment.media_key,
        attachment.media_type,
    )
    .await
    {
        Ok(data) => data,
        Err(err) => {
            if let Some(ref dp) = attachment.direct_path {
                let fallback_url = if dp.starts_with('/') {
                    format!("https://{}{}", DEF_MEDIA_HOST, dp)
                } else {
                    format!("https://{}/{}", DEF_MEDIA_HOST, dp)
                };
                if fallback_url != download_url {
                    download_and_decrypt_media(
                        &fallback_url,
                        &attachment.media_key,
                        attachment.media_type,
                    )
                    .await?
                } else {
                    return Err(err);
                }
            } else {
                return Err(err);
            }
        }
    };

    let ext = attachment
        .media_type
        .default_extension(attachment.mimetype.as_deref());
    let filename = if let Some(ref name) = attachment.filename {
        let clean = sanitize_filename(name);
        format!("{}_{}", file_prefix, clean)
    } else {
        format!("{}.{}", file_prefix, ext)
    };

    let target_path = dest_dir.join(filename);
    tokio::fs::write(&target_path, plaintext)
        .await
        .map_err(|e| MediaError::IoError(e.to_string()))?;

    Ok(target_path)
}

/// Sanitizes a filename so that it is universally valid across Windows, macOS, Linux, and Android/FAT32 filesystems.
pub fn sanitize_filename(name: &str) -> String {
    // Universal path split across both Windows and Unix delimiters
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);

    let sanitized: String = base
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    let trimmed = sanitized.trim_matches(|c| c == ' ' || c == '.');
    if trimmed.is_empty() {
        return "file".to_string();
    }

    // Windows reserved device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
    let stem = trimmed.split('.').next().unwrap_or(trimmed);
    let is_reserved = matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM0"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT0"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );

    if is_reserved {
        format!("_{}", trimmed)
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaConnHost {
    pub hostname: String,
    pub max_content_length_bytes: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaConnInfo {
    pub auth: String,
    pub ttl: u64,
    pub hosts: Vec<MediaConnHost>,
    pub fetched_at: std::time::SystemTime,
}

impl MediaConnInfo {
    pub fn is_expired(&self) -> bool {
        match self.fetched_at.elapsed() {
            Ok(dur) => dur.as_secs() >= self.ttl,
            Err(_) => true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaUploadResult {
    pub url: Option<String>,
    pub direct_path: Option<String>,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub object_id: Option<String>,
}

/// Standard RFC 3986 percent encoding for query string params (e.g. auth token).
pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

/// Uploads an already encrypted media payload to the WhatsApp media CDN.
pub async fn upload_encrypted_media_body(
    body: reqwest::Body,
    file_enc_sha256: &[u8; 32],
    media_type: MediaType,
    media_conn: &MediaConnInfo,
) -> Result<MediaUploadResult, MediaError> {
    use base64::prelude::*;
    let token_b64 = BASE64_URL_SAFE_NO_PAD.encode(file_enc_sha256);
    let hostname = media_conn
        .hosts
        .first()
        .map(|h| h.hostname.as_str())
        .unwrap_or(DEF_MEDIA_HOST);

    let auth_encoded = percent_encode(&media_conn.auth);
    let upload_url = format!(
        "https://{}{}/{}?auth={}&token={}",
        hostname,
        media_type.upload_path(),
        token_b64,
        auth_encoded,
        token_b64
    );

    debug!(
        "Uploading encrypted media to WhatsApp CDN: '{}'...",
        upload_url
    );

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| MediaError::HttpError(e.to_string()))?;

    let response = client
        .post(&upload_url)
        .header("Content-Type", "application/octet-stream")
        .header("Origin", DEFAULT_ORIGIN)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .body(body)
        .send()
        .await
        .map_err(|e| MediaError::HttpError(format!("Failed to connect to upload CDN: {}", e)))?;

    if !response.status().is_success() {
        return Err(MediaError::HttpError(format!(
            "Server returned upload status {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        MediaError::HttpError(format!("Failed to read upload response bytes: {}", e))
    })?;

    let result = serde_json::from_slice::<MediaUploadResult>(&bytes).map_err(|e| {
        MediaError::HttpError(format!("Failed to parse upload JSON response: {}", e))
    })?;

    Ok(result)
}

pub async fn upload_encrypted_media(
    encrypted_bytes: &[u8],
    file_enc_sha256: &[u8; 32],
    media_type: MediaType,
    media_conn: &MediaConnInfo,
) -> Result<MediaUploadResult, MediaError> {
    let body = reqwest::Body::from(bytes::Bytes::copy_from_slice(encrypted_bytes));
    upload_encrypted_media_body(body, file_enc_sha256, media_type, media_conn).await
}

/// Uploads an unencrypted media payload specifically for WhatsApp Newsletters / Channels.
/// Returns MediaUploadResult containing direct_path, url, and the server-assigned MediaHandle.
pub async fn upload_newsletter_media(
    raw_bytes: &[u8],
    media_type: MediaType,
    media_conn: &MediaConnInfo,
) -> Result<MediaUploadResult, MediaError> {
    use base64::prelude::*;
    let file_sha256 = sha256(raw_bytes);
    let token_b64 = BASE64_URL_SAFE_NO_PAD.encode(file_sha256);
    let hostname = media_conn
        .hosts
        .first()
        .map(|h| h.hostname.as_str())
        .unwrap_or(DEF_MEDIA_HOST);

    let auth_encoded = percent_encode(&media_conn.auth);
    let upload_url = format!(
        "https://{}{}/{}?auth={}&token={}",
        hostname,
        media_type.newsletter_upload_path(),
        token_b64,
        auth_encoded,
        token_b64
    );

    debug!(
        "Uploading raw media for WhatsApp Newsletter: '{}'...",
        upload_url
    );

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| MediaError::HttpError(e.to_string()))?;

    let response = client
        .post(&upload_url)
        .header("Content-Type", "application/octet-stream")
        .header("Origin", DEFAULT_ORIGIN)
        .header("Referer", format!("{}/", DEFAULT_ORIGIN))
        .header("User-Agent", DEFAULT_USER_AGENT)
        .body(raw_bytes.to_vec())
        .send()
        .await
        .map_err(|e| {
            MediaError::HttpError(format!("Failed to connect to newsletter upload CDN: {}", e))
        })?;

    if !response.status().is_success() {
        return Err(MediaError::HttpError(format!(
            "Server returned newsletter upload status {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        MediaError::HttpError(format!("Failed to read upload response bytes: {}", e))
    })?;

    let result = serde_json::from_slice::<MediaUploadResult>(&bytes).map_err(|e| {
        MediaError::HttpError(format!(
            "Failed to parse newsletter upload JSON response: {}",
            e
        ))
    })?;

    Ok(result)
}

/// Downloads, decrypts, and decompresses a WhatsApp HistorySync archive payload.
pub async fn download_history_sync(
    notification: &baileys_proto::proto::message::HistorySyncNotification,
) -> Result<baileys_proto::proto::HistorySync, MediaError> {
    use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
    use prost::Message as ProstMessage;
    use std::io::Read;

    // 1. Check if the notification carries an inline bootstrap payload directly
    let decompressed_bytes =
        if let Some(ref inline) = notification.initial_hist_bootstrap_inline_payload {
            let mut decompressed = Vec::new();
            let mut zlib_dec = ZlibDecoder::new(&inline[..]);
            if zlib_dec.read_to_end(&mut decompressed).is_ok() && !decompressed.is_empty() {
                decompressed
            } else {
                decompressed.clear();
                let mut gz_dec = GzDecoder::new(&inline[..]);
                if gz_dec.read_to_end(&mut decompressed).is_ok() && !decompressed.is_empty() {
                    decompressed
                } else {
                    decompressed.clear();
                    let mut def_dec = DeflateDecoder::new(&inline[..]);
                    if def_dec.read_to_end(&mut decompressed).is_ok() && !decompressed.is_empty() {
                        decompressed
                    } else {
                        inline.clone()
                    }
                }
            }
        } else {
            // 2. Otherwise download and decrypt external archive via direct_path and media_key
            let media_key = notification
                .media_key
                .as_ref()
                .ok_or(MediaError::MissingUrl)?;
            let direct_path = notification
                .direct_path
                .as_ref()
                .ok_or(MediaError::MissingUrl)?;

            let url = if direct_path.starts_with('/') {
                format!("https://{}{}", DEF_MEDIA_HOST, direct_path)
            } else {
                format!("https://{}/{}", DEF_MEDIA_HOST, direct_path)
            };

            let decrypted = download_and_decrypt_media(&url, media_key, MediaType::History).await?;

            // Try GzDecoder first (WhatsApp default for HistorySync), fallback to ZlibDecoder, then DeflateDecoder
            let mut decompressed = Vec::new();
            let mut gz_dec = GzDecoder::new(&decrypted[..]);
            if gz_dec.read_to_end(&mut decompressed).is_err() {
                decompressed.clear();
                let mut zlib_dec = ZlibDecoder::new(&decrypted[..]);
                if zlib_dec.read_to_end(&mut decompressed).is_err() {
                    decompressed.clear();
                    let mut def_dec = DeflateDecoder::new(&decrypted[..]);
                    def_dec.read_to_end(&mut decompressed).map_err(|e| {
                        MediaError::DecryptionFailed(format!(
                            "Failed to decompress history payload: {}",
                            e
                        ))
                    })?;
                }
            }
            decompressed
        };

    let history =
        baileys_proto::proto::HistorySync::decode(&decompressed_bytes[..]).map_err(|e| {
            MediaError::DecryptionFailed(format!("Failed to parse HistorySync protobuf: {}", e))
        })?;

    Ok(history)
}

/// Derives the 32-byte AES-GCM encryption key for Media Retry requests and notifications.
/// Matches Baileys `hkdf(mediaKey, 32, { info: 'WhatsApp Media Retry Notification' })`.
pub fn get_media_retry_key(media_key: &[u8]) -> Result<[u8; 32], MediaError> {
    if media_key.len() != 32 {
        return Err(MediaError::InvalidMediaKeyLength(media_key.len()));
    }
    let derived = hkdf_sha256(
        media_key,
        32,
        None,
        Some(b"WhatsApp Media Retry Notification"),
    )
    .map_err(|e| MediaError::CryptoError(e.to_string()))?;

    let mut key = [0u8; 32];
    key.copy_from_slice(&derived);
    Ok(key)
}

/// Encrypts a ServerErrorReceipt payload for requesting a media re-upload from the sender phone.
/// Returns `(ciphertext, iv)`.
pub fn encrypt_media_retry_request(
    msg_id: &str,
    media_key: &[u8],
) -> Result<(Vec<u8>, [u8; 12]), MediaError> {
    let receipt = ServerErrorReceipt {
        stanza_id: Some(msg_id.to_string()),
    };
    let mut recp_buf = Vec::new();
    receipt
        .encode(&mut recp_buf)
        .map_err(|e| MediaError::ProtobufError(e.to_string()))?;

    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);

    let retry_key = get_media_retry_key(media_key)?;
    let ciphertext = aes_256_gcm_encrypt(&recp_buf, &retry_key, &iv, msg_id.as_bytes())
        .map_err(|e| MediaError::CryptoError(e.to_string()))?;

    Ok((ciphertext, iv))
}

/// Decrypts an incoming MediaRetryNotification payload from the server / sender device.
pub fn decrypt_media_retry_notification(
    ciphertext: &[u8],
    iv: &[u8],
    media_key: &[u8],
    msg_id: &str,
) -> Result<MediaRetryNotification, MediaError> {
    let retry_key = get_media_retry_key(media_key)?;
    let plaintext = aes_256_gcm_decrypt(ciphertext, &retry_key, iv, msg_id.as_bytes())
        .map_err(|e| MediaError::CryptoError(e.to_string()))?;

    let notif = MediaRetryNotification::decode(&plaintext[..])
        .map_err(|e| MediaError::ProtobufError(e.to_string()))?;

    Ok(notif)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_encryption_and_decryption_roundtrip() {
        let sample_image = b"RIFF....WEBPVP8 ... Image Data Payload ...";
        let encrypted =
            encrypt_media_bytes(sample_image, None, MediaType::Image).expect("Encryption failed");

        assert_eq!(
            encrypted.encrypted_bytes.len(),
            sample_image.len() + 16 - (sample_image.len() % 16) + 10
        );

        let decrypted = decrypt_media_bytes(
            &encrypted.encrypted_bytes,
            &encrypted.media_key,
            MediaType::Image,
        )
        .expect("Decryption failed");

        assert_eq!(decrypted, sample_image);
    }

    #[test]
    fn test_media_corrupted_mac_fails() {
        let sample_audio = b"Opus Voice Note Sample Bytes 1234567890";
        let mut encrypted =
            encrypt_media_bytes(sample_audio, None, MediaType::Audio).expect("Encryption failed");

        // Corrupt MAC tag at the end
        let len = encrypted.encrypted_bytes.len();
        encrypted.encrypted_bytes[len - 1] ^= 0xFF;

        let res = decrypt_media_bytes(
            &encrypted.encrypted_bytes,
            &encrypted.media_key,
            MediaType::Audio,
        );
        assert_eq!(res, Err(MediaError::InvalidMacSignature));
    }

    #[test]
    fn test_percent_encode_and_media_conn() {
        assert_eq!(percent_encode("hello+world/123="), "hello%2Bworld%2F123%3D");

        let conn = MediaConnInfo {
            auth: "sample_token".to_string(),
            ttl: 3600,
            hosts: vec![MediaConnHost {
                hostname: "mmg.whatsapp.net".to_string(),
                max_content_length_bytes: Some(104857600),
            }],
            fetched_at: std::time::SystemTime::now(),
        };

        assert!(!conn.is_expired());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("valid_name.png"), "valid_name.png");
        assert_eq!(
            sanitize_filename("weird/path/file*name?.jpg"),
            "file_name_.jpg"
        );
        assert_eq!(
            sanitize_filename(r"weird\path\to:stream.png"),
            "to_stream.png"
        );
        assert_eq!(sanitize_filename("aux.txt"), "_aux.txt");
        assert_eq!(sanitize_filename("CON.PNG"), "_CON.PNG");
        assert_eq!(
            sanitize_filename("  test <image> | audio \"1\".mp3  "),
            "test _image_ _ audio _1_.mp3"
        );
        assert_eq!(sanitize_filename("..."), "file");
        assert_eq!(sanitize_filename(""), "file");
    }

    #[test]
    fn test_media_retry_encryption_decryption_roundtrip() {
        let media_key = [7u8; 32];
        let msg_id = "3EB0ABC123456789";

        let (ciphertext, iv) = encrypt_media_retry_request(msg_id, &media_key)
            .expect("Encrypt media retry request failed");

        assert!(!ciphertext.is_empty());
        assert_eq!(iv.len(), 12);

        // Also test decrypting a simulated MediaRetryNotification
        let notif_to_send = MediaRetryNotification {
            stanza_id: Some(msg_id.to_string()),
            direct_path: Some("/v/t62.7118-24/test_file.enc".to_string()),
            result: Some(
                baileys_proto::proto::media_retry_notification::ResultType::Success as i32,
            ),
            message_secret: None,
        };
        let mut notif_buf = Vec::new();
        notif_to_send.encode(&mut notif_buf).unwrap();

        let retry_key = get_media_retry_key(&media_key).unwrap();
        let notif_iv = [9u8; 12];
        let notif_ct =
            aes_256_gcm_encrypt(&notif_buf, &retry_key, &notif_iv, msg_id.as_bytes()).unwrap();

        let decrypted_notif =
            decrypt_media_retry_notification(&notif_ct, &notif_iv, &media_key, msg_id)
                .expect("Decrypt media retry notification failed");

        assert_eq!(decrypted_notif.stanza_id.as_deref(), Some(msg_id));
        assert_eq!(
            decrypted_notif.direct_path.as_deref(),
            Some("/v/t62.7118-24/test_file.enc")
        );
        assert_eq!(
            decrypted_notif.result,
            Some(baileys_proto::proto::media_retry_notification::ResultType::Success as i32)
        );
    }

    #[test]
    fn test_media_type_from_str() {
        assert_eq!("image".parse::<MediaType>().unwrap(), MediaType::Image);
        assert_eq!("video".parse::<MediaType>().unwrap(), MediaType::Video);
        assert_eq!("AUDIO".parse::<MediaType>().unwrap(), MediaType::Audio);
        assert_eq!("doc".parse::<MediaType>().unwrap(), MediaType::Document);
        assert_eq!("sticker".parse::<MediaType>().unwrap(), MediaType::Sticker);
        assert_eq!("history".parse::<MediaType>().unwrap(), MediaType::History);
        assert!("invalid_type".parse::<MediaType>().is_err());
    }
}
