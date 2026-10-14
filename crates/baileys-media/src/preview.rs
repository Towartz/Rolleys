//! WhatsApp Rich Link Preview Engine.
//!
//! Extracts OpenGraph / HTML metadata (title, description, image thumbnail) from URLs
//! to generate rich link preview cards matching WhatsApp Web behavior.

use crate::{MediaError, DEFAULT_USER_AGENT};
use image::imageops::FilterType;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::time::Duration;
use tracing::{debug, warn};

pub const DEFAULT_THUMBNAIL_WIDTH_PX: u32 = 192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlPreview {
    pub matched_text: String,
    pub canonical_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub jpeg_thumbnail: Option<Vec<u8>>,
}

/// Extracts the first HTTP/HTTPS URL from a message text string.
pub fn extract_first_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| {
            c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == '<'
                || c == '>'
                || c == '"'
                || c == '\''
                || c == ','
                || c == '.'
        });

        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return Some(trimmed.to_string());
        }

        // Match well-known domain suffixes if no protocol prefix is present
        if trimmed.contains('.') && !trimmed.starts_with('.') && !trimmed.ends_with('.') {
            let lower = trimmed.to_lowercase();
            if lower.contains(".com")
                || lower.contains(".org")
                || lower.contains(".net")
                || lower.contains(".io")
                || lower.contains(".dev")
                || lower.contains(".app")
                || lower.contains(".me")
                || lower.contains(".co")
            {
                return Some(format!("https://{}", trimmed));
            }
        }
    }
    None
}

/// Simple, robust HTML parser that extracts OpenGraph and standard meta tags without heavy dependencies.
pub fn parse_opengraph_html(
    html: &str,
    base_url: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut image_url: Option<String> = None;

    // Search for <meta ...> tags
    let mut rest = html;
    while let Some(start) = rest.find("<meta ") {
        rest = &rest[start + 6..];
        if let Some(end) = rest.find('>') {
            let tag = &rest[..end];
            rest = &rest[end + 1..];

            let tag_lower = tag.to_lowercase();
            let is_og_title = tag_lower.contains("property=\"og:title\"")
                || tag_lower.contains("property='og:title'")
                || tag_lower.contains("name=\"twitter:title\"")
                || tag_lower.contains("name='twitter:title'");

            let is_og_desc = tag_lower.contains("property=\"og:description\"")
                || tag_lower.contains("property='og:description'")
                || tag_lower.contains("name=\"description\"")
                || tag_lower.contains("name='description'")
                || tag_lower.contains("name=\"twitter:description\"")
                || tag_lower.contains("name='twitter:description'");

            let is_og_img = tag_lower.contains("property=\"og:image\"")
                || tag_lower.contains("property='og:image'")
                || tag_lower.contains("name=\"twitter:image\"")
                || tag_lower.contains("name='twitter:image'");

            if let Some(content_val) = extract_attr_value(tag, "content") {
                let cleaned = html_escape::decode_html_entities(&content_val)
                    .trim()
                    .to_string();
                if is_og_title && title.is_none() && !cleaned.is_empty() {
                    title = Some(cleaned);
                } else if is_og_desc && description.is_none() && !cleaned.is_empty() {
                    description = Some(cleaned);
                } else if is_og_img && image_url.is_none() && !cleaned.is_empty() {
                    image_url = Some(resolve_url(base_url, &cleaned));
                }
            }
        } else {
            break;
        }
    }

    // Fallback: <title> tag if og:title was not found
    if title.is_none() {
        if let Some(start) = html.to_lowercase().find("<title>") {
            let after = &html[start + 7..];
            if let Some(end) = after.to_lowercase().find("</title>") {
                let raw_title = &after[..end];
                let cleaned = html_escape::decode_html_entities(raw_title)
                    .trim()
                    .to_string();
                if !cleaned.is_empty() {
                    title = Some(cleaned);
                }
            }
        }
    }

    (title, description, image_url)
}

fn extract_attr_value(tag: &str, attr: &str) -> Option<String> {
    let lower_tag = tag.to_lowercase();
    let pattern_double = format!("{}=\"", attr);
    let pattern_single = format!("{}='", attr);

    if let Some(pos) = lower_tag.find(&pattern_double) {
        let after = &tag[pos + pattern_double.len()..];
        if let Some(end) = after.find('"') {
            return Some(after[..end].to_string());
        }
    } else if let Some(pos) = lower_tag.find(&pattern_single) {
        let after = &tag[pos + pattern_single.len()..];
        if let Some(end) = after.find('\'') {
            return Some(after[..end].to_string());
        }
    }
    None
}

fn resolve_url(base: &str, target: &str) -> String {
    if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else if target.starts_with("//") {
        format!("https:{}", target)
    } else if target.starts_with('/') {
        if let Ok(parsed) = reqwest::Url::parse(base) {
            format!(
                "{}://{}{}",
                parsed.scheme(),
                parsed.host_str().unwrap_or_default(),
                target
            )
        } else {
            target.to_string()
        }
    } else {
        format!("{}/{}", base.trim_end_matches('/'), target)
    }
}

/// Scales down an image to a JPEG thumbnail with maximum width matching WhatsApp standard.
pub fn generate_jpeg_thumbnail(
    image_bytes: &[u8],
    target_width: u32,
) -> Result<Vec<u8>, MediaError> {
    let img = image::load_from_memory(image_bytes).map_err(|e| {
        MediaError::DecryptionFailed(format!("Failed to decode preview image: {}", e))
    })?;

    let (width, height) = (img.width(), img.height());
    let thumb = if width > target_width {
        let target_height = (height as f64 * (target_width as f64 / width as f64)).round() as u32;
        img.resize(target_width, target_height.max(1), FilterType::Lanczos3)
    } else {
        img
    };

    let mut out_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut out_bytes);
    thumb
        .write_to(&mut cursor, ImageFormat::Jpeg)
        .map_err(|e| {
            MediaError::EncryptionFailed(format!("Failed to encode JPEG thumbnail: {}", e))
        })?;

    Ok(out_bytes)
}

/// Fetches rich metadata (OpenGraph & thumbnail) for the first URL in the provided message text.
pub async fn fetch_url_preview(
    text: &str,
    custom_client: Option<&reqwest::Client>,
) -> Result<Option<UrlPreview>, MediaError> {
    let url = match extract_first_url(text) {
        Some(u) => u,
        None => return Ok(None),
    };

    debug!("Fetching link preview for URL: {}", url);

    let default_client;
    let client = match custom_client {
        Some(c) => c,
        None => {
            default_client = reqwest::Client::builder()
                .timeout(Duration::from_secs(4))
                .user_agent(DEFAULT_USER_AGENT)
                .build()
                .map_err(|e| MediaError::HttpError(e.to_string()))?;
            &default_client
        }
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to fetch URL for link preview ({}): {}", url, e);
            return Ok(None);
        }
    };

    if !resp.status().is_success() {
        return Ok(None);
    }

    let final_url = resp.url().to_string();
    let html = match resp.text().await {
        Ok(h) => h,
        Err(_) => return Ok(None),
    };

    let (title, description, image_url) = parse_opengraph_html(&html, &final_url);

    // If neither title nor description could be found, preview is not useful
    if title.is_none() && description.is_none() {
        return Ok(None);
    }

    let mut jpeg_thumbnail = None;
    if let Some(ref img_href) = image_url {
        if let Ok(img_resp) = client.get(img_href).send().await {
            if img_resp.status().is_success() {
                if let Ok(bytes) = img_resp.bytes().await {
                    if !bytes.is_empty() && bytes.len() <= 5 * 1024 * 1024 {
                        if let Ok(thumb) =
                            generate_jpeg_thumbnail(&bytes, DEFAULT_THUMBNAIL_WIDTH_PX)
                        {
                            jpeg_thumbnail = Some(thumb);
                        }
                    }
                }
            }
        }
    }

    Ok(Some(UrlPreview {
        matched_text: url.clone(),
        canonical_url: final_url,
        title,
        description,
        jpeg_thumbnail,
    }))
}

mod html_escape {
    pub fn decode_html_entities(s: &str) -> String {
        s.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&nbsp;", " ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_first_url() {
        let msg =
            "Check out this link: https://github.com/Rose-Project/rose-baileys for more info!";
        assert_eq!(
            extract_first_url(msg),
            Some("https://github.com/Rose-Project/rose-baileys".to_string())
        );

        let bare = "Visit google.com today.";
        assert_eq!(
            extract_first_url(bare),
            Some("https://google.com".to_string())
        );

        let no_url = "Hello there no links in here";
        assert_eq!(extract_first_url(no_url), None);
    }

    #[test]
    fn test_parse_opengraph_html() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Test Page Title</title>
                <meta property="og:title" content="Rose Baileys WhatsApp Engine &amp; SDK" />
                <meta property="og:description" content="Ultra-fast pure Rust WhatsApp protocol client." />
                <meta property="og:image" content="/assets/cover.png" />
            </head>
            <body><h1>Welcome</h1></body>
            </html>
        "#;

        let (title, desc, img) = parse_opengraph_html(html, "https://example.com");
        assert_eq!(title.as_deref(), Some("Rose Baileys WhatsApp Engine & SDK"));
        assert_eq!(
            desc.as_deref(),
            Some("Ultra-fast pure Rust WhatsApp protocol client.")
        );
        assert_eq!(img.as_deref(), Some("https://example.com/assets/cover.png"));
    }

    #[test]
    fn test_generate_jpeg_thumbnail() {
        // Create a 200x200 RGB image
        let img = image::RgbImage::new(200, 200);
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
            .unwrap();

        let thumb_jpeg =
            generate_jpeg_thumbnail(&png_bytes, 100).expect("Thumbnail generation failed");
        assert!(!thumb_jpeg.is_empty());

        // Verify decoded thumbnail width is <= 100
        let decoded = image::load_from_memory(&thumb_jpeg).expect("Load decoded thumb failed");
        assert_eq!(decoded.width(), 100);
        assert_eq!(decoded.height(), 100);
    }
}
