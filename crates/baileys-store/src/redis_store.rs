//! Redis-backed distributed storage engine for WhatsApp authentication, keys, and chats.
//!
//! Enables multiple stateless app instances or serverless workers to share
//! a single WhatsApp connection session by storing credentials, prekeys, and sessions in Redis.

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use crate::traits::{
    AppStateStore, AuthCreds, ChatStore, KeyEntry, NewsletterStore, SessionStore, SignalKeysStore,
};
use crate::StoreError;

/// Redis storage adapter for distributed session persistence.
#[derive(Clone)]
pub struct RedisStore {
    manager: ConnectionManager,
    prefix: String,
}

impl RedisStore {
    /// Creates a new `RedisStore` from a Redis connection URL (e.g. `redis://127.0.0.1:6379`).
    pub async fn new(redis_url: &str, session_prefix: Option<&str>) -> Result<Self, StoreError> {
        let client =
            redis::Client::open(redis_url).map_err(|e| StoreError::Redis(e.to_string()))?;
        let manager = ConnectionManager::new(client).await?;
        let prefix = session_prefix.unwrap_or("wa:session:").to_string();
        Ok(Self { manager, prefix })
    }

    /// Creates a `RedisStore` using an already-established `ConnectionManager`.
    pub fn from_manager(manager: ConnectionManager, prefix: impl Into<String>) -> Self {
        Self {
            manager,
            prefix: prefix.into(),
        }
    }

    pub fn creds_key_for(prefix: &str) -> String {
        format!("{}creds", prefix)
    }

    pub fn key_entry_key_for(prefix: &str, key_type: &str, id: &str) -> String {
        format!("{}keys:{}:{}", prefix, key_type, id)
    }

    pub fn key_pattern_for(prefix: &str, key_type: &str) -> String {
        format!("{}keys:{}:*", prefix, key_type)
    }

    pub fn creds_key(&self) -> String {
        Self::creds_key_for(&self.prefix)
    }

    pub fn key_entry_key(&self, key_type: &str, id: &str) -> String {
        Self::key_entry_key_for(&self.prefix, key_type, id)
    }

    pub fn key_pattern(&self, key_type: &str) -> String {
        Self::key_pattern_for(&self.prefix, key_type)
    }
}

#[async_trait]
impl SessionStore for RedisStore {
    async fn get_creds(&self) -> Result<Option<AuthCreds>, StoreError> {
        let mut conn = self.manager.clone();
        let json_str: Option<String> = conn.get(self.creds_key()).await?;
        match json_str {
            Some(s) => {
                let creds: AuthCreds = serde_json::from_str(&s)?;
                Ok(Some(creds))
            }
            None => Ok(None),
        }
    }

    async fn save_creds(&self, creds: &AuthCreds) -> Result<(), StoreError> {
        let json_str = serde_json::to_string(creds)?;
        let mut conn = self.manager.clone();
        conn.set::<_, _, ()>(self.creds_key(), json_str).await?;
        Ok(())
    }

    async fn clear(&self) -> Result<(), StoreError> {
        let mut conn = self.manager.clone();
        let pattern = format!("{}*", self.prefix);
        let keys: Vec<String> = conn.keys(&pattern).await?;
        if !keys.is_empty() {
            conn.del::<_, ()>(keys).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SignalKeysStore for RedisStore {
    async fn get_key(&self, key_type: &str, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let mut conn = self.manager.clone();
        let redis_key = self.key_entry_key(key_type, id);
        let val: Option<Vec<u8>> = conn.get(redis_key).await?;
        Ok(val)
    }

    async fn set_keys(&self, entries: Vec<KeyEntry>) -> Result<(), StoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut conn = self.manager.clone();
        let mut pipe = redis::pipe();
        for entry in entries {
            let redis_key = self.key_entry_key(&entry.key_type, &entry.id);
            pipe.set(redis_key, entry.data);
        }
        pipe.query_async::<()>(&mut conn).await?;
        Ok(())
    }

    async fn remove_keys(&self, key_type: &str, ids: &[String]) -> Result<(), StoreError> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.manager.clone();
        let redis_keys: Vec<String> = ids
            .iter()
            .map(|id| self.key_entry_key(key_type, id))
            .collect();
        conn.del::<_, ()>(redis_keys).await?;
        Ok(())
    }

    async fn get_key_count(&self, key_type: &str) -> Result<usize, StoreError> {
        let mut conn = self.manager.clone();
        let pattern = self.key_pattern(key_type);
        let keys: Vec<String> = conn.keys(pattern).await?;
        Ok(keys.len())
    }

    async fn get_all_keys(&self, key_type: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let mut conn = self.manager.clone();
        let pattern = self.key_pattern(key_type);
        let keys: Vec<String> = conn.keys(pattern).await?;
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let values: Vec<Option<Vec<u8>>> = conn.get(&keys).await?;
        let prefix_len = format!("{}keys:{}:", self.prefix, key_type).len();
        let mut result = Vec::with_capacity(keys.len());
        for (k, val_opt) in keys.into_iter().zip(values) {
            if let Some(val) = val_opt {
                let id = k[prefix_len..].to_string();
                result.push((id, val));
            }
        }
        Ok(result)
    }
}

#[async_trait]
impl ChatStore for RedisStore {}

#[async_trait]
impl NewsletterStore for RedisStore {}

#[async_trait]
impl AppStateStore for RedisStore {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_invalid_redis_url() {
        let res = RedisStore::new("invalid://url", None).await;
        assert!(res.is_err());
    }

    #[test]
    fn test_redis_key_generation() {
        let prefix = "wa:test:";
        assert_eq!(RedisStore::creds_key_for(prefix), "wa:test:creds");
        assert_eq!(
            RedisStore::key_entry_key_for(prefix, "pre-key", "42"),
            "wa:test:keys:pre-key:42"
        );
        assert_eq!(
            RedisStore::key_pattern_for(prefix, "session"),
            "wa:test:keys:session:*"
        );
    }
}
