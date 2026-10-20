//! Domain submodule for business operations on BaileysClient.

use super::*;

impl BaileysClient {
    /// Returns all WhatsApp Business labels from local store.
    pub async fn get_labels(&self) -> Result<Vec<StoredLabel>, CoreError> {
        self.store
            .get_labels()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Returns a specific label by ID from local store.
    pub async fn get_label(&self, id: &str) -> Result<Option<StoredLabel>, CoreError> {
        self.store
            .get_label(id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Creates or updates a WhatsApp Business label, saves to SQLite, and dispatches AppState Syncd patch.
    pub async fn upsert_label(
        &self,
        id: Option<&str>,
        name: &str,
        color: i32,
        predefined_id: Option<i32>,
    ) -> Result<String, CoreError> {
        let label_id = match id {
            Some(existing_id) => existing_id.to_string(),
            None => format!("{}", rand::random::<u32>()),
        };

        let label = StoredLabel {
            id: label_id.clone(),
            name: name.to_string(),
            color,
            predefined_id,
            deleted: false,
        };

        // 1. Update local database immediately
        self.store
            .upsert_label(&label)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::LabelEdit {
            label: label.clone(),
        });

        // 3. Dispatch wire-level Syncd patch
        let id_clone = label_id.clone();
        let name_clone = name.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_label_edit_patch(
                    &id_clone,
                    &name_clone,
                    color,
                    predefined_id,
                    false,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(label_id)
    }
    /// Deletes a WhatsApp Business label, removes associations in SQLite, and dispatches AppState Syncd patch.
    pub async fn delete_label(&self, id: &str) -> Result<(), CoreError> {
        // 1. Fetch label or create tombstone
        let existing = self.store.get_label(id).await.unwrap_or(None);
        let name = existing.as_ref().map(|l| l.name.as_str()).unwrap_or("");
        let color = existing.as_ref().map(|l| l.color).unwrap_or(0);
        let predefined_id = existing.as_ref().and_then(|l| l.predefined_id);

        // 2. Update local database immediately
        self.store
            .delete_label(id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 3. Emit event
        self.events.emit(BaileysEvent::LabelEdit {
            label: StoredLabel {
                id: id.to_string(),
                name: name.to_string(),
                color,
                predefined_id,
                deleted: true,
            },
        });

        // 4. Dispatch wire-level Syncd patch
        let id_str = id.to_string();
        let name_str = name.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_label_edit_patch(
                    &id_str,
                    &name_str,
                    color,
                    predefined_id,
                    true,
                    key_id,
                    key_data,
                    state_hash,
                    ver,
                )
            },
        )
        .await;

        Ok(())
    }
    /// Associates a label with a chat (group or 1-on-1).
    pub async fn add_chat_label(&self, chat_jid: &str, label_id: &str) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .add_chat_label(&normalized, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::ChatLabelUpdate {
            chat_jid: normalized.clone(),
            label_id: label_id.to_string(),
            labeled: true,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_chat_label_patch(
                    &lid, &jid, true, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }
    /// Removes a label association from a chat.
    pub async fn remove_chat_label(&self, chat_jid: &str, label_id: &str) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .remove_chat_label(&normalized, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::ChatLabelUpdate {
            chat_jid: normalized.clone(),
            label_id: label_id.to_string(),
            labeled: false,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_chat_label_patch(
                    &lid, &jid, false, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }
    /// Gets all label IDs associated with a chat.
    pub async fn get_chat_labels(&self, chat_jid: &str) -> Result<Vec<String>, CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };
        self.store
            .get_chat_labels(&normalized)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Gets all chat JIDs associated with a label.
    pub async fn get_chats_by_label(&self, label_id: &str) -> Result<Vec<String>, CoreError> {
        self.store
            .get_chats_by_label(label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Associates a label with an individual message.
    pub async fn add_message_label(
        &self,
        chat_jid: &str,
        message_id: &str,
        label_id: &str,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .add_message_label(&normalized, message_id, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::MessageLabelUpdate {
            chat_jid: normalized.clone(),
            message_id: message_id.to_string(),
            label_id: label_id.to_string(),
            labeled: true,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        let mid = message_id.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_message_label_patch(
                    &lid, &jid, &mid, true, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }
    /// Removes a label association from an individual message.
    pub async fn remove_message_label(
        &self,
        chat_jid: &str,
        message_id: &str,
        label_id: &str,
    ) -> Result<(), CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };

        // 1. Update local database
        self.store
            .remove_message_label(&normalized, message_id, label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;

        // 2. Emit event
        self.events.emit(BaileysEvent::MessageLabelUpdate {
            chat_jid: normalized.clone(),
            message_id: message_id.to_string(),
            label_id: label_id.to_string(),
            labeled: false,
        });

        // 3. Dispatch wire-level Syncd patch
        let lid = label_id.to_string();
        let jid = normalized.clone();
        let mid = message_id.to_string();
        self.dispatch_syncd_patch(
            crate::syncd::REGULAR_LOW,
            move |key_id, key_data, state_hash, ver| {
                crate::labels::create_message_label_patch(
                    &lid, &jid, &mid, false, key_id, key_data, state_hash, ver,
                )
            },
        )
        .await;

        Ok(())
    }
    /// Gets all label IDs associated with a message.
    pub async fn get_message_labels(
        &self,
        chat_jid: &str,
        message_id: &str,
    ) -> Result<Vec<String>, CoreError> {
        let normalized = if !chat_jid.contains('@') {
            format!("{}@s.whatsapp.net", chat_jid.trim_start_matches('+'))
        } else {
            chat_jid.to_string()
        };
        self.store
            .get_message_labels(&normalized, message_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Gets all message IDs associated with a label.
    pub async fn get_messages_by_label(&self, label_id: &str) -> Result<Vec<String>, CoreError> {
        self.store
            .get_messages_by_label(label_id)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Queries the WhatsApp Business product catalog for a merchant JID.
    /// Also automatically caches all returned products into local SQLite storage.
    pub async fn get_catalog(
        &self,
        biz_jid: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::commerce::CatalogResult, CoreError> {
        let iq_node = crate::commerce::build_get_catalog_iq(biz_jid, limit, cursor);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let catalog_res = crate::commerce::parse_catalog_response(&res, biz_jid)?;

        // Cache into local database
        for prod in &catalog_res.products {
            let stored: baileys_store::StoredCatalogProduct = prod.clone().into();
            let _ = self.store.save_catalog_product(&stored).await;
        }

        Ok(catalog_res)
    }
    /// Queries a single product by product ID for a merchant.
    /// Also caches the product into local SQLite storage.
    pub async fn get_product(
        &self,
        biz_jid: &str,
        product_id: &str,
    ) -> Result<crate::commerce::CatalogProduct, CoreError> {
        let iq_node = crate::commerce::build_get_product_iq(biz_jid, product_id);
        let res = self.send_iq(iq_node, Duration::from_secs(10)).await?;
        let prod_node = res
            .get_child("response")
            .and_then(|r| r.get_child("product"))
            .or_else(|| res.get_child("product"))
            .ok_or_else(|| CoreError::Protocol("Missing product node in response".to_string()))?;

        let prod = crate::commerce::parse_product_node(prod_node, biz_jid)?;
        let stored: baileys_store::StoredCatalogProduct = prod.clone().into();
        let _ = self.store.save_catalog_product(&stored).await;

        Ok(prod)
    }
    /// Queries product collections/categories for a merchant.
    /// Also caches the collections into local SQLite storage.
    pub async fn get_collections(
        &self,
        biz_jid: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::commerce::CollectionsResult, CoreError> {
        let iq_node = crate::commerce::build_get_collections_iq(biz_jid, limit, cursor);
        let res = self.send_iq(iq_node, Duration::from_secs(15)).await?;
        let col_res = crate::commerce::parse_collections_response(&res, biz_jid)?;

        for col in &col_res.collections {
            let stored: baileys_store::StoredCatalogCollection = col.clone().into();
            let _ = self.store.save_catalog_collection(&stored).await;
        }

        Ok(col_res)
    }
    /// Sends a product card message to a recipient.
    pub async fn send_product_message(
        &self,
        to_jid: &str,
        biz_owner_jid: &str,
        product: &crate::commerce::CatalogProduct,
        caption: Option<&str>,
        footer: Option<&str>,
    ) -> Result<String, CoreError> {
        let proto = crate::commerce::build_product_message(biz_owner_jid, product, caption, footer);
        self.send_message(to_jid, proto).await
    }
    /// Sends an order card message to a recipient and saves it in SQLite.
    pub async fn send_order_message(
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
        let proto = crate::commerce::build_order_message(
            order_id,
            seller_jid,
            title,
            item_count,
            total_amount_1000,
            currency,
            message,
            thumbnail,
        );

        let msg_id = self.send_message(to_jid, proto).await?;

        // Cache order in local database
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let stored_order = baileys_store::StoredOrder {
            order_id: order_id.to_string(),
            chat_jid: to_jid.to_string(),
            seller_jid: seller_jid.to_string(),
            item_count,
            status: "INQUIRY".to_string(),
            surface: Some("CATALOG".to_string()),
            order_title: Some(title.to_string()),
            message: message.map(|s| s.to_string()),
            total_amount_1000,
            total_currency_code: currency.to_string(),
            token: None,
            created_at: now,
        };
        let _ = self.store.save_order(&stored_order).await;

        Ok(msg_id)
    }
    /// Retrieves cached products from local SQLite store for a business JID.
    pub async fn get_cached_catalog(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<baileys_store::StoredCatalogProduct>, CoreError> {
        self.store
            .get_catalog_products(biz_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Retrieves cached collections from local SQLite store for a business JID.
    pub async fn get_cached_collections(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<baileys_store::StoredCatalogCollection>, CoreError> {
        self.store
            .get_catalog_collections(biz_jid)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Retrieves cached orders for a chat from local SQLite store.
    pub async fn get_cached_orders(
        &self,
        chat_jid: &str,
        limit: u32,
    ) -> Result<Vec<baileys_store::StoredOrder>, CoreError> {
        self.store
            .get_orders(chat_jid, limit)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))
    }
    /// Creates a new commerce catalog product.
    pub async fn product_create(
        &self,
        product: &crate::commerce::CatalogProduct,
    ) -> Result<String, CoreError> {
        let iq = crate::commerce::build_product_create_iq(product);
        let res = self.send_iq(iq, Duration::from_secs(20)).await?;
        res.get_child("product")
            .and_then(|p| p.get_child("id"))
            .and_then(|id| id.content_as_string())
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Protocol("Missing product id in creation response".into()))
    }
    /// Updates an existing commerce catalog product.
    pub async fn product_update(
        &self,
        product_id: &str,
        product: &crate::commerce::CatalogProduct,
    ) -> Result<(), CoreError> {
        let iq = crate::commerce::build_product_update_iq(product_id, product);
        self.send_iq(iq, Duration::from_secs(20)).await?;
        Ok(())
    }
    /// Deletes one or more commerce catalog products.
    pub async fn product_delete(&self, product_ids: &[&str]) -> Result<(), CoreError> {
        let iq = crate::commerce::build_product_delete_iq(product_ids);
        self.send_iq(iq, Duration::from_secs(15)).await?;
        Ok(())
    }
}
