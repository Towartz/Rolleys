//! Domain service for WhatsApp Status Stories & Ephemeral Updates.

use crate::client::BaileysClient;
use crate::privacy::ProfileStatus;
use crate::CoreError;
use baileys_store::StoredStory;

/// Domain service providing all WhatsApp Status & Stories operations.
#[derive(Clone)]
pub struct StatusService {
    client: BaileysClient,
}

impl StatusService {
    pub fn new(client: BaileysClient) -> Self {
        Self { client }
    }

    /// Publishes a text status story to `status@broadcast`.
    pub async fn send_text(
        &self,
        text: &str,
        background_color: Option<&str>,
        font: Option<i32>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        self.client
            .send_text_status(text, background_color, font, audience)
            .await
    }

    /// Publishes an HD Image status story to `status@broadcast`.
    pub async fn send_image(
        &self,
        image_bytes: &[u8],
        caption: Option<&str>,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        self.client
            .send_status_image(image_bytes, caption, mimetype, audience)
            .await
    }

    /// Publishes an HD Video status story to `status@broadcast`.
    pub async fn send_video(
        &self,
        video_bytes: &[u8],
        caption: Option<&str>,
        mimetype: Option<&str>,
        audience: Option<Vec<String>>,
    ) -> Result<String, CoreError> {
        self.client
            .send_status_video(video_bytes, caption, mimetype, audience)
            .await
    }

    /// Sends a read receipt for a status story, marking it as viewed both on WhatsApp and locally.
    pub async fn send_read_receipt(&self, msg_id: &str, author_jid: &str) -> Result<(), CoreError> {
        self.client
            .send_status_read_receipt(msg_id, author_jid)
            .await
    }

    /// Fetches recent status updates from the SQLite cache.
    pub async fn get_recent_stories(
        &self,
        limit: u32,
        unviewed_only: bool,
    ) -> Result<Vec<StoredStory>, CoreError> {
        self.client.get_recent_stories(limit, unviewed_only).await
    }

    /// Fetches all active status stories posted by a specific contact.
    pub async fn get_stories_by_author(
        &self,
        author_jid: &str,
    ) -> Result<Vec<StoredStory>, CoreError> {
        self.client.get_stories_by_author(author_jid).await
    }

    /// Queries the text status / "About" info of a contact.
    pub async fn fetch_profile_status(
        &self,
        jid: &str,
    ) -> Result<Option<ProfileStatus>, CoreError> {
        self.client.fetch_status(jid).await
    }
}
