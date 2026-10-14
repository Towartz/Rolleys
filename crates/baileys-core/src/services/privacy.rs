//! Domain service for WhatsApp Privacy & Safety Settings.

use crate::client::BaileysClient;
use crate::privacy::PrivacySettings;
use crate::usync::OnWhatsAppResult;
use crate::CoreError;

/// Domain service providing all WhatsApp Privacy operations.
#[derive(Clone)]
pub struct PrivacyService {
    client: BaileysClient,
}

impl PrivacyService {
    pub fn new(client: BaileysClient) -> Self {
        Self { client }
    }

    /// Fetches the user's current privacy settings from WhatsApp gateway.
    pub async fn fetch_privacy_settings(&self) -> Result<PrivacySettings, CoreError> {
        self.client.fetch_privacy_settings().await
    }

    /// Updates a privacy setting (category: "readreceipts", "profile", "status", "online", "last", "groupadd", "calladd").
    pub async fn update_privacy_setting(
        &self,
        category: &str,
        value: &str,
    ) -> Result<(), CoreError> {
        self.client.update_privacy_setting(category, value).await
    }

    /// Fetches the list of blocked JIDs from WhatsApp server and syncs to local SQLite store.
    pub async fn fetch_blocklist(&self) -> Result<Vec<String>, CoreError> {
        self.client.fetch_blocklist().await
    }

    /// Retrieves all locally cached blocked contacts from SQLite.
    pub async fn get_blocked_contacts(&self) -> Result<Vec<String>, CoreError> {
        self.client.get_blocked_contacts().await
    }

    /// Blocks a contact and persists to local database.
    pub async fn block(&self, jid: &str) -> Result<(), CoreError> {
        self.client.update_block_status(jid, "block").await
    }

    /// Unblocks a contact and persists to local database.
    pub async fn unblock(&self, jid: &str) -> Result<(), CoreError> {
        self.client.update_block_status(jid, "unblock").await
    }

    /// Blocks or unblocks a contact ("block" or "unblock") and persists to SQLite.
    pub async fn update_block_status(&self, jid: &str, action: &str) -> Result<(), CoreError> {
        self.client.update_block_status(jid, action).await
    }

    /// Checks whether phone numbers are registered on WhatsApp using USync protocol.
    pub async fn check_on_whatsapp(
        &self,
        numbers: &[&str],
    ) -> Result<Vec<OnWhatsAppResult>, CoreError> {
        self.client.check_on_whatsapp(numbers).await
    }
}
