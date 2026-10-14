//! Domain service for WhatsApp messages.

use crate::client::{BaileysClient, MessageOptions};
use crate::CoreError;
use baileys_proto::proto::Message;

/// Domain service providing messaging operations.
#[derive(Clone)]
pub struct MessageService {
    client: BaileysClient,
}

impl MessageService {
    pub fn new(client: BaileysClient) -> Self {
        Self { client }
    }

    /// Sends a raw Protobuf message to `to_jid`.
    pub async fn send(&self, to_jid: &str, message: Message) -> Result<String, CoreError> {
        self.client.send_message(to_jid, message).await
    }

    /// Sends a Protobuf message with custom relay options (e.g. status broadcast recipient list).
    pub async fn send_with_options(
        &self,
        to_jid: &str,
        message: Message,
        options: MessageOptions,
    ) -> Result<String, CoreError> {
        self.client
            .send_message_with_options(to_jid, message, options)
            .await
    }

    /// Sends a plaintext message to `to_jid`.
    pub async fn send_text(&self, to_jid: &str, text: &str) -> Result<String, CoreError> {
        self.client.send_text_message(to_jid, text).await
    }

    /// Sends a plaintext message quoting an earlier message.
    pub async fn send_text_with_quote(
        &self,
        to_jid: &str,
        text: &str,
        quoted_id: &str,
        quoted_text: &str,
        quoted_participant: Option<&str>,
    ) -> Result<String, CoreError> {
        self.client
            .send_text_message_with_quote(
                to_jid,
                text,
                quoted_id,
                Some(quoted_text),
                quoted_participant,
            )
            .await
    }

    /// Sends an encrypted JPEG/PNG image message to `to_jid`.
    pub async fn send_image(
        &self,
        to_jid: &str,
        image_data: &[u8],
        caption: Option<&str>,
    ) -> Result<String, CoreError> {
        self.client
            .send_image_message(to_jid, image_data, caption)
            .await
    }

    /// Sends an encrypted document message to `to_jid`.
    pub async fn send_document(
        &self,
        to_jid: &str,
        data: &[u8],
        filename: &str,
        mimetype: &str,
        title: Option<&str>,
    ) -> Result<String, CoreError> {
        self.client
            .send_document_message(to_jid, data, filename, mimetype, title)
            .await
    }

    /// Sends an emoji reaction to a message in `to_jid`.
    pub async fn react(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<String, CoreError> {
        self.client.send_reaction(to_jid, msg_id, emoji).await
    }

    /// Creates and sends an interactive poll.
    pub async fn send_poll(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        selectable_count: u32,
    ) -> Result<String, CoreError> {
        self.client
            .send_poll(to_jid, question, options, selectable_count)
            .await
    }

    /// Casts a vote on an existing poll.
    pub async fn vote_poll(
        &self,
        chat_jid: &str,
        poll_msg_id: &str,
        selected_options: &[String],
    ) -> Result<String, CoreError> {
        self.client
            .send_poll_vote(chat_jid, poll_msg_id, selected_options)
            .await
    }

    /// Sends a geographic location message.
    pub async fn send_location(
        &self,
        to_jid: &str,
        lat: f64,
        lng: f64,
        name: Option<&str>,
        address: Option<&str>,
        url: Option<&str>,
    ) -> Result<String, CoreError> {
        self.client
            .send_location(to_jid, lat, lng, name, address, url)
            .await
    }

    /// Sends a vCard contact card message.
    pub async fn send_contact(
        &self,
        to_jid: &str,
        display_name: &str,
        phone: &str,
    ) -> Result<String, CoreError> {
        self.client.send_contact(to_jid, display_name, phone).await
    }

    /// Sends a text message with automatic rich link preview card.
    pub async fn send_text_with_preview(
        &self,
        to_jid: &str,
        text: &str,
    ) -> Result<String, CoreError> {
        self.client.send_text_with_preview(to_jid, text).await
    }
}
