use baileys_store::{BaileysStore, KeyEntry};
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::BaileysConfig;

/// Fallback WhatsApp Web version if dynamic resolution fails and none is configured.
pub const DEFAULT_WA_VERSION: (u32, u32, u32) = (2, 3000, 1043857760);

/// Parses `client_revision` from WhatsApp Web service worker script (`sw.js`).
pub fn parse_client_revision(body: &str) -> Option<u32> {
    let key = "client_revision";
    let pos = body.find(key)?;
    let remainder = &body[pos + key.len()..];
    let start = remainder.find(|c: char| c.is_ascii_digit())?;
    let digits: String = remainder[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u32>().ok()
}

/// Parses the default version tuple `[p, s, t]` from Baileys `Defaults/index.ts`.
pub fn parse_baileys_default_version(body: &str) -> Option<(u32, u32, u32)> {
    let key = "const version = [";
    let pos = body.find(key)?;
    let slice = &body[pos + key.len()..];
    let end = slice.find(']')?;
    let nums: Vec<u32> = slice[..end]
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();
    if nums.len() >= 3 {
        Some((nums[0], nums[1], nums[2]))
    } else {
        None
    }
}

/// Attempts to query the live WhatsApp Web service worker, falling back to Baileys upstream.
pub async fn fetch_latest_wa_version() -> Result<(u32, u32, u32), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    // 1. Primary endpoint: https://web.whatsapp.com/sw.js
    let sw_res = client
        .get("https://web.whatsapp.com/sw.js")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .header("sec-fetch-site", "none")
        .send()
        .await;

    if let Ok(resp) = sw_res {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Some(rev) = parse_client_revision(&text) {
                    debug!("Parsed live WhatsApp Web client_revision: {}", rev);
                    return Ok((2, 3000, rev));
                }
            }
        }
    }

    // 2. Secondary fallback: Baileys GitHub upstream Defaults/index.ts
    let gh_res = client
        .get(
            "https://raw.githubusercontent.com/WhiskeySockets/Baileys/master/src/Defaults/index.ts",
        )
        .send()
        .await;

    if let Ok(resp) = gh_res {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Some(ver) = parse_baileys_default_version(&text) {
                    debug!("Parsed upstream Baileys version: {:?}", ver);
                    return Ok(ver);
                }
            }
        }
    }

    Err("Could not retrieve WhatsApp Web version from live or upstream sources".to_string())
}

/// Resolves the current WhatsApp protocol version by consulting:
/// 1. Config override (if `dynamic_version == false`)
/// 2. Local persistent store cache (`wa_version/current`)
/// 3. Remote live fetch (falling back to config version on failure)
pub async fn resolve_wa_version<S: BaileysStore + ?Sized>(
    store: &S,
    config: &BaileysConfig,
) -> (u32, u32, u32) {
    if !config.dynamic_version {
        return config.app_version;
    }

    // Check store cache
    if let Ok(Some(cached_bytes)) = store.get_key("wa_version", "current").await {
        if let Ok(ver) = serde_json::from_slice::<(u32, u32, u32)>(&cached_bytes) {
            debug!(
                "Using cached WhatsApp Web version: {}.{}.{}",
                ver.0, ver.1, ver.2
            );
            return ver;
        }
    }

    // Fetch live version
    match fetch_latest_wa_version().await {
        Ok(ver) => {
            info!(
                "Successfully resolved live WhatsApp Web version: {}.{}.{}",
                ver.0, ver.1, ver.2
            );
            if let Ok(serialized) = serde_json::to_vec(&ver) {
                let _ = store
                    .set_keys(vec![KeyEntry {
                        key_type: "wa_version".to_string(),
                        id: "current".to_string(),
                        data: serialized,
                    }])
                    .await;
            }
            ver
        }
        Err(err) => {
            warn!(
                "Dynamic WhatsApp version resolution failed: {}. Falling back to default: {}.{}.{}",
                err, config.app_version.0, config.app_version.1, config.app_version.2
            );
            config.app_version
        }
    }
}

/// Invalidates cached WhatsApp version from store (invoked on 405 Method Not Allowed / Outdated Version).
pub async fn invalidate_cached_version<S: BaileysStore + ?Sized>(store: &S) {
    warn!("Invalidating cached WhatsApp Web version to force re-fetch on next connection");
    let _ = store
        .remove_keys("wa_version", &["current".to_string()])
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_store::{MemoryStore, SignalKeysStore};

    #[test]
    fn test_parse_client_revision() {
        let sample = r#"self.__CLIENT_REVISION__ = "1043857760"; "client_revision": 1043857760, other: true"#;
        assert_eq!(parse_client_revision(sample), Some(1043857760));

        let escaped = r#"\"client_revision\": 1019626480,"#;
        assert_eq!(parse_client_revision(escaped), Some(1019626480));

        assert_eq!(parse_client_revision("nothing here"), None);
    }

    #[test]
    fn test_parse_baileys_default_version() {
        let sample = "export const version = [2, 3000, 1019626480];";
        let sample2 = "const version = [2, 3000, 1043857760];";
        assert_eq!(
            parse_baileys_default_version(sample),
            Some((2, 3000, 1019626480))
        );
        assert_eq!(
            parse_baileys_default_version(sample2),
            Some((2, 3000, 1043857760))
        );
        assert_eq!(
            parse_baileys_default_version("const irrelevant = true;"),
            None
        );
    }

    #[tokio::test]
    async fn test_version_caching_and_invalidation() {
        let store = MemoryStore::new();
        let config = BaileysConfig::default();

        // Initially no cache
        let key = store.get_key("wa_version", "current").await.unwrap();
        assert!(key.is_none());

        // Cache a custom version
        let test_ver = (2, 3000, 999999999);
        let serialized = serde_json::to_vec(&test_ver).unwrap();
        store
            .set_keys(vec![KeyEntry {
                key_type: "wa_version".to_string(),
                id: "current".to_string(),
                data: serialized,
            }])
            .await
            .unwrap();

        let resolved = resolve_wa_version(&store, &config).await;
        assert_eq!(resolved, test_ver);

        // Invalidate cache
        invalidate_cached_version(&store).await;
        let key_after = store.get_key("wa_version", "current").await.unwrap();
        assert!(key_after.is_none());
    }
}
