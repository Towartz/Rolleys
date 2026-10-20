//! Domain submodule for companions operations on BaileysClient.

use super::*;

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

        // Baileys parity: Check SQLite persistent cache first!
        if let Ok(cached) = self.store.get_user_devices(&normalized).await {
            if !cached.is_empty() {
                return Ok(cached);
            }
        }
        if let Ok(cached) = self.store.get_user_devices(&norm_user).await {
            if !cached.is_empty() {
                return Ok(cached);
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
}
