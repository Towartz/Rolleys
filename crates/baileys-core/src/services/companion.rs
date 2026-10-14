//! Domain service for WhatsApp Multi-Device Companion Pairing & QR codes.

use crate::client::BaileysClient;
use crate::CoreError;

/// Domain service providing Companion device pairing and QR code operations.
#[derive(Clone)]
pub struct CompanionService {
    client: BaileysClient,
}

impl CompanionService {
    pub fn new(client: BaileysClient) -> Self {
        Self { client }
    }

    /// Requests an 8-character pairing code for phone number authentication.
    pub async fn request_pairing_code(&self, phone_number: &str) -> Result<String, CoreError> {
        self.client.request_pairing_code(phone_number).await
    }

    /// Retrieves the most recent terminal QR code string if waiting for pairing.
    pub async fn get_last_qr(&self) -> Option<String> {
        self.client.get_last_qr().await
    }

    /// Queries linked companion device IDs for a user JID via USync protocol.
    pub async fn get_user_devices(&self, user_jid: &str) -> Result<Vec<u32>, CoreError> {
        self.client.get_user_devices(user_jid).await
    }
}
