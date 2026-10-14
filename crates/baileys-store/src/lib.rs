//! Storage engine for WhatsApp sessions and authentication keys.
//!
//! Provides pluggable backends:
//! - `SqliteStore`: Embedded SQLite (recommended for Aurora Chats on Android and desktop apps).
//! - `FileStore`: Multi-file JSON store (100% backward compatible with Baileys `useMultiFileAuthState`).
//! - `MemoryStore`: In-memory store (for testing and ephemeral bots).

pub mod buffer_json;
pub mod file_store;
pub mod memory_store;
pub mod sqlite_store;
pub mod traits;

pub use buffer_json::*;
pub use file_store::*;
pub use memory_store::*;
pub use sqlite_store::*;
pub use traits::*;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    #[error("I/O error in store: {0}")]
    Io(String),

    #[error("Serialization / JSON error in store: {0}")]
    Serialization(String),

    #[error("SQLite database error: {0}")]
    Database(String),

    #[error("Invalid key format")]
    InvalidKeyFormat,
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_store_creds_and_keys() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");

        // 1. Initial creds should be None
        assert!(store.get_creds().await.unwrap().is_none());

        // 2. Save creds
        let mut creds = AuthCreds::init();
        creds.me_jid = Some("123456789@s.whatsapp.net".to_string());
        store.save_creds(&creds).await.unwrap();

        // 3. Fetch creds
        let fetched = store.get_creds().await.unwrap().expect("Creds not found");
        assert_eq!(fetched.me_jid, Some("123456789@s.whatsapp.net".to_string()));
        assert_eq!(fetched.registration_id, creds.registration_id);

        // 4. Set keys
        let entries = vec![
            KeyEntry {
                key_type: "pre-key".to_string(),
                id: "1".to_string(),
                data: vec![0x01, 0x02, 0x03],
            },
            KeyEntry {
                key_type: "sender-key".to_string(),
                id: "group-123".to_string(),
                data: vec![0xAA, 0xBB],
            },
        ];
        store.set_keys(entries).await.unwrap();

        // 5. Get key
        let key = store.get_key("pre-key", "1").await.unwrap();
        assert_eq!(key, Some(vec![0x01, 0x02, 0x03]));

        // 6. Remove key
        store
            .remove_keys("pre-key", &["1".to_string()])
            .await
            .unwrap();
        assert!(store.get_key("pre-key", "1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sqlite_store_chats_messages_contacts() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");

        // 1. Save and query chats
        let chat1 = StoredChat {
            jid: "120363391057464982@g.us".to_string(),
            name: Some("[ID] Endfield Chat".to_string()),
            unread_count: 5,
            last_message_timestamp: 1700000000,
            last_message_summary: Some("Hello world".to_string()),
            pinned: false,
            archived: false,
            mute_until: None,
            ephemeral_expiration: 0,
            ephemeral_setting_timestamp: None,
        };
        store.save_chat(&chat1).await.unwrap();

        let chats = store.get_chats().await.unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].name.as_deref(), Some("[ID] Endfield Chat"));
        assert_eq!(chats[0].unread_count, 5);
        assert!(!chats[0].pinned);
        assert!(!chats[0].archived);
        assert!(chats[0].mute_until.is_none());

        // Test chat mutations
        store
            .update_chat_pin("120363391057464982@g.us", true)
            .await
            .unwrap();
        store
            .update_chat_archive("120363391057464982@g.us", true)
            .await
            .unwrap();
        store
            .update_chat_mute("120363391057464982@g.us", Some(1799999999))
            .await
            .unwrap();

        let mutated_chat = store
            .get_chat("120363391057464982@g.us")
            .await
            .unwrap()
            .unwrap();
        assert!(mutated_chat.pinned);
        assert!(mutated_chat.archived);
        assert_eq!(mutated_chat.mute_until, Some(1799999999));

        // 2. Save and query messages
        let msg1 = StoredMessage {
            id: "MSG1".to_string(),
            chat_jid: "120363391057464982@g.us".to_string(),
            sender_jid: "151514400198704:0@lid".to_string(),
            from_me: false,
            timestamp: 1700000001,
            summary: "First message".to_string(),
            status: "received".to_string(),
            has_media: false,
            proto_bytes: vec![1, 2, 3],
            is_starred: false,
            is_revoked: false,
            is_edited: false,
        };
        let msg2 = StoredMessage {
            id: "MSG2".to_string(),
            chat_jid: "120363391057464982@g.us".to_string(),
            sender_jid: "me@s.whatsapp.net".to_string(),
            from_me: true,
            timestamp: 1700000002,
            summary: "Reply message".to_string(),
            status: "sent".to_string(),
            has_media: false,
            proto_bytes: vec![4, 5, 6],
            is_starred: false,
            is_revoked: false,
            is_edited: false,
        };

        store.save_messages_batch(&[msg1, msg2]).await.unwrap();

        let messages = store
            .get_messages("120363391057464982@g.us", 10, None)
            .await
            .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "MSG2"); // DESC order
        assert_eq!(messages[1].id, "MSG1");

        // Single message fetch
        let fetched_msg = store
            .get_message("MSG1")
            .await
            .unwrap()
            .expect("MSG1 should exist");
        assert_eq!(fetched_msg.summary, "First message");
        assert_eq!(fetched_msg.proto_bytes, vec![1, 2, 3]);
        assert!(!fetched_msg.is_starred);

        // Test message star mutation
        store
            .update_message_star("120363391057464982@g.us", "MSG1", true)
            .await
            .unwrap();
        let fetched_starred = store
            .get_message("MSG1")
            .await
            .unwrap()
            .expect("MSG1 should exist");
        assert!(fetched_starred.is_starred);

        // 3. Save and query contacts
        let contact = StoredContact {
            jid: "6283162498175@s.whatsapp.net".to_string(),
            name: Some("Aurora Tester".to_string()),
            push_name: Some("Rose".to_string()),
            lid: Some("92810518745139:37@lid".to_string()),
        };
        store.save_contact(&contact).await.unwrap();

        let fetched_contact = store
            .get_contact("6283162498175@s.whatsapp.net")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched_contact.name.as_deref(), Some("Aurora Tester"));
        assert_eq!(
            fetched_contact.lid.as_deref(),
            Some("92810518745139:37@lid")
        );

        // 4. Save and query newsletters
        let nl = StoredNewsletter {
            jid: "120363144038483540@newsletter".to_string(),
            name: "WhatsApp Updates".to_string(),
            description: Some("Official Channel".to_string()),
            invite_code: Some("0029Va4K9bZ5Ui2T".to_string()),
            picture_url: None,
            role: Some("SUBSCRIBER".to_string()),
            muted: false,
            subscribers_count: 50000000,
            updated_at: 1700000005,
        };
        store.save_newsletter(&nl).await.unwrap();

        let fetched_nl = store
            .get_newsletter("120363144038483540@newsletter")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched_nl.name, "WhatsApp Updates");
        assert_eq!(fetched_nl.subscribers_count, 50000000);
        assert_eq!(fetched_nl.role.as_deref(), Some("SUBSCRIBER"));

        let all_nls = store.get_newsletters().await.unwrap();
        assert_eq!(all_nls.len(), 1);
        assert_eq!(all_nls[0].jid, "120363144038483540@newsletter");

        // 5. Save and query Polls & Votes
        let poll = StoredPoll {
            id: "POLL1".to_string(),
            chat_jid: "120363391057464982@g.us".to_string(),
            sender_jid: "me@s.whatsapp.net".to_string(),
            question: "Favorite Language?".to_string(),
            options: vec![
                "Rust".to_string(),
                "TypeScript".to_string(),
                "Python".to_string(),
            ],
            enc_key: vec![0x42; 32],
            selectable_count: 1,
            created_at: 1700000010,
        };
        store.save_poll(&poll).await.unwrap();

        let fetched_poll = store.get_poll("POLL1").await.unwrap().unwrap();
        assert_eq!(fetched_poll.question, "Favorite Language?");
        assert_eq!(fetched_poll.options.len(), 3);
        assert_eq!(fetched_poll.enc_key, vec![0x42; 32]);

        // Cast votes
        store
            .save_poll_vote("POLL1", "user1@s.whatsapp.net", &["Rust".to_string()])
            .await
            .unwrap();
        store
            .save_poll_vote("POLL1", "user2@s.whatsapp.net", &["Rust".to_string()])
            .await
            .unwrap();
        store
            .save_poll_vote("POLL1", "user3@s.whatsapp.net", &["TypeScript".to_string()])
            .await
            .unwrap();

        let votes = store.get_poll_votes("POLL1").await.unwrap();
        assert_eq!(votes.len(), 3);

        let tally = store.get_poll_tally("POLL1").await.unwrap().unwrap();
        assert_eq!(tally.total_voters, 3);
        assert_eq!(tally.option_votes.get("Rust"), Some(&2));
        assert_eq!(tally.option_votes.get("TypeScript"), Some(&1));
        assert_eq!(tally.option_votes.get("Python"), Some(&0));

        // 6. Community Subgroups
        let comm_jid = "120363000000000001@g.us";
        let sub1 = StoredCommunitySubgroup {
            group_jid: "120363000000000002@g.us".to_string(),
            subject: "Announcements".to_string(),
            creation: Some(1700000020),
            creator: Some("admin@s.whatsapp.net".to_string()),
            size: Some(15),
            linked_at: 1700000020,
        };
        let sub2 = StoredCommunitySubgroup {
            group_jid: "120363000000000003@g.us".to_string(),
            subject: "General Discussion".to_string(),
            creation: Some(1700000025),
            creator: Some("admin@s.whatsapp.net".to_string()),
            size: Some(40),
            linked_at: 1700000025,
        };

        store
            .save_community_subgroup(comm_jid, &sub1)
            .await
            .unwrap();
        store
            .save_community_subgroup(comm_jid, &sub2)
            .await
            .unwrap();

        let subgroups = store.get_community_subgroups(comm_jid).await.unwrap();
        assert_eq!(subgroups.len(), 2);
        assert_eq!(subgroups[0].subject, "Announcements");
        assert_eq!(subgroups[1].subject, "General Discussion");

        let parent = store
            .get_parent_community("120363000000000003@g.us")
            .await
            .unwrap();
        assert_eq!(parent.as_deref(), Some(comm_jid));

        // Remove subgroup
        store
            .remove_community_subgroup(comm_jid, "120363000000000003@g.us")
            .await
            .unwrap();
        let remaining = store.get_community_subgroups(comm_jid).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].group_jid, "120363000000000002@g.us");

        // 7. Call logs
        let call = StoredCall {
            call_id: "CALL_TEST_123".to_string(),
            chat_jid: "628123456789@s.whatsapp.net".to_string(),
            caller_jid: "628123456789@s.whatsapp.net".to_string(),
            is_group: false,
            group_jid: None,
            is_video: true,
            status: "offer".to_string(),
            timestamp: 1700000030,
            duration: None,
        };
        store.save_call(&call).await.unwrap();

        let fetched_call = store
            .get_call("CALL_TEST_123")
            .await
            .unwrap()
            .expect("Call should exist");
        assert_eq!(fetched_call.call_id, "CALL_TEST_123");
        assert_eq!(fetched_call.caller_jid, "628123456789@s.whatsapp.net");
        assert!(fetched_call.is_video);
        assert_eq!(fetched_call.status, "offer");

        let history = store.get_call_history(10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].call_id, "CALL_TEST_123");

        // 8. Status & Broadcast Stories
        let story1 = StoredStory {
            msg_id: "STORY_MSG_001".to_string(),
            author_jid: "628999999999@s.whatsapp.net".to_string(),
            author_name: Some("Alice".to_string()),
            timestamp: 1700000040,
            message_type: "text".to_string(),
            content: Some("Hello WhatsApp Status!".to_string()),
            caption: None,
            has_media: false,
            viewed: false,
        };
        let story2 = StoredStory {
            msg_id: "STORY_MSG_002".to_string(),
            author_jid: "628888888888@s.whatsapp.net".to_string(),
            author_name: Some("Bob".to_string()),
            timestamp: 1700000045,
            message_type: "image".to_string(),
            content: None,
            caption: Some("Vacation photo".to_string()),
            has_media: true,
            viewed: false,
        };
        store.save_story(&story1).await.unwrap();
        store.save_story(&story2).await.unwrap();

        let unviewed = store.get_stories(10, true).await.unwrap();
        assert_eq!(unviewed.len(), 2);
        assert_eq!(unviewed[0].msg_id, "STORY_MSG_002"); // ordered desc by timestamp

        store.mark_story_viewed("STORY_MSG_002").await.unwrap();
        let unviewed_after = store.get_stories(10, true).await.unwrap();
        assert_eq!(unviewed_after.len(), 1);
        assert_eq!(unviewed_after[0].msg_id, "STORY_MSG_001");

        let all_stories = store.get_stories(10, false).await.unwrap();
        assert_eq!(all_stories.len(), 2);

        let bob_stories = store
            .get_stories_by_author("628888888888@s.whatsapp.net")
            .await
            .unwrap();
        assert_eq!(bob_stories.len(), 1);
        assert_eq!(bob_stories[0].caption.as_deref(), Some("Vacation photo"));

        // 9. Blocked contacts
        store
            .save_blocked_contact("628111111111@s.whatsapp.net")
            .await
            .unwrap();
        store
            .save_blocked_contact("628222222222@s.whatsapp.net")
            .await
            .unwrap();
        let blocked = store.get_blocked_contacts().await.unwrap();
        assert_eq!(blocked.len(), 2);
        assert!(blocked.contains(&"628111111111@s.whatsapp.net".to_string()));
        assert!(blocked.contains(&"628222222222@s.whatsapp.net".to_string()));

        store
            .remove_blocked_contact("628111111111@s.whatsapp.net")
            .await
            .unwrap();
        let blocked_after_remove = store.get_blocked_contacts().await.unwrap();
        assert_eq!(blocked_after_remove.len(), 1);
        assert_eq!(blocked_after_remove[0], "628222222222@s.whatsapp.net");

        // Sync full blocklist
        let new_list = vec![
            "628333333333@s.whatsapp.net".to_string(),
            "628444444444@s.whatsapp.net".to_string(),
        ];
        store.set_blocked_contacts(&new_list).await.unwrap();
        let synced = store.get_blocked_contacts().await.unwrap();
        assert_eq!(synced.len(), 2);
        assert_eq!(synced, new_list);
    }

    #[tokio::test]
    async fn test_sqlite_store_ephemeral_edit_revoke() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");

        // Ephemeral setting
        let chat_jid = "120363999999999999@g.us";
        store
            .update_chat_ephemeral(chat_jid, 86400, Some(1700000000))
            .await
            .unwrap();

        let eph = store
            .get_chat_ephemeral(chat_jid)
            .await
            .unwrap()
            .expect("Chat ephemeral should exist");
        assert_eq!(eph.0, 86400);
        assert_eq!(eph.1, Some(1700000000));

        let chat = store
            .get_chat(chat_jid)
            .await
            .unwrap()
            .expect("Chat row should exist");
        assert_eq!(chat.ephemeral_expiration, 86400);
        assert_eq!(chat.ephemeral_setting_timestamp, Some(1700000000));

        // Message edit & revoke
        let msg = StoredMessage {
            id: "MSG_TO_EDIT".to_string(),
            chat_jid: chat_jid.to_string(),
            sender_jid: "me@s.whatsapp.net".to_string(),
            from_me: true,
            timestamp: 1700000001,
            summary: "Original message".to_string(),
            status: "sent".to_string(),
            has_media: false,
            proto_bytes: vec![1, 2, 3],
            is_starred: false,
            is_revoked: false,
            is_edited: false,
        };
        store.save_message(&msg).await.unwrap();

        // Edit message
        let updated = store
            .update_message_content(
                chat_jid,
                "MSG_TO_EDIT",
                "Edited message text",
                Some(&[9, 9, 9]),
            )
            .await
            .unwrap();
        assert!(updated);

        let fetched_edited = store
            .get_message("MSG_TO_EDIT")
            .await
            .unwrap()
            .expect("Message should exist");
        assert_eq!(fetched_edited.summary, "Edited message text");
        assert_eq!(fetched_edited.proto_bytes, vec![9, 9, 9]);
        assert!(fetched_edited.is_edited);
        assert!(!fetched_edited.is_revoked);

        // Revoke message
        let revoked = store.revoke_message(chat_jid, "MSG_TO_EDIT").await.unwrap();
        assert!(revoked);

        let fetched_revoked = store
            .get_message("MSG_TO_EDIT")
            .await
            .unwrap()
            .expect("Message should exist");
        assert_eq!(fetched_revoked.summary, "<This message was deleted>");
        assert!(fetched_revoked.is_revoked);

        // Check chat last_message_summary was updated
        let chat_after_revoke = store
            .get_chat(chat_jid)
            .await
            .unwrap()
            .expect("Chat row should exist");
        assert_eq!(
            chat_after_revoke.last_message_summary.as_deref(),
            Some("<This message was deleted>")
        );
    }

    #[tokio::test]
    async fn test_sqlite_store_labels_and_associations() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");

        // 1. Create labels
        let l1 = StoredLabel {
            id: "1".to_string(),
            name: "New Customer".to_string(),
            color: 0,
            predefined_id: Some(1),
            deleted: false,
        };
        let l2 = StoredLabel {
            id: "2".to_string(),
            name: "Pending Payment".to_string(),
            color: 5,
            predefined_id: Some(3),
            deleted: false,
        };
        store.upsert_label(&l1).await.unwrap();
        store.upsert_label(&l2).await.unwrap();

        let labels = store.get_labels().await.unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].name, "New Customer");
        assert_eq!(labels[1].name, "Pending Payment");

        let fetched = store
            .get_label("1")
            .await
            .unwrap()
            .expect("Label 1 should exist");
        assert_eq!(fetched, l1);

        // 2. Chat label associations
        let chat_jid = "628123456789@s.whatsapp.net";
        store.add_chat_label(chat_jid, "1").await.unwrap();
        store.add_chat_label(chat_jid, "2").await.unwrap();

        let chat_labels = store.get_chat_labels(chat_jid).await.unwrap();
        assert_eq!(chat_labels.len(), 2);
        assert!(chat_labels.contains(&"1".to_string()));
        assert!(chat_labels.contains(&"2".to_string()));

        let chats_with_l1 = store.get_chats_by_label("1").await.unwrap();
        assert_eq!(chats_with_l1, vec![chat_jid.to_string()]);

        store.remove_chat_label(chat_jid, "1").await.unwrap();
        let chat_labels_after = store.get_chat_labels(chat_jid).await.unwrap();
        assert_eq!(chat_labels_after, vec!["2".to_string()]);

        // 3. Message label associations
        let msg_id = "MSG_1001";
        store
            .add_message_label(chat_jid, msg_id, "2")
            .await
            .unwrap();

        let msg_labels = store.get_message_labels(chat_jid, msg_id).await.unwrap();
        assert_eq!(msg_labels, vec!["2".to_string()]);

        let msgs_with_l2 = store.get_messages_by_label("2").await.unwrap();
        assert_eq!(msgs_with_l2, vec![msg_id.to_string()]);

        store
            .remove_message_label(chat_jid, msg_id, "2")
            .await
            .unwrap();
        let msg_labels_after = store.get_message_labels(chat_jid, msg_id).await.unwrap();
        assert!(msg_labels_after.is_empty());

        // 4. Delete label
        store.delete_label("2").await.unwrap();
        let labels_after_del = store.get_labels().await.unwrap();
        assert_eq!(labels_after_del.len(), 1);
        assert_eq!(labels_after_del[0].id, "1");
    }

    #[tokio::test]
    async fn test_memory_store_roundtrip() {
        let store = MemoryStore::new();

        let creds = AuthCreds::init();
        store.save_creds(&creds).await.unwrap();

        let fetched = store.get_creds().await.unwrap().unwrap();
        assert_eq!(fetched.registration_id, creds.registration_id);
    }

    #[tokio::test]
    async fn test_file_store_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!("baileys_test_{}", rand::random::<u32>()));
        let store = FileStore::new(&temp_dir)
            .await
            .expect("Failed to create FileStore");

        let mut creds = AuthCreds::init();
        creds.me_jid = Some("987654321@s.whatsapp.net".to_string());
        store.save_creds(&creds).await.unwrap();

        let fetched = store.get_creds().await.unwrap().expect("Creds not found");
        assert_eq!(fetched.me_jid, Some("987654321@s.whatsapp.net".to_string()));

        // Clean up
        store.clear().await.unwrap();
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_sqlite_store_catalog_and_orders() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");

        // 1. Catalog Products
        let prod1 = StoredCatalogProduct {
            product_id: "prod_001".to_string(),
            business_owner_jid: "62812345678@s.whatsapp.net".to_string(),
            title: "Rose Perfume".to_string(),
            description: Some("Organic rose fragrance 50ml".to_string()),
            currency: "USD".to_string(),
            price_amount_1000: 49990, // $49.99
            sale_price_amount_1000: Some(39990),
            retailer_id: Some("SKU-RP-01".to_string()),
            url: Some("https://example.com/perfume".to_string()),
            image_urls: vec![
                "https://mmg.whatsapp.net/v/1.jpg".to_string(),
                "https://mmg.whatsapp.net/v/2.jpg".to_string(),
            ],
            is_hidden: false,
            availability: "in_stock".to_string(),
            updated_at: 1728000000,
        };
        store.save_catalog_product(&prod1).await.unwrap();

        let fetched_prod = store
            .get_catalog_product("prod_001")
            .await
            .unwrap()
            .expect("Product not found");
        assert_eq!(fetched_prod.title, "Rose Perfume");
        assert_eq!(fetched_prod.price_amount_1000, 49990);
        assert_eq!(fetched_prod.image_urls.len(), 2);
        assert_eq!(fetched_prod.availability, "in_stock");

        let biz_products = store
            .get_catalog_products("62812345678@s.whatsapp.net")
            .await
            .unwrap();
        assert_eq!(biz_products.len(), 1);
        assert_eq!(biz_products[0].product_id, "prod_001");

        // 2. Catalog Collections
        let col1 = StoredCatalogCollection {
            collection_id: "col_001".to_string(),
            business_owner_jid: "62812345678@s.whatsapp.net".to_string(),
            name: "Spring Collection 2026".to_string(),
            products_count: 12,
            status: "active".to_string(),
            updated_at: 1728000000,
        };
        store.save_catalog_collection(&col1).await.unwrap();

        let collections = store
            .get_catalog_collections("62812345678@s.whatsapp.net")
            .await
            .unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].name, "Spring Collection 2026");
        assert_eq!(collections[0].products_count, 12);

        // 3. Orders
        let ord1 = StoredOrder {
            order_id: "ord_999".to_string(),
            chat_jid: "62812345678@s.whatsapp.net".to_string(),
            seller_jid: "62812345678@s.whatsapp.net".to_string(),
            item_count: 2,
            status: "INQUIRY".to_string(),
            surface: Some("CATALOG".to_string()),
            order_title: Some("Order #999".to_string()),
            message: Some("Please prepare gift box".to_string()),
            total_amount_1000: 89980,
            total_currency_code: "USD".to_string(),
            token: Some("tok_abc123".to_string()),
            created_at: 1728000050,
        };
        store.save_order(&ord1).await.unwrap();

        let fetched_ord = store
            .get_order("ord_999")
            .await
            .unwrap()
            .expect("Order not found");
        assert_eq!(fetched_ord.order_title.as_deref(), Some("Order #999"));
        assert_eq!(fetched_ord.item_count, 2);
        assert_eq!(fetched_ord.total_amount_1000, 89980);

        let chat_orders = store
            .get_orders("62812345678@s.whatsapp.net", 10)
            .await
            .unwrap();
        assert_eq!(chat_orders.len(), 1);
        assert_eq!(chat_orders[0].order_id, "ord_999");
    }

    #[tokio::test]
    async fn test_sqlite_store_user_devices() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");

        let user = "62812345678@s.whatsapp.net";
        let initial = store.get_user_devices(user).await.unwrap();
        assert!(initial.is_empty());

        // Save devices
        store.save_user_devices(user, &[0, 37, 42]).await.unwrap();
        let devs = store.get_user_devices(user).await.unwrap();
        assert_eq!(devs, vec![0, 37, 42]);

        // Remove one device
        store.remove_user_device(user, 37).await.unwrap();
        let remaining = store.get_user_devices(user).await.unwrap();
        assert_eq!(remaining, vec![0, 42]);

        // Overwrite devices
        store.save_user_devices(user, &[0, 99]).await.unwrap();
        let updated = store.get_user_devices(user).await.unwrap();
        assert_eq!(updated, vec![0, 99]);
    }

    #[tokio::test]
    async fn test_sqlite_store_chat_actions() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");
        let chat_jid = "628999111222@s.whatsapp.net";

        // Save chat
        let chat = StoredChat {
            jid: chat_jid.to_string(),
            name: Some("Test Friend".to_string()),
            unread_count: 0,
            last_message_timestamp: 1728000000,
            last_message_summary: Some("Hello".to_string()),
            pinned: false,
            archived: false,
            mute_until: None,
            ephemeral_expiration: 0,
            ephemeral_setting_timestamp: None,
        };
        store.save_chat(&chat).await.unwrap();

        // Save 2 messages (one normal, one starred)
        let msg1 = StoredMessage {
            id: "msg_normal".to_string(),
            chat_jid: chat_jid.to_string(),
            sender_jid: chat_jid.to_string(),
            from_me: false,
            timestamp: 1728000001,
            summary: "Regular message".to_string(),
            status: "delivered".to_string(),
            has_media: false,
            proto_bytes: vec![1, 2, 3],
            is_starred: false,
            is_revoked: false,
            is_edited: false,
        };
        let msg2 = StoredMessage {
            id: "msg_starred".to_string(),
            chat_jid: chat_jid.to_string(),
            sender_jid: chat_jid.to_string(),
            from_me: false,
            timestamp: 1728000002,
            summary: "Starred message".to_string(),
            status: "delivered".to_string(),
            has_media: false,
            proto_bytes: vec![4, 5, 6],
            is_starred: true,
            is_revoked: false,
            is_edited: false,
        };
        store.save_message(&msg1).await.unwrap();
        store.save_message(&msg2).await.unwrap();

        let msgs = store.get_messages(chat_jid, 10, None).await.unwrap();
        assert_eq!(msgs.len(), 2);

        // 1. Clear chat with keep_starred = true
        store.clear_chat_messages(chat_jid, true).await.unwrap();
        let msgs_after_clear = store.get_messages(chat_jid, 10, None).await.unwrap();
        assert_eq!(msgs_after_clear.len(), 1);
        assert_eq!(msgs_after_clear[0].id, "msg_starred");

        // 2. Update chat unread
        store.update_chat_unread(chat_jid, true).await.unwrap();
        let fetched_chat = store.get_chat(chat_jid).await.unwrap().unwrap();
        assert_eq!(fetched_chat.unread_count, 1);

        store.update_chat_unread(chat_jid, false).await.unwrap();
        let fetched_chat2 = store.get_chat(chat_jid).await.unwrap().unwrap();
        assert_eq!(fetched_chat2.unread_count, 0);

        // 3. Delete chat
        store.delete_chat(chat_jid).await.unwrap();
        assert!(store.get_chat(chat_jid).await.unwrap().is_none());
        assert!(store
            .get_messages(chat_jid, 10, None)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_store_kv_and_media_downloads() {
        let store = SqliteStore::open_in_memory().expect("Failed to open SQLite");

        // 1. KV Store
        assert_eq!(store.get_kv("theme").await.unwrap(), None);
        store.set_kv("theme", "dark").await.unwrap();
        assert_eq!(
            store.get_kv("theme").await.unwrap(),
            Some("dark".to_string())
        );

        store.set_kv("settings:auto_read", "true").await.unwrap();
        store.set_kv("settings:sound", "false").await.unwrap();

        let prefix_list = store.list_kv_prefix("settings:").await.unwrap();
        assert_eq!(prefix_list.len(), 2);
        assert_eq!(
            prefix_list.get("settings:auto_read"),
            Some(&"true".to_string())
        );

        store.delete_kv("theme").await.unwrap();
        assert_eq!(store.get_kv("theme").await.unwrap(), None);

        // 2. Media Downloads
        assert_eq!(store.get_media_download("MSG_MEDIA_1").await.unwrap(), None);
        store
            .record_media_download(
                "MSG_MEDIA_1",
                "123@s.whatsapp.net",
                "/downloads/photo.jpg",
                1024,
                Some("image/jpeg"),
            )
            .await
            .unwrap();

        assert_eq!(
            store.get_media_download("MSG_MEDIA_1").await.unwrap(),
            Some("/downloads/photo.jpg".to_string())
        );
    }
}
