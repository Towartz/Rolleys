use async_trait::async_trait;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use crate::traits::{
    AppStateStore, AuthCreds, ChatStore, KeyEntry, NewsletterStore, PollTally, SessionStore,
    SignalKeysStore, StoredCall, StoredCatalogCollection, StoredCatalogProduct, StoredChat,
    StoredCommunitySubgroup, StoredContact, StoredLabel, StoredMessage, StoredNewsletter,
    StoredOrder, StoredPoll, StoredPollVote, StoredStory,
};
use crate::StoreError;

const SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS creds (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        json TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS session_keys (
        key_type TEXT NOT NULL,
        id TEXT NOT NULL,
        data BLOB NOT NULL,
        PRIMARY KEY (key_type, id)
    );
    CREATE TABLE IF NOT EXISTS chats (
        jid TEXT PRIMARY KEY,
        name TEXT,
        unread_count INTEGER NOT NULL DEFAULT 0,
        last_message_timestamp INTEGER NOT NULL DEFAULT 0,
        last_message_summary TEXT,
        pinned INTEGER NOT NULL DEFAULT 0,
        archived INTEGER NOT NULL DEFAULT 0,
        mute_until INTEGER,
        ephemeral_expiration INTEGER NOT NULL DEFAULT 0,
        ephemeral_setting_timestamp INTEGER
    );
    CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        chat_jid TEXT NOT NULL,
        sender_jid TEXT NOT NULL,
        from_me INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        summary TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'received',
        has_media INTEGER NOT NULL DEFAULT 0,
        proto_bytes BLOB NOT NULL,
        is_starred INTEGER NOT NULL DEFAULT 0,
        is_revoked INTEGER NOT NULL DEFAULT 0,
        is_edited INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_messages_chat_ts ON messages (chat_jid, timestamp DESC);
    CREATE TABLE IF NOT EXISTS contacts (
        jid TEXT PRIMARY KEY,
        name TEXT,
        push_name TEXT,
        lid TEXT
    );
    CREATE TABLE IF NOT EXISTS newsletters (
        jid TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT,
        invite_code TEXT,
        picture_url TEXT,
        role TEXT,
        muted INTEGER NOT NULL DEFAULT 0,
        subscribers_count INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS polls (
        id TEXT PRIMARY KEY,
        chat_jid TEXT NOT NULL,
        sender_jid TEXT NOT NULL,
        question TEXT NOT NULL,
        options_json TEXT NOT NULL,
        enc_key BLOB NOT NULL,
        selectable_count INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS poll_votes (
        poll_id TEXT NOT NULL,
        voter_jid TEXT NOT NULL,
        selected_options_json TEXT NOT NULL,
        updated_at INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (poll_id, voter_jid)
    );
    CREATE TABLE IF NOT EXISTS community_links (
        community_jid TEXT NOT NULL,
        group_jid TEXT NOT NULL,
        subject TEXT,
        creation INTEGER,
        creator TEXT,
        size INTEGER,
        linked_at INTEGER NOT NULL,
        PRIMARY KEY (community_jid, group_jid)
    );
    CREATE TABLE IF NOT EXISTS call_logs (
        call_id TEXT PRIMARY KEY,
        chat_jid TEXT NOT NULL,
        caller_jid TEXT NOT NULL,
        is_group INTEGER NOT NULL,
        group_jid TEXT,
        is_video INTEGER NOT NULL,
        status TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        duration INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_call_logs_ts ON call_logs(timestamp DESC);
    CREATE TABLE IF NOT EXISTS status_stories (
        msg_id TEXT PRIMARY KEY,
        author_jid TEXT NOT NULL,
        author_name TEXT,
        timestamp INTEGER NOT NULL,
        message_type TEXT NOT NULL,
        content TEXT,
        caption TEXT,
        has_media INTEGER NOT NULL DEFAULT 0,
        viewed INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_status_stories_ts ON status_stories(timestamp DESC);
    CREATE INDEX IF NOT EXISTS idx_status_stories_author ON status_stories(author_jid);
    CREATE TABLE IF NOT EXISTS blocked_contacts (
        jid TEXT PRIMARY KEY,
        blocked_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS labels (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        color INTEGER NOT NULL,
        predefined_id INTEGER,
        deleted INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS chat_labels (
        chat_jid TEXT NOT NULL,
        label_id TEXT NOT NULL,
        PRIMARY KEY (chat_jid, label_id)
    );
    CREATE INDEX IF NOT EXISTS idx_chat_labels_label ON chat_labels(label_id);
    CREATE TABLE IF NOT EXISTS message_labels (
        chat_jid TEXT NOT NULL,
        message_id TEXT NOT NULL,
        label_id TEXT NOT NULL,
        PRIMARY KEY (chat_jid, message_id, label_id)
    );
    CREATE INDEX IF NOT EXISTS idx_msg_labels_label ON message_labels(label_id);
    CREATE TABLE IF NOT EXISTS catalog_products (
        product_id TEXT PRIMARY KEY,
        business_owner_jid TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        currency TEXT NOT NULL,
        price_amount_1000 INTEGER NOT NULL,
        sale_price_amount_1000 INTEGER,
        retailer_id TEXT,
        url TEXT,
        image_urls TEXT,
        is_hidden INTEGER NOT NULL DEFAULT 0,
        availability TEXT NOT NULL DEFAULT 'in_stock',
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_catalog_products_biz ON catalog_products(business_owner_jid);
    CREATE TABLE IF NOT EXISTS catalog_collections (
        collection_id TEXT PRIMARY KEY,
        business_owner_jid TEXT NOT NULL,
        name TEXT NOT NULL,
        products_count INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'active',
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_catalog_collections_biz ON catalog_collections(business_owner_jid);
    CREATE TABLE IF NOT EXISTS orders (
        order_id TEXT PRIMARY KEY,
        chat_jid TEXT NOT NULL,
        seller_jid TEXT NOT NULL,
        item_count INTEGER NOT NULL,
        status TEXT NOT NULL,
        surface TEXT,
        order_title TEXT,
        message TEXT,
        total_amount_1000 INTEGER NOT NULL,
        total_currency_code TEXT NOT NULL,
        token TEXT,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_orders_chat_ts ON orders(chat_jid, created_at DESC);
    CREATE TABLE IF NOT EXISTS user_devices (
        user_jid TEXT NOT NULL,
        device_id INTEGER NOT NULL,
        key_index INTEGER DEFAULT 0,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (user_jid, device_id)
    );
    CREATE INDEX IF NOT EXISTS idx_user_devices_jid ON user_devices(user_jid);
    CREATE TABLE IF NOT EXISTS kv_store (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS media_downloads (
        message_id TEXT PRIMARY KEY,
        chat_jid TEXT NOT NULL,
        file_path TEXT NOT NULL,
        file_size INTEGER NOT NULL DEFAULT 0,
        mimetype TEXT,
        downloaded_at INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_media_downloads_chat ON media_downloads(chat_jid);
    CREATE TABLE IF NOT EXISTS sync_cursors (
        sync_type TEXT PRIMARY KEY,
        cursor_val TEXT NOT NULL,
        updated_at INTEGER NOT NULL DEFAULT 0
    );
";

/// High-performance embedded SQLite database store, recommended for Aurora Chats (Android) and desktop apps.
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let conn = Connection::open(p).map_err(|e| StoreError::Database(e.to_string()))?;

        // Initialize schema with WAL and NORMAL sync for maximum mobile/desktop I/O throughput
        let batch = format!(
            "{} PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;",
            SCHEMA_SQL
        );
        conn.execute_batch(&batch)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        // Backward-compatible non-destructive migrations for existing databases
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE chats ADD COLUMN mute_until INTEGER", []);
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN ephemeral_expiration INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN ephemeral_setting_timestamp INTEGER",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN is_starred INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN is_revoked INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN is_edited INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS newsletters (
            jid TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            invite_code TEXT,
            picture_url TEXT,
            role TEXT,
            muted INTEGER NOT NULL DEFAULT 0,
            subscribers_count INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS polls (
            id TEXT PRIMARY KEY,
            chat_jid TEXT NOT NULL,
            sender_jid TEXT NOT NULL,
            question TEXT NOT NULL,
            options_json TEXT NOT NULL,
            enc_key BLOB NOT NULL,
            selectable_count INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS poll_votes (
            poll_id TEXT NOT NULL,
            voter_jid TEXT NOT NULL,
            selected_options_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (poll_id, voter_jid)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS community_links (
            community_jid TEXT NOT NULL,
            group_jid TEXT NOT NULL,
            subject TEXT,
            creation INTEGER,
            creator TEXT,
            size INTEGER,
            linked_at INTEGER NOT NULL,
            PRIMARY KEY (community_jid, group_jid)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS call_logs (
            call_id TEXT PRIMARY KEY,
            chat_jid TEXT NOT NULL,
            caller_jid TEXT NOT NULL,
            is_group INTEGER NOT NULL,
            group_jid TEXT,
            is_video INTEGER NOT NULL,
            status TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            duration INTEGER
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_call_logs_ts ON call_logs(timestamp DESC)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS status_stories (
            msg_id TEXT PRIMARY KEY,
            author_jid TEXT NOT NULL,
            author_name TEXT,
            timestamp INTEGER NOT NULL,
            message_type TEXT NOT NULL,
            content TEXT,
            caption TEXT,
            has_media INTEGER NOT NULL DEFAULT 0,
            viewed INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_status_stories_ts ON status_stories(timestamp DESC)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_status_stories_author ON status_stories(author_jid)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS blocked_contacts (
            jid TEXT PRIMARY KEY,
            blocked_at INTEGER NOT NULL
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS labels (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            color INTEGER NOT NULL,
            predefined_id INTEGER,
            deleted INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS chat_labels (
            chat_jid TEXT NOT NULL,
            label_id TEXT NOT NULL,
            PRIMARY KEY (chat_jid, label_id)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_labels_label ON chat_labels(label_id)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS message_labels (
            chat_jid TEXT NOT NULL,
            message_id TEXT NOT NULL,
            label_id TEXT NOT NULL,
            PRIMARY KEY (chat_jid, message_id, label_id)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_msg_labels_label ON message_labels(label_id)",
            [],
        );

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory().map_err(|e| StoreError::Database(e.to_string()))?;

        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        // Backward-compatible non-destructive migrations for existing databases
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE chats ADD COLUMN mute_until INTEGER", []);
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN ephemeral_expiration INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE chats ADD COLUMN ephemeral_setting_timestamp INTEGER",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN is_starred INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN is_revoked INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN is_edited INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS newsletters (
            jid TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            invite_code TEXT,
            picture_url TEXT,
            role TEXT,
            muted INTEGER NOT NULL DEFAULT 0,
            subscribers_count INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS polls (
            id TEXT PRIMARY KEY,
            chat_jid TEXT NOT NULL,
            sender_jid TEXT NOT NULL,
            question TEXT NOT NULL,
            options_json TEXT NOT NULL,
            enc_key BLOB NOT NULL,
            selectable_count INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS poll_votes (
            poll_id TEXT NOT NULL,
            voter_jid TEXT NOT NULL,
            selected_options_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (poll_id, voter_jid)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS community_links (
            community_jid TEXT NOT NULL,
            group_jid TEXT NOT NULL,
            subject TEXT NOT NULL,
            creator TEXT,
            size INTEGER,
            linked_at INTEGER NOT NULL,
            PRIMARY KEY (community_jid, group_jid)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS call_logs (
            call_id TEXT PRIMARY KEY,
            chat_jid TEXT NOT NULL,
            caller_jid TEXT NOT NULL,
            is_group INTEGER NOT NULL,
            group_jid TEXT,
            is_video INTEGER NOT NULL,
            status TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            duration INTEGER
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_call_logs_ts ON call_logs(timestamp DESC)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS status_stories (
            msg_id TEXT PRIMARY KEY,
            author_jid TEXT NOT NULL,
            author_name TEXT,
            timestamp INTEGER NOT NULL,
            message_type TEXT NOT NULL,
            content TEXT,
            caption TEXT,
            has_media INTEGER NOT NULL DEFAULT 0,
            viewed INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_status_stories_ts ON status_stories(timestamp DESC)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_status_stories_author ON status_stories(author_jid)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS blocked_contacts (
            jid TEXT PRIMARY KEY,
            blocked_at INTEGER NOT NULL
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS labels (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            color INTEGER NOT NULL,
            predefined_id INTEGER,
            deleted INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS chat_labels (
            chat_jid TEXT NOT NULL,
            label_id TEXT NOT NULL,
            PRIMARY KEY (chat_jid, label_id)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_labels_label ON chat_labels(label_id)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS message_labels (
            chat_jid TEXT NOT NULL,
            message_id TEXT NOT NULL,
            label_id TEXT NOT NULL,
            PRIMARY KEY (chat_jid, message_id, label_id)
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_msg_labels_label ON message_labels(label_id)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS kv_store (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS media_downloads (
            message_id TEXT PRIMARY KEY,
            chat_jid TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_size INTEGER NOT NULL DEFAULT 0,
            mimetype TEXT,
            downloaded_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_media_downloads_chat ON media_downloads(chat_jid)",
            [],
        );
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_cursors (
            sync_type TEXT PRIMARY KEY,
            cursor_val TEXT NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0
        )",
            [],
        );

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl SessionStore for SqliteStore {
    async fn get_creds(&self) -> Result<Option<AuthCreds>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT json FROM creds WHERE id = 1")
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let json_str: String = row
                .get(0)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let creds = serde_json::from_str(&json_str)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            Ok(Some(creds))
        } else {
            Ok(None)
        }
    }

    async fn save_creds(&self, creds: &AuthCreds) -> Result<(), StoreError> {
        let json_str =
            serde_json::to_string(creds).map_err(|e| StoreError::Serialization(e.to_string()))?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO creds (id, json) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![json_str],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    async fn clear(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "DELETE FROM creds;
             DELETE FROM session_keys;
             DELETE FROM chats;
             DELETE FROM messages;
             DELETE FROM contacts;",
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl SignalKeysStore for SqliteStore {
    async fn get_key(&self, key_type: &str, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT data FROM session_keys WHERE key_type = ?1 AND id = ?2")
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![key_type, id])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let data: Vec<u8> = row
                .get(0)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    async fn set_keys(&self, entries: Vec<KeyEntry>) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO session_keys (key_type, id, data) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key_type, id) DO UPDATE SET data = excluded.data",
                )
                .map_err(|e| StoreError::Database(e.to_string()))?;

            for entry in entries {
                stmt.execute(params![entry.key_type, entry.id, entry.data])
                    .map_err(|e| StoreError::Database(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    async fn remove_keys(&self, key_type: &str, ids: &[String]) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare("DELETE FROM session_keys WHERE key_type = ?1 AND id = ?2")
                .map_err(|e| StoreError::Database(e.to_string()))?;

            for id in ids {
                stmt.execute(params![key_type, id])
                    .map_err(|e| StoreError::Database(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    async fn get_key_count(&self, key_type: &str) -> Result<usize, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM session_keys WHERE key_type = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let count: usize = stmt
            .query_row(params![key_type], |r| r.get(0))
            .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn get_all_keys(&self, key_type: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, data FROM session_keys WHERE key_type = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![key_type], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let result = rows.flatten().collect();
        Ok(result)
    }
}

#[async_trait]
impl ChatStore for SqliteStore {
    async fn save_chat(&self, chat: &StoredChat) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chats (jid, name, unread_count, last_message_timestamp, last_message_summary, pinned, archived, mute_until, ephemeral_expiration, ephemeral_setting_timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(jid) DO UPDATE SET
                name = COALESCE(excluded.name, chats.name),
                unread_count = excluded.unread_count,
                last_message_timestamp = MAX(chats.last_message_timestamp, excluded.last_message_timestamp),
                last_message_summary = COALESCE(excluded.last_message_summary, chats.last_message_summary),
                pinned = CASE WHEN excluded.pinned != 0 THEN excluded.pinned ELSE chats.pinned END,
                archived = CASE WHEN excluded.archived != 0 THEN excluded.archived ELSE chats.archived END,
                mute_until = COALESCE(excluded.mute_until, chats.mute_until),
                ephemeral_expiration = CASE WHEN excluded.ephemeral_expiration != 0 THEN excluded.ephemeral_expiration ELSE chats.ephemeral_expiration END,
                ephemeral_setting_timestamp = COALESCE(excluded.ephemeral_setting_timestamp, chats.ephemeral_setting_timestamp)",
            params![
                chat.jid,
                chat.name,
                chat.unread_count,
                chat.last_message_timestamp as i64,
                chat.last_message_summary,
                if chat.pinned { 1 } else { 0 },
                if chat.archived { 1 } else { 0 },
                chat.mute_until.map(|ts| ts as i64),
                chat.ephemeral_expiration as i64,
                chat.ephemeral_setting_timestamp,
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_chats(&self) -> Result<Vec<StoredChat>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT jid, name, unread_count, last_message_timestamp, last_message_summary, pinned, archived, mute_until, ephemeral_expiration, ephemeral_setting_timestamp FROM chats ORDER BY pinned DESC, last_message_timestamp DESC")
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let ts: i64 = row.get(3)?;
                let pinned_int: i32 = row.get(5).unwrap_or(0);
                let archived_int: i32 = row.get(6).unwrap_or(0);
                let mute_until_val: Option<i64> = row.get(7).unwrap_or(None);
                let ephemeral_expiration: u32 = row.get(8).unwrap_or(0);
                let ephemeral_setting_timestamp: Option<i64> = row.get(9).unwrap_or(None);
                Ok(StoredChat {
                    jid: row.get(0)?,
                    name: row.get(1)?,
                    unread_count: row.get(2)?,
                    last_message_timestamp: ts as u64,
                    last_message_summary: row.get(4)?,
                    pinned: pinned_int != 0,
                    archived: archived_int != 0,
                    mute_until: mute_until_val.map(|v| v as u64),
                    ephemeral_expiration,
                    ephemeral_setting_timestamp,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut chats = Vec::new();
        for r in rows {
            chats.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(chats)
    }

    async fn get_chat(&self, jid: &str) -> Result<Option<StoredChat>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT jid, name, unread_count, last_message_timestamp, last_message_summary, pinned, archived, mute_until, ephemeral_expiration, ephemeral_setting_timestamp FROM chats WHERE jid = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let ts: i64 = row
                .get(3)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let pinned_int: i32 = row.get(5).unwrap_or(0);
            let archived_int: i32 = row.get(6).unwrap_or(0);
            let mute_until_val: Option<i64> = row.get(7).unwrap_or(None);
            let ephemeral_expiration: u32 = row.get(8).unwrap_or(0);
            let ephemeral_setting_timestamp: Option<i64> = row.get(9).unwrap_or(None);
            Ok(Some(StoredChat {
                jid: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                name: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                unread_count: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                last_message_timestamp: ts as u64,
                last_message_summary: row
                    .get(4)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                pinned: pinned_int != 0,
                archived: archived_int != 0,
                mute_until: mute_until_val.map(|v| v as u64),
                ephemeral_expiration,
                ephemeral_setting_timestamp,
            }))
        } else {
            Ok(None)
        }
    }

    async fn save_message(&self, msg: &StoredMessage) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, chat_jid, sender_jid, from_me, timestamp, summary, status, has_media, proto_bytes, is_starred, is_revoked, is_edited)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                summary = excluded.summary,
                is_starred = CASE WHEN excluded.is_starred != 0 THEN excluded.is_starred ELSE messages.is_starred END,
                is_revoked = CASE WHEN excluded.is_revoked != 0 THEN excluded.is_revoked ELSE messages.is_revoked END,
                is_edited = CASE WHEN excluded.is_edited != 0 THEN excluded.is_edited ELSE messages.is_edited END",
            params![
                msg.id,
                msg.chat_jid,
                msg.sender_jid,
                if msg.from_me { 1 } else { 0 },
                msg.timestamp as i64,
                msg.summary,
                msg.status,
                if msg.has_media { 1 } else { 0 },
                msg.proto_bytes,
                if msg.is_starred { 1 } else { 0 },
                if msg.is_revoked { 1 } else { 0 },
                if msg.is_edited { 1 } else { 0 },
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn save_messages_batch(&self, messages: &[StoredMessage]) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO messages (id, chat_jid, sender_jid, from_me, timestamp, summary, status, has_media, proto_bytes, is_starred, is_revoked, is_edited)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                     ON CONFLICT(id) DO UPDATE SET
                        status = excluded.status,
                        summary = excluded.summary,
                        is_starred = CASE WHEN excluded.is_starred != 0 THEN excluded.is_starred ELSE messages.is_starred END,
                        is_revoked = CASE WHEN excluded.is_revoked != 0 THEN excluded.is_revoked ELSE messages.is_revoked END,
                        is_edited = CASE WHEN excluded.is_edited != 0 THEN excluded.is_edited ELSE messages.is_edited END",
                )
                .map_err(|e| StoreError::Database(e.to_string()))?;

            for msg in messages {
                stmt.execute(params![
                    msg.id,
                    msg.chat_jid,
                    msg.sender_jid,
                    if msg.from_me { 1 } else { 0 },
                    msg.timestamp as i64,
                    msg.summary,
                    msg.status,
                    if msg.has_media { 1 } else { 0 },
                    msg.proto_bytes,
                    if msg.is_starred { 1 } else { 0 },
                    if msg.is_revoked { 1 } else { 0 },
                    if msg.is_edited { 1 } else { 0 },
                ])
                .map_err(|e| StoreError::Database(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    async fn get_messages(
        &self,
        chat_jid: &str,
        limit: u32,
        before_timestamp: Option<u64>,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut messages = Vec::new();

        if let Some(before_ts) = before_timestamp {
            let mut stmt = conn
                .prepare(
                    "SELECT id, chat_jid, sender_jid, from_me, timestamp, summary, status, has_media, proto_bytes, is_starred, is_revoked, is_edited
                     FROM messages
                     WHERE chat_jid = ?1 AND timestamp < ?2
                     ORDER BY timestamp DESC LIMIT ?3",
                )
                .map_err(|e| StoreError::Database(e.to_string()))?;

            let rows = stmt
                .query_map(params![chat_jid, before_ts as i64, limit], |row| {
                    let from_me_int: i32 = row.get(3)?;
                    let ts: i64 = row.get(4)?;
                    let has_media_int: i32 = row.get(7)?;
                    let is_starred_int: i32 = row.get(9).unwrap_or(0);
                    let is_revoked_int: i32 = row.get(10).unwrap_or(0);
                    let is_edited_int: i32 = row.get(11).unwrap_or(0);
                    Ok(StoredMessage {
                        id: row.get(0)?,
                        chat_jid: row.get(1)?,
                        sender_jid: row.get(2)?,
                        from_me: from_me_int != 0,
                        timestamp: ts as u64,
                        summary: row.get(5)?,
                        status: row.get(6)?,
                        has_media: has_media_int != 0,
                        proto_bytes: row.get(8)?,
                        is_starred: is_starred_int != 0,
                        is_revoked: is_revoked_int != 0,
                        is_edited: is_edited_int != 0,
                    })
                })
                .map_err(|e| StoreError::Database(e.to_string()))?;

            for r in rows {
                messages.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, chat_jid, sender_jid, from_me, timestamp, summary, status, has_media, proto_bytes, is_starred, is_revoked, is_edited
                     FROM messages
                     WHERE chat_jid = ?1
                     ORDER BY timestamp DESC LIMIT ?2",
                )
                .map_err(|e| StoreError::Database(e.to_string()))?;

            let rows = stmt
                .query_map(params![chat_jid, limit], |row| {
                    let from_me_int: i32 = row.get(3)?;
                    let ts: i64 = row.get(4)?;
                    let has_media_int: i32 = row.get(7)?;
                    let is_starred_int: i32 = row.get(9).unwrap_or(0);
                    let is_revoked_int: i32 = row.get(10).unwrap_or(0);
                    let is_edited_int: i32 = row.get(11).unwrap_or(0);
                    Ok(StoredMessage {
                        id: row.get(0)?,
                        chat_jid: row.get(1)?,
                        sender_jid: row.get(2)?,
                        from_me: from_me_int != 0,
                        timestamp: ts as u64,
                        summary: row.get(5)?,
                        status: row.get(6)?,
                        has_media: has_media_int != 0,
                        proto_bytes: row.get(8)?,
                        is_starred: is_starred_int != 0,
                        is_revoked: is_revoked_int != 0,
                        is_edited: is_edited_int != 0,
                    })
                })
                .map_err(|e| StoreError::Database(e.to_string()))?;

            for r in rows {
                messages.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
            }
        }

        Ok(messages)
    }

    async fn get_message(&self, id: &str) -> Result<Option<StoredMessage>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, chat_jid, sender_jid, from_me, timestamp, summary, status, has_media, proto_bytes, is_starred, is_revoked, is_edited
                 FROM messages WHERE id = ?1",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![id])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let from_me_int: i32 = row
                .get(3)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let ts: i64 = row
                .get(4)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let has_media_int: i32 = row
                .get(7)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let is_starred_int: i32 = row.get(9).unwrap_or(0);
            let is_revoked_int: i32 = row.get(10).unwrap_or(0);
            let is_edited_int: i32 = row.get(11).unwrap_or(0);
            Ok(Some(StoredMessage {
                id: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                chat_jid: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                sender_jid: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                from_me: from_me_int != 0,
                timestamp: ts as u64,
                summary: row
                    .get(5)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                status: row
                    .get(6)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                has_media: has_media_int != 0,
                proto_bytes: row
                    .get(8)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                is_starred: is_starred_int != 0,
                is_revoked: is_revoked_int != 0,
                is_edited: is_edited_int != 0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_latest_message(&self) -> Result<Option<StoredMessage>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, chat_jid, sender_jid, from_me, timestamp, summary, status, has_media, proto_bytes, is_starred, is_revoked, is_edited
                 FROM messages ORDER BY timestamp DESC LIMIT 1",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query([])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let from_me_int: i32 = row
                .get(3)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let ts: i64 = row
                .get(4)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let has_media_int: i32 = row
                .get(7)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let is_starred_int: i32 = row.get(9).unwrap_or(0);
            let is_revoked_int: i32 = row.get(10).unwrap_or(0);
            let is_edited_int: i32 = row.get(11).unwrap_or(0);
            Ok(Some(StoredMessage {
                id: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                chat_jid: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                sender_jid: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                from_me: from_me_int != 0,
                timestamp: ts as u64,
                summary: row
                    .get(5)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                status: row
                    .get(6)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                has_media: has_media_int != 0,
                proto_bytes: row
                    .get(8)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                is_starred: is_starred_int != 0,
                is_revoked: is_revoked_int != 0,
                is_edited: is_edited_int != 0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn update_chat_pin(&self, jid: &str, pinned: bool) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chats (jid, pinned) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET pinned = excluded.pinned",
            params![jid, if pinned { 1 } else { 0 }],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_chat_archive(&self, jid: &str, archived: bool) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chats (jid, archived) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET archived = excluded.archived",
            params![jid, if archived { 1 } else { 0 }],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_chat_mute(&self, jid: &str, mute_until: Option<u64>) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chats (jid, mute_until) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET mute_until = excluded.mute_until",
            params![jid, mute_until.map(|ts| ts as i64)],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_message_star(
        &self,
        chat_jid: &str,
        msg_id: &str,
        is_starred: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET is_starred = ?1 WHERE id = ?2 AND chat_jid = ?3",
            params![if is_starred { 1 } else { 0 }, msg_id, chat_jid],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_message_status(&self, msg_id: &str, status: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET status = ?1 WHERE id = ?2",
            params![status, msg_id],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn clear_chat_messages(
        &self,
        chat_jid: &str,
        keep_starred: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        if keep_starred {
            conn.execute(
                "DELETE FROM messages WHERE chat_jid = ?1 AND is_starred = 0",
                params![chat_jid],
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;
        } else {
            conn.execute(
                "DELETE FROM messages WHERE chat_jid = ?1",
                params![chat_jid],
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;
        }
        let _ = conn.execute(
            "UPDATE chats SET last_message_summary = NULL WHERE jid = ?1",
            params![chat_jid],
        );
        Ok(())
    }

    async fn delete_chat(&self, chat_jid: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM messages WHERE chat_jid = ?1",
            params![chat_jid],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        conn.execute("DELETE FROM chats WHERE jid = ?1", params![chat_jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let _ = conn.execute(
            "DELETE FROM chat_labels WHERE chat_jid = ?1",
            params![chat_jid],
        );
        Ok(())
    }

    async fn update_chat_unread(&self, chat_jid: &str, unread: bool) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let unread_count = if unread { 1 } else { 0 };
        conn.execute(
            "INSERT INTO chats (jid, unread_count) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET unread_count = excluded.unread_count",
            params![chat_jid, unread_count],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn save_contact(&self, contact: &StoredContact) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO contacts (jid, name, push_name, lid)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(jid) DO UPDATE SET
                name = COALESCE(excluded.name, contacts.name),
                push_name = COALESCE(excluded.push_name, contacts.push_name),
                lid = COALESCE(excluded.lid, contacts.lid)",
            params![contact.jid, contact.name, contact.push_name, contact.lid],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_contact(&self, jid: &str) -> Result<Option<StoredContact>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT jid, name, push_name, lid FROM contacts WHERE jid = ?1 OR lid = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            Ok(Some(StoredContact {
                jid: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                name: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                push_name: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                lid: row
                    .get(3)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_contacts(&self) -> Result<Vec<StoredContact>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT jid, name, push_name, lid FROM contacts ORDER BY name ASC")
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(StoredContact {
                    jid: row.get(0)?,
                    name: row.get(1)?,
                    push_name: row.get(2)?,
                    lid: row.get(3)?,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut contacts = Vec::new();
        for r in rows {
            contacts.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(contacts)
    }

    async fn search_messages(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn
            .prepare(
                "SELECT id, chat_jid, sender_jid, from_me, timestamp, summary, status, has_media, proto_bytes, is_starred, is_revoked, is_edited
                 FROM messages WHERE summary LIKE ?1 ORDER BY timestamp DESC LIMIT ?2"
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![pattern, limit], |row| {
                let from_me_int: i32 = row.get(3)?;
                let has_media_int: i32 = row.get(7)?;
                let starred_int: i32 = row.get(9)?;
                let revoked_int: i32 = row.get(10)?;
                let edited_int: i32 = row.get(11)?;

                Ok(StoredMessage {
                    id: row.get(0)?,
                    chat_jid: row.get(1)?,
                    sender_jid: row.get(2)?,
                    from_me: from_me_int > 0,
                    timestamp: row.get(4)?,
                    summary: row.get(5)?,
                    status: row.get(6)?,
                    has_media: has_media_int > 0,
                    proto_bytes: row.get(8)?,
                    is_starred: starred_int > 0,
                    is_revoked: revoked_int > 0,
                    is_edited: edited_int > 0,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut msgs = Vec::new();
        for r in rows {
            msgs.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(msgs)
    }

    async fn update_message_content(
        &self,
        chat_jid: &str,
        message_id: &str,
        new_content: &str,
        new_proto_bytes: Option<&[u8]>,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.lock().unwrap();
        let count = if let Some(bytes) = new_proto_bytes {
            conn.execute(
                "UPDATE messages SET summary = ?1, proto_bytes = ?2, is_edited = 1 WHERE id = ?3 AND chat_jid = ?4",
                params![new_content, bytes, message_id, chat_jid],
            )
        } else {
            conn.execute(
                "UPDATE messages SET summary = ?1, is_edited = 1 WHERE id = ?2 AND chat_jid = ?3",
                params![new_content, message_id, chat_jid],
            )
        }.map_err(|e| StoreError::Database(e.to_string()))?;

        // Also update last_message_summary in chats if this was the latest message
        let _ = conn.execute(
            "UPDATE chats SET last_message_summary = ?1 WHERE jid = ?2 AND last_message_timestamp <= (SELECT timestamp FROM messages WHERE id = ?3)",
            params![new_content, chat_jid, message_id],
        );

        Ok(count > 0)
    }

    async fn revoke_message(&self, chat_jid: &str, message_id: &str) -> Result<bool, StoreError> {
        self.revoke_message_ex(chat_jid, message_id, false).await
    }

    async fn revoke_message_ex(
        &self,
        chat_jid: &str,
        message_id: &str,
        anti_revoke: bool,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.lock().unwrap();
        let msg_sql = if anti_revoke {
            "UPDATE messages SET summary = CASE WHEN summary LIKE '[REVOKED]%' THEN summary ELSE '[REVOKED] ' || summary END, is_revoked = 1 WHERE id = ?1 AND chat_jid = ?2"
        } else {
            "UPDATE messages SET summary = '<This message was deleted>', is_revoked = 1 WHERE id = ?1 AND chat_jid = ?2"
        };

        let count = conn
            .execute(msg_sql, params![message_id, chat_jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let chat_sql = if anti_revoke {
            "UPDATE chats SET last_message_summary = CASE WHEN last_message_summary LIKE '[REVOKED]%' THEN last_message_summary ELSE '[REVOKED] ' || last_message_summary END WHERE jid = ?1 AND last_message_timestamp <= (SELECT timestamp FROM messages WHERE id = ?2)"
        } else {
            "UPDATE chats SET last_message_summary = '<This message was deleted>' WHERE jid = ?1 AND last_message_timestamp <= (SELECT timestamp FROM messages WHERE id = ?2)"
        };

        let _ = conn.execute(chat_sql, params![chat_jid, message_id]);

        Ok(count > 0)
    }

    async fn update_chat_ephemeral(
        &self,
        chat_jid: &str,
        ephemeral_expiration: u32,
        setting_timestamp: Option<i64>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chats (jid, ephemeral_expiration, ephemeral_setting_timestamp)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(jid) DO UPDATE SET
                ephemeral_expiration = excluded.ephemeral_expiration,
                ephemeral_setting_timestamp = excluded.ephemeral_setting_timestamp",
            params![chat_jid, ephemeral_expiration as i64, setting_timestamp],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_chat_ephemeral(
        &self,
        chat_jid: &str,
    ) -> Result<Option<(u32, Option<i64>)>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT ephemeral_expiration, ephemeral_setting_timestamp FROM chats WHERE jid = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut rows = stmt
            .query(params![chat_jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let expiration: u32 = row.get(0).unwrap_or(0);
            let ts: Option<i64> = row.get(1).unwrap_or(None);
            Ok(Some((expiration, ts)))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl NewsletterStore for SqliteStore {
    async fn save_newsletter(&self, newsletter: &StoredNewsletter) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO newsletters (jid, name, description, invite_code, picture_url, role, muted, subscribers_count, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(jid) DO UPDATE SET
                name = excluded.name,
                description = COALESCE(excluded.description, newsletters.description),
                invite_code = COALESCE(excluded.invite_code, newsletters.invite_code),
                picture_url = COALESCE(excluded.picture_url, newsletters.picture_url),
                role = COALESCE(excluded.role, newsletters.role),
                muted = excluded.muted,
                subscribers_count = excluded.subscribers_count,
                updated_at = excluded.updated_at",
            params![
                newsletter.jid,
                newsletter.name,
                newsletter.description,
                newsletter.invite_code,
                newsletter.picture_url,
                newsletter.role,
                if newsletter.muted { 1 } else { 0 },
                newsletter.subscribers_count as i64,
                newsletter.updated_at as i64,
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_newsletters(&self) -> Result<Vec<StoredNewsletter>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT jid, name, description, invite_code, picture_url, role, muted, subscribers_count, updated_at
                 FROM newsletters ORDER BY updated_at DESC",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let muted_int: i32 = row.get(6)?;
                let subs: i64 = row.get(7)?;
                let updated: i64 = row.get(8)?;
                Ok(StoredNewsletter {
                    jid: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    invite_code: row.get(3)?,
                    picture_url: row.get(4)?,
                    role: row.get(5)?,
                    muted: muted_int != 0,
                    subscribers_count: subs as u64,
                    updated_at: updated as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_newsletter(&self, jid: &str) -> Result<Option<StoredNewsletter>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT jid, name, description, invite_code, picture_url, role, muted, subscribers_count, updated_at
                 FROM newsletters WHERE jid = ?1",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query(params![jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let muted_int: i32 = row
                .get(6)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let subs: i64 = row
                .get(7)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let updated: i64 = row
                .get(8)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            Ok(Some(StoredNewsletter {
                jid: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                name: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                description: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                invite_code: row
                    .get(3)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                picture_url: row
                    .get(4)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                role: row
                    .get(5)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                muted: muted_int != 0,
                subscribers_count: subs as u64,
                updated_at: updated as u64,
            }))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl AppStateStore for SqliteStore {
    async fn save_poll(&self, poll: &StoredPoll) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let options_json =
            serde_json::to_string(&poll.options).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT OR REPLACE INTO polls (id, chat_jid, sender_jid, question, options_json, enc_key, selectable_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                poll.id,
                poll.chat_jid,
                poll.sender_jid,
                poll.question,
                options_json,
                poll.enc_key,
                poll.selectable_count as i64,
                poll.created_at as i64,
            ],
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_poll(&self, poll_id: &str) -> Result<Option<StoredPoll>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, chat_jid, sender_jid, question, options_json, enc_key, selectable_count, created_at
             FROM polls WHERE id = ?1"
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        let mut rows = stmt
            .query(params![poll_id])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let options_json: String = row
                .get(4)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let options: Vec<String> = serde_json::from_str(&options_json).unwrap_or_default();
            let selectable: i64 = row
                .get(6)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let created: i64 = row
                .get(7)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            Ok(Some(StoredPoll {
                id: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                chat_jid: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                sender_jid: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                question: row
                    .get(3)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                options,
                enc_key: row
                    .get(5)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                selectable_count: selectable as u32,
                created_at: created as u64,
            }))
        } else {
            Ok(None)
        }
    }

    async fn save_poll_vote(
        &self,
        poll_id: &str,
        voter_jid: &str,
        selected_options: &[String],
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let options_json =
            serde_json::to_string(&selected_options).unwrap_or_else(|_| "[]".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        conn.execute(
            "INSERT OR REPLACE INTO poll_votes (poll_id, voter_jid, selected_options_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![poll_id, voter_jid, options_json, now as i64],
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_poll_votes(&self, poll_id: &str) -> Result<Vec<StoredPollVote>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT poll_id, voter_jid, selected_options_json, updated_at
             FROM poll_votes WHERE poll_id = ?1",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![poll_id], |row| {
                let opts_json: String = row.get(2)?;
                let selected_options: Vec<String> =
                    serde_json::from_str(&opts_json).unwrap_or_default();
                let updated: i64 = row.get(3)?;
                Ok(StoredPollVote {
                    poll_id: row.get(0)?,
                    voter_jid: row.get(1)?,
                    selected_options,
                    updated_at: updated as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_poll_tally(&self, poll_id: &str) -> Result<Option<PollTally>, StoreError> {
        let poll_opt = self.get_poll(poll_id).await?;
        let poll = match poll_opt {
            Some(p) => p,
            None => return Ok(None),
        };
        let votes = self.get_poll_votes(poll_id).await?;
        let mut option_votes: HashMap<String, u32> = HashMap::new();
        for opt in &poll.options {
            option_votes.insert(opt.clone(), 0);
        }
        for v in &votes {
            for opt in &v.selected_options {
                *option_votes.entry(opt.clone()).or_insert(0) += 1;
            }
        }
        Ok(Some(PollTally {
            poll_id: poll.id,
            question: poll.question,
            total_voters: votes.len(),
            option_votes,
        }))
    }

    async fn save_community_subgroup(
        &self,
        community_jid: &str,
        subgroup: &StoredCommunitySubgroup,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO community_links (community_jid, group_jid, subject, creation, creator, size, linked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                community_jid,
                subgroup.group_jid,
                subgroup.subject,
                subgroup.creation.map(|c| c as i64),
                subgroup.creator,
                subgroup.size.map(|s| s as i64),
                subgroup.linked_at as i64,
            ],
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn remove_community_subgroup(
        &self,
        community_jid: &str,
        group_jid: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM community_links WHERE community_jid = ?1 AND group_jid = ?2",
            params![community_jid, group_jid],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_community_subgroups(
        &self,
        community_jid: &str,
    ) -> Result<Vec<StoredCommunitySubgroup>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT group_jid, subject, creation, creator, size, linked_at
             FROM community_links WHERE community_jid = ?1
             ORDER BY linked_at ASC",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![community_jid], |row| {
                let creation: Option<i64> = row.get(2)?;
                let size: Option<i64> = row.get(4)?;
                let linked_at: i64 = row.get(5)?;
                Ok(StoredCommunitySubgroup {
                    group_jid: row.get(0)?,
                    subject: row.get(1)?,
                    creation: creation.map(|c| c as u64),
                    creator: row.get(3)?,
                    size: size.map(|s| s as u32),
                    linked_at: linked_at as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_parent_community(&self, group_jid: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT community_jid FROM community_links WHERE group_jid = ?1 LIMIT 1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut rows = stmt
            .query(params![group_jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let comm: String = row
                .get(0)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            Ok(Some(comm))
        } else {
            Ok(None)
        }
    }

    async fn save_call(&self, call: &StoredCall) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO call_logs (
                call_id, chat_jid, caller_jid, is_group, group_jid, is_video, status, timestamp, duration
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                call.call_id,
                call.chat_jid,
                call.caller_jid,
                if call.is_group { 1 } else { 0 },
                call.group_jid,
                if call.is_video { 1 } else { 0 },
                call.status,
                call.timestamp as i64,
                call.duration.map(|d| d as i64),
            ],
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_call(&self, call_id: &str) -> Result<Option<StoredCall>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT call_id, chat_jid, caller_jid, is_group, group_jid, is_video, status, timestamp, duration
             FROM call_logs WHERE call_id = ?1 LIMIT 1"
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        let mut rows = stmt
            .query(params![call_id])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let is_group_int: i32 = row
                .get(3)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let is_video_int: i32 = row
                .get(5)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let ts: i64 = row
                .get(7)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            let dur: Option<i64> = row
                .get(8)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            Ok(Some(StoredCall {
                call_id: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                chat_jid: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                caller_jid: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                is_group: is_group_int != 0,
                group_jid: row
                    .get(4)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                is_video: is_video_int != 0,
                status: row
                    .get(6)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                timestamp: ts as u64,
                duration: dur.map(|d| d as u32),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_call_history(&self, limit: u32) -> Result<Vec<StoredCall>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT call_id, chat_jid, caller_jid, is_group, group_jid, is_video, status, timestamp, duration
             FROM call_logs ORDER BY timestamp DESC LIMIT ?1"
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let is_group_int: i32 = row.get(3)?;
                let is_video_int: i32 = row.get(5)?;
                let ts: i64 = row.get(7)?;
                let dur: Option<i64> = row.get(8)?;
                Ok(StoredCall {
                    call_id: row.get(0)?,
                    chat_jid: row.get(1)?,
                    caller_jid: row.get(2)?,
                    is_group: is_group_int != 0,
                    group_jid: row.get(4)?,
                    is_video: is_video_int != 0,
                    status: row.get(6)?,
                    timestamp: ts as u64,
                    duration: dur.map(|d| d as u32),
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn save_story(&self, story: &StoredStory) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO status_stories (
                msg_id, author_jid, author_name, timestamp, message_type, content, caption, has_media, viewed
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                story.msg_id,
                story.author_jid,
                story.author_name,
                story.timestamp as i64,
                story.message_type,
                story.content,
                story.caption,
                if story.has_media { 1 } else { 0 },
                if story.viewed { 1 } else { 0 },
            ],
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_stories(
        &self,
        limit: u32,
        unviewed_only: bool,
    ) -> Result<Vec<StoredStory>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let sql = if unviewed_only {
            "SELECT msg_id, author_jid, author_name, timestamp, message_type, content, caption, has_media, viewed
             FROM status_stories WHERE viewed = 0 ORDER BY timestamp DESC LIMIT ?1"
        } else {
            "SELECT msg_id, author_jid, author_name, timestamp, message_type, content, caption, has_media, viewed
             FROM status_stories ORDER BY timestamp DESC LIMIT ?1"
        };
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let ts: i64 = row.get(3)?;
                let has_media_int: i32 = row.get(7)?;
                let viewed_int: i32 = row.get(8)?;
                Ok(StoredStory {
                    msg_id: row.get(0)?,
                    author_jid: row.get(1)?,
                    author_name: row.get(2)?,
                    timestamp: ts as u64,
                    message_type: row.get(4)?,
                    content: row.get(5)?,
                    caption: row.get(6)?,
                    has_media: has_media_int != 0,
                    viewed: viewed_int != 0,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_stories_by_author(
        &self,
        author_jid: &str,
    ) -> Result<Vec<StoredStory>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT msg_id, author_jid, author_name, timestamp, message_type, content, caption, has_media, viewed
             FROM status_stories WHERE author_jid = ?1 ORDER BY timestamp DESC"
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![author_jid], |row| {
                let ts: i64 = row.get(3)?;
                let has_media_int: i32 = row.get(7)?;
                let viewed_int: i32 = row.get(8)?;
                Ok(StoredStory {
                    msg_id: row.get(0)?,
                    author_jid: row.get(1)?,
                    author_name: row.get(2)?,
                    timestamp: ts as u64,
                    message_type: row.get(4)?,
                    content: row.get(5)?,
                    caption: row.get(6)?,
                    has_media: has_media_int != 0,
                    viewed: viewed_int != 0,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn mark_story_viewed(&self, msg_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE status_stories SET viewed = 1 WHERE msg_id = ?1",
            params![msg_id],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn save_blocked_contact(&self, jid: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO blocked_contacts (jid, blocked_at) VALUES (?1, ?2)
             ON CONFLICT(jid) DO UPDATE SET blocked_at = ?2",
            params![jid, now],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn remove_blocked_contact(&self, jid: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM blocked_contacts WHERE jid = ?1", params![jid])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_blocked_contacts(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT jid FROM blocked_contacts ORDER BY blocked_at ASC")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn set_blocked_contacts(&self, jids: &[String]) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction()
            .map_err(|e| StoreError::Database(e.to_string()))?;
        tx.execute("DELETE FROM blocked_contacts", [])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        {
            let mut stmt = tx
                .prepare("INSERT INTO blocked_contacts (jid, blocked_at) VALUES (?1, ?2)")
                .map_err(|e| StoreError::Database(e.to_string()))?;
            for (idx, jid) in jids.iter().enumerate() {
                stmt.execute(params![jid, now + idx as i64])
                    .map_err(|e| StoreError::Database(e.to_string()))?;
            }
        }
        tx.commit()
            .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    // ==========================================
    // Track 9: Labels & WhatsApp Business Protocol
    // ==========================================
    async fn upsert_label(&self, label: &StoredLabel) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO labels (id, name, color, predefined_id, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                color = excluded.color,
                predefined_id = excluded.predefined_id,
                deleted = excluded.deleted",
            params![
                label.id,
                label.name,
                label.color,
                label.predefined_id,
                if label.deleted { 1 } else { 0 },
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_labels(&self) -> Result<Vec<StoredLabel>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, color, predefined_id, deleted FROM labels WHERE deleted = 0 ORDER BY id ASC")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![], |row| {
                let deleted_int: i32 = row.get(4)?;
                Ok(StoredLabel {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    predefined_id: row.get(3)?,
                    deleted: deleted_int != 0,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_label(&self, id: &str) -> Result<Option<StoredLabel>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, color, predefined_id, deleted FROM labels WHERE id = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        if let Some(row) = rows
            .next()
            .map_err(|e| StoreError::Database(e.to_string()))?
        {
            let deleted_int: i32 = row
                .get(4)
                .map_err(|e| StoreError::Database(e.to_string()))?;
            Ok(Some(StoredLabel {
                id: row
                    .get(0)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                name: row
                    .get(1)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                color: row
                    .get(2)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                predefined_id: row
                    .get(3)
                    .map_err(|e| StoreError::Database(e.to_string()))?,
                deleted: deleted_int != 0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete_label(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE labels SET deleted = 1 WHERE id = ?1", params![id])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let _ = conn.execute("DELETE FROM chat_labels WHERE label_id = ?1", params![id]);
        let _ = conn.execute(
            "DELETE FROM message_labels WHERE label_id = ?1",
            params![id],
        );
        Ok(())
    }

    async fn add_chat_label(&self, chat_jid: &str, label_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO chat_labels (chat_jid, label_id) VALUES (?1, ?2)",
            params![chat_jid, label_id],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn remove_chat_label(&self, chat_jid: &str, label_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM chat_labels WHERE chat_jid = ?1 AND label_id = ?2",
            params![chat_jid, label_id],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_chat_labels(&self, chat_jid: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT label_id FROM chat_labels WHERE chat_jid = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![chat_jid], |row| row.get(0))
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_chats_by_label(&self, label_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT chat_jid FROM chat_labels WHERE label_id = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![label_id], |row| row.get(0))
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn add_message_label(
        &self,
        chat_jid: &str,
        message_id: &str,
        label_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO message_labels (chat_jid, message_id, label_id) VALUES (?1, ?2, ?3)",
            params![chat_jid, message_id, label_id],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn remove_message_label(
        &self,
        chat_jid: &str,
        message_id: &str,
        label_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM message_labels WHERE chat_jid = ?1 AND message_id = ?2 AND label_id = ?3",
            params![chat_jid, message_id, label_id],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_message_labels(
        &self,
        chat_jid: &str,
        message_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT label_id FROM message_labels WHERE chat_jid = ?1 AND message_id = ?2")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![chat_jid, message_id], |row| row.get(0))
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_messages_by_label(&self, label_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT message_id FROM message_labels WHERE label_id = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![label_id], |row| row.get(0))
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn save_catalog_product(&self, product: &StoredCatalogProduct) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let images_json =
            serde_json::to_string(&product.image_urls).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT OR REPLACE INTO catalog_products (
                product_id, business_owner_jid, title, description, currency,
                price_amount_1000, sale_price_amount_1000, retailer_id, url,
                image_urls, is_hidden, availability, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                product.product_id,
                product.business_owner_jid,
                product.title,
                product.description,
                product.currency,
                product.price_amount_1000,
                product.sale_price_amount_1000,
                product.retailer_id,
                product.url,
                images_json,
                if product.is_hidden { 1 } else { 0 },
                product.availability,
                product.updated_at as i64,
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_catalog_products(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<StoredCatalogProduct>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT product_id, business_owner_jid, title, description, currency,
                        price_amount_1000, sale_price_amount_1000, retailer_id, url,
                        image_urls, is_hidden, availability, updated_at
                 FROM catalog_products
                 WHERE business_owner_jid = ?1
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![biz_jid], |row| {
                let img_json: Option<String> = row.get(9)?;
                let image_urls: Vec<String> = img_json
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default();
                let is_hidden_int: i32 = row.get(10)?;
                let updated_at_i64: i64 = row.get(12)?;

                Ok(StoredCatalogProduct {
                    product_id: row.get(0)?,
                    business_owner_jid: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    currency: row.get(4)?,
                    price_amount_1000: row.get(5)?,
                    sale_price_amount_1000: row.get(6)?,
                    retailer_id: row.get(7)?,
                    url: row.get(8)?,
                    image_urls,
                    is_hidden: is_hidden_int != 0,
                    availability: row.get(11)?,
                    updated_at: updated_at_i64 as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn get_catalog_product(
        &self,
        product_id: &str,
    ) -> Result<Option<StoredCatalogProduct>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT product_id, business_owner_jid, title, description, currency,
                        price_amount_1000, sale_price_amount_1000, retailer_id, url,
                        image_urls, is_hidden, availability, updated_at
                 FROM catalog_products
                 WHERE product_id = ?1",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![product_id], |row| {
                let img_json: Option<String> = row.get(9)?;
                let image_urls: Vec<String> = img_json
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default();
                let is_hidden_int: i32 = row.get(10)?;
                let updated_at_i64: i64 = row.get(12)?;

                Ok(StoredCatalogProduct {
                    product_id: row.get(0)?,
                    business_owner_jid: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    currency: row.get(4)?,
                    price_amount_1000: row.get(5)?,
                    sale_price_amount_1000: row.get(6)?,
                    retailer_id: row.get(7)?,
                    url: row.get(8)?,
                    image_urls,
                    is_hidden: is_hidden_int != 0,
                    availability: row.get(11)?,
                    updated_at: updated_at_i64 as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Database(e.to_string()))?)),
            None => Ok(None),
        }
    }

    async fn save_catalog_collection(
        &self,
        collection: &StoredCatalogCollection,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO catalog_collections (
                collection_id, business_owner_jid, name, products_count, status, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                collection.collection_id,
                collection.business_owner_jid,
                collection.name,
                collection.products_count as i64,
                collection.status,
                collection.updated_at as i64,
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_catalog_collections(
        &self,
        biz_jid: &str,
    ) -> Result<Vec<StoredCatalogCollection>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT collection_id, business_owner_jid, name, products_count, status, updated_at
                 FROM catalog_collections
                 WHERE business_owner_jid = ?1
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![biz_jid], |row| {
                let cnt: i64 = row.get(3)?;
                let updated_at_i64: i64 = row.get(5)?;
                Ok(StoredCatalogCollection {
                    collection_id: row.get(0)?,
                    business_owner_jid: row.get(1)?,
                    name: row.get(2)?,
                    products_count: cnt as u32,
                    status: row.get(4)?,
                    updated_at: updated_at_i64 as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn save_order(&self, order: &StoredOrder) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO orders (
                order_id, chat_jid, seller_jid, item_count, status, surface,
                order_title, message, total_amount_1000, total_currency_code,
                token, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                order.order_id,
                order.chat_jid,
                order.seller_jid,
                order.item_count,
                order.status,
                order.surface,
                order.order_title,
                order.message,
                order.total_amount_1000,
                order.total_currency_code,
                order.token,
                order.created_at as i64,
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_order(&self, order_id: &str) -> Result<Option<StoredOrder>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT order_id, chat_jid, seller_jid, item_count, status, surface,
                        order_title, message, total_amount_1000, total_currency_code,
                        token, created_at
                 FROM orders
                 WHERE order_id = ?1",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut rows = stmt
            .query_map(params![order_id], |row| {
                let created_at_i64: i64 = row.get(11)?;
                Ok(StoredOrder {
                    order_id: row.get(0)?,
                    chat_jid: row.get(1)?,
                    seller_jid: row.get(2)?,
                    item_count: row.get(3)?,
                    status: row.get(4)?,
                    surface: row.get(5)?,
                    order_title: row.get(6)?,
                    message: row.get(7)?,
                    total_amount_1000: row.get(8)?,
                    total_currency_code: row.get(9)?,
                    token: row.get(10)?,
                    created_at: created_at_i64 as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Database(e.to_string()))?)),
            None => Ok(None),
        }
    }

    async fn get_orders(&self, chat_jid: &str, limit: u32) -> Result<Vec<StoredOrder>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT order_id, chat_jid, seller_jid, item_count, status, surface,
                        order_title, message, total_amount_1000, total_currency_code,
                        token, created_at
                 FROM orders
                 WHERE chat_jid = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![chat_jid, limit as i64], |row| {
                let created_at_i64: i64 = row.get(11)?;
                Ok(StoredOrder {
                    order_id: row.get(0)?,
                    chat_jid: row.get(1)?,
                    seller_jid: row.get(2)?,
                    item_count: row.get(3)?,
                    status: row.get(4)?,
                    surface: row.get(5)?,
                    order_title: row.get(6)?,
                    message: row.get(7)?,
                    total_amount_1000: row.get(8)?,
                    total_currency_code: row.get(9)?,
                    token: row.get(10)?,
                    created_at: created_at_i64 as u64,
                })
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    // ==========================================
    // Track 13: Multi-Device Companions & Devices
    // ==========================================
    async fn save_user_devices(
        &self,
        user_jid: &str,
        device_ids: &[u32],
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let tx = conn
            .unchecked_transaction()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        tx.execute(
            "DELETE FROM user_devices WHERE user_jid = ?1",
            params![user_jid],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO user_devices (user_jid, device_id, key_index, updated_at)
                 VALUES (?1, ?2, 0, ?3)",
                )
                .map_err(|e| StoreError::Database(e.to_string()))?;

            for &dev in device_ids {
                stmt.execute(params![user_jid, dev as i64, now])
                    .map_err(|e| StoreError::Database(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_user_devices(&self, user_jid: &str) -> Result<Vec<u32>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT device_id FROM user_devices WHERE user_jid = ?1 ORDER BY device_id ASC",
            )
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![user_jid], |row| {
                let dev: i64 = row.get(0)?;
                Ok(dev as u32)
            })
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| StoreError::Database(e.to_string()))?);
        }
        Ok(list)
    }

    async fn remove_user_device(&self, user_jid: &str, device_id: u32) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM user_devices WHERE user_jid = ?1 AND device_id = ?2",
            params![user_jid, device_id as i64],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    // ==========================================
    // KV Store & Media Download Tracking
    // ==========================================
    async fn set_kv(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO kv_store (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now],
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_kv(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT value FROM kv_store WHERE key = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        match stmt.query_row(params![key], |row| row.get(0)) {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Database(e.to_string())),
        }
    }

    async fn delete_kv(&self, key: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM kv_store WHERE key = ?1", params![key])
            .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_kv_prefix(&self, prefix: &str) -> Result<HashMap<String, String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("{}%", prefix);
        let mut stmt = conn
            .prepare("SELECT key, value FROM kv_store WHERE key LIKE ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![pattern], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut res = HashMap::new();
        for r in rows.flatten() {
            res.insert(r.0, r.1);
        }
        Ok(res)
    }

    async fn record_media_download(
        &self,
        message_id: &str,
        chat_jid: &str,
        file_path: &str,
        file_size: u64,
        mimetype: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO media_downloads (message_id, chat_jid, file_path, file_size, mimetype, downloaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(message_id) DO UPDATE SET file_path = excluded.file_path, file_size = excluded.file_size, mimetype = excluded.mimetype, downloaded_at = excluded.downloaded_at",
            params![message_id, chat_jid, file_path, file_size as i64, mimetype, now],
        ).map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_media_download(&self, message_id: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT file_path FROM media_downloads WHERE message_id = ?1")
            .map_err(|e| StoreError::Database(e.to_string()))?;
        match stmt.query_row(params![message_id], |row| row.get(0)) {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Database(e.to_string())),
        }
    }
}
