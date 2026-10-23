//! Domain submodule for companions operations on BaileysClient.

use super::*;

/// How long a cached device list is trusted, as Baileys' `DEFAULT_CACHE_TTLS.USER_DEVICES`.
pub(crate) const USER_DEVICES_TTL_SECS: i64 = 5 * 60;

/// Whether a device list saved at `saved_at` may still be used at `now`; a store that does not
/// track the time keeps the old behaviour of trusting its list.
fn device_cache_is_fresh(saved_at: Option<i64>, now: i64) -> bool {
    saved_at.is_none_or(|at| now - at < USER_DEVICES_TTL_SECS)
}

impl BaileysClient {
    /// Queries linked companion devices for a user JID via USync protocol.
    /// Also updates local SQLite storage cache.
    pub async fn get_user_devices(&self, user_jid: &str) -> Result<Vec<u32>, CoreError> {
        let normalized = if !user_jid.contains('@') {
            format!("{}@{}", user_jid, S_WHATSAPP_NET)
        } else {
            user_jid.to_string()
        };
        let norm_user = baileys_binary::jid::jid_normalized_user(&normalized);

        // Baileys parity: a cached list is used for USER_DEVICES_TTL_SECS, then queried again, so a
        // device unlinked in the meantime stops being sent to (and asked for pre-keys).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        for key in [&normalized, &norm_user] {
            if let Ok((cached, saved_at)) = self.store.get_user_devices_with_time(key).await {
                let fresh = device_cache_is_fresh(saved_at, now);
                if !cached.is_empty() && fresh {
                    return Ok(cached);
                }
            }
        }

        // Cache miss: Query WhatsApp USync gateway
        let iq_node = crate::usync::build_usync_devices_iq(&[&normalized]);
        match self.send_iq(iq_node, Duration::from_secs(10)).await {
            Ok(res) => {
                if let Ok(user_devs) = crate::usync::parse_usync_devices_response(&res) {
                    if let Some(ud) = user_devs.into_iter().find(|u| {
                        baileys_binary::jid::jid_normalized_user(&u.user_jid) == norm_user
                            || u.user_jid == normalized
                    }) {
                        let mut ids: Vec<u32> = ud.devices.iter().map(|d| d.id).collect();
                        if !ids.contains(&0) {
                            ids.insert(0, 0);
                        }
                        let _ = self.store.save_user_devices(&normalized, &ids).await;
                        let _ = self.store.save_user_devices(&norm_user, &ids).await;
                        return Ok(ids);
                    }
                }
            }
            Err(e) => {
                debug!(
                    "USync device query failed for {}: {}, falling back to local cache",
                    normalized, e
                );
            }
        }

        let cached = self
            .store
            .get_user_devices(&normalized)
            .await
            .unwrap_or_default();
        if cached.is_empty() {
            Ok(vec![0])
        } else {
            Ok(cached)
        }
    }
    /// Removes one device (`user:device@server`) from the cached device list of its user.
    pub(crate) async fn forget_device(&self, device_jid: &str) {
        let Some((user_part, server)) = device_jid.split_once('@') else {
            return;
        };
        let Some((user, device)) = user_part.split_once(':') else {
            return; // the primary device (0) always exists
        };
        let Ok(device) = device.parse::<u32>() else {
            return;
        };
        let user_jid = format!("{user}@{server}");
        debug!("Dropping unknown device {device_jid} from the device cache");
        let _ = self.store.remove_user_device(&user_jid, device).await;
    }

    /// Queries all active companion devices linked to our current account.
    pub async fn get_paired_devices(&self) -> Result<Vec<crate::usync::DeviceRecord>, CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Auth("No active session credentials found".into()))?;

        let me_jid = creds.me_jid.as_deref().unwrap_or("");
        let user_jid = if let Some((u, _)) = me_jid.split_once(':') {
            format!("{}@{}", u, S_WHATSAPP_NET)
        } else if let Some((u, _)) = me_jid.split_once('@') {
            format!("{}@{}", u, S_WHATSAPP_NET)
        } else {
            return Err(CoreError::Auth("Invalid me_jid format".into()));
        };

        let iq_node = crate::usync::build_usync_devices_iq(&[&user_jid]);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let user_devs = crate::usync::parse_usync_devices_response(&res)?;

        if let Some(ud) = user_devs.into_iter().find(|u| u.user_jid == user_jid) {
            let ids: Vec<u32> = ud.devices.iter().map(|d| d.id).collect();
            let _ = self.store.save_user_devices(&user_jid, &ids).await;
            Ok(ud.devices)
        } else {
            let cached_ids = self
                .store
                .get_user_devices(&user_jid)
                .await
                .map_err(|e| CoreError::Store(e.to_string()))?;
            Ok(cached_ids
                .into_iter()
                .map(|id| crate::usync::DeviceRecord {
                    id,
                    key_index: None,
                })
                .collect())
        }
    }
    /// Revokes/unlinks a companion device from our account by device ID.
    pub async fn remove_companion_device(&self, device_id: u32) -> Result<(), CoreError> {
        let creds = self
            .store
            .get_creds()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?
            .ok_or_else(|| CoreError::Auth("No active session credentials found".into()))?;

        let me_jid = creds.me_jid.as_deref().unwrap_or("");
        let user_phone = if let Some((u, _)) = me_jid.split_once(':') {
            u
        } else if let Some((u, _)) = me_jid.split_once('@') {
            u
        } else {
            return Err(CoreError::Auth("Invalid me_jid format".into()));
        };

        let dev_jid = format!("{}:{}@{}", user_phone, device_id, S_WHATSAPP_NET);
        let user_jid = format!("{}@{}", user_phone, S_WHATSAPP_NET);

        let iq_node = crate::usync::build_remove_device_iq(&dev_jid);
        self.send_iq(iq_node, Duration::from_secs(10)).await?;

        let _ = self.store.remove_user_device(&user_jid, device_id).await;
        Ok(())
    }
    /// Resolves LIDs from Phone Number JIDs via USync protocol.
    pub async fn get_lid_for_pn(
        &self,
        pn_jids: &[&str],
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq = crate::usync::build_usync_lid_iq(pn_jids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_lid_response(&res)
    }
    /// Resolves Phone Number JIDs from LIDs via USync protocol.
    pub async fn get_pn_for_lid(
        &self,
        lid_jids: &[&str],
    ) -> Result<Vec<(String, String)>, CoreError> {
        let iq = crate::usync::build_usync_lid_iq(lid_jids);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_lid_response(&res)
    }

    /// Looks up a WhatsApp account by its @username and optional 4-digit PIN via USync.
    pub async fn lookup_username(
        &self,
        username: &str,
        pin: Option<&str>,
    ) -> Result<Option<crate::username::ResolvedUsername>, CoreError> {
        let clean = crate::username::normalize_username(username);
        crate::username::validate_username(&clean)
            .map_err(|e| CoreError::Protocol(e.to_string()))?;

        if let Some(p) = pin {
            if !crate::username::is_valid_username_key(p) {
                return Err(CoreError::Protocol("Username key must be 4 digits".into()));
            }
        }

        let iq = crate::usync::build_usync_username_lookup_iq(&clean, pin);
        let res = self.send_iq(iq, Duration::from_secs(15)).await?;
        crate::usync::parse_usync_username_response(&res, &clean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_store::{AppStateStore, SqliteStore};

    #[test]
    fn a_device_list_expires_after_the_baileys_ttl() {
        assert!(device_cache_is_fresh(Some(1_000), 1_000));
        assert!(device_cache_is_fresh(
            Some(1_000),
            1_000 + USER_DEVICES_TTL_SECS - 1
        ));
        assert!(!device_cache_is_fresh(
            Some(1_000),
            1_000 + USER_DEVICES_TTL_SECS
        ));
        assert!(device_cache_is_fresh(None, 1_000_000));
    }

    #[tokio::test]
    async fn a_fresh_list_is_served_and_an_unknown_device_is_forgotten() {
        let store = std::sync::Arc::new(SqliteStore::open_in_memory().unwrap());
        let user = "92810518745139@lid";
        store.save_user_devices(user, &[0, 60, 72]).await.unwrap();
        let config = crate::BaileysConfigBuilder::new()
            .with_websocket_url("mock://devices")
            .build();
        let client = BaileysClient::new(store.clone(), config);

        // Just saved: no network needed (the client is not even connected).
        assert_eq!(
            client.get_user_devices(user).await.unwrap(),
            vec![0, 60, 72]
        );

        client.forget_device("92810518745139:72@lid").await;
        client.forget_device("92810518745139@lid").await; // primary: kept
        assert_eq!(store.get_user_devices(user).await.unwrap(), vec![0, 60]);
    }
}
