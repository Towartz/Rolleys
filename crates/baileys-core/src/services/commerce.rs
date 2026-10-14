//! Domain service for WhatsApp Business Commerce, Product Catalogs, and Orders.

use crate::client::BaileysClient;
use crate::commerce::{CatalogProduct, CatalogResult, CollectionsResult};
use crate::CoreError;
use baileys_store::{StoredCatalogCollection, StoredCatalogProduct, StoredOrder};

/// Domain service providing all WhatsApp Commerce & Catalog operations.
#[derive(Clone)]
pub struct CommerceService {
    client: BaileysClient,
}

impl CommerceService {
    pub fn new(client: BaileysClient) -> Self {
        Self { client }
    }

    /// Queries the WhatsApp Business product catalog for a merchant JID.
    pub async fn get_catalog(
        &self,
        biz_jid: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<CatalogResult, CoreError> {
        self.client.get_catalog(biz_jid, limit, cursor).await
    }

    /// Queries a single product by product ID for a merchant.
    pub async fn get_product(
        &self,
        biz_jid: &str,
        product_id: &str,
    ) -> Result<CatalogProduct, CoreError> {
        self.client.get_product(biz_jid, product_id).await
    }

    /// Queries product collections/categories for a merchant.
    pub async fn get_collections(
        &self,
        biz_jid: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<CollectionsResult, CoreError> {
        self.client.get_collections(biz_jid, limit, cursor).await
    }

    /// Sends an interactive product card message to a recipient.
    pub async fn send_product(
        &self,
        to_jid: &str,
        biz_owner_jid: &str,
        product: &CatalogProduct,
        caption: Option<&str>,
        footer: Option<&str>,
    ) -> Result<String, CoreError> {
        self.client
            .send_product_message(to_jid, biz_owner_jid, product, caption, footer)
            .await
    }

    /// Sends an interactive order card message to a recipient and saves it in SQLite.
    pub async fn send_order(
        &self,
        to_jid: &str,
        seller_jid: &str,
        order_id: &str,
        title: &str,
        item_count: i32,
        total_amount_1000: i64,
        currency: &str,
        message: Option<&str>,
        thumbnail: Option<Vec<u8>>,
    ) -> Result<String, CoreError> {
        self.client
            .send_order_message(
                to_jid,
                seller_jid,
                order_id,
                title,
                item_count,
                total_amount_1000,
                currency,
                message,
                thumbnail,
            )
            .await
    }

    /// Retrieves cached products from local SQLite store for a business JID.
    pub async fn get_cached_catalog(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<StoredCatalogProduct>, CoreError> {
        self.client.get_cached_catalog(biz_jid).await
    }

    /// Retrieves cached collections from local SQLite store for a business JID.
    pub async fn get_cached_collections(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<StoredCatalogCollection>, CoreError> {
        self.client.get_cached_collections(biz_jid).await
    }

    /// Retrieves cached orders for a chat from local SQLite store.
    pub async fn get_cached_orders(
        &self,
        chat_jid: &str,
        limit: u32,
    ) -> Result<Vec<StoredOrder>, CoreError> {
        self.client.get_cached_orders(chat_jid, limit).await
    }
}
