use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::traits::{
    AppStateStore, AuthCreds, ChatStore, KeyEntry, NewsletterStore, SessionStore, SignalKeysStore,
};
use crate::StoreError;

/// Ephemeral in-memory store, ideal for testing, ephemeral bots, and serverless workers.
#[derive(Debug, Default)]
pub struct MemoryStore {
    creds: RwLock<Option<AuthCreds>>,
    keys: RwLock<HashMap<(String, String), Vec<u8>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for MemoryStore {
    async fn get_creds(&self) -> Result<Option<AuthCreds>, StoreError> {
        let creds = self.creds.read().unwrap();
        Ok(creds.clone())
    }

    async fn save_creds(&self, creds: &AuthCreds) -> Result<(), StoreError> {
        let mut store = self.creds.write().unwrap();
        *store = Some(creds.clone());
        Ok(())
    }

    async fn clear(&self) -> Result<(), StoreError> {
        let mut creds = self.creds.write().unwrap();
        *creds = None;
        let mut keys = self.keys.write().unwrap();
        keys.clear();
        Ok(())
    }
}

#[async_trait]
impl SignalKeysStore for MemoryStore {
    async fn get_key(&self, key_type: &str, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let keys = self.keys.read().unwrap();
        Ok(keys.get(&(key_type.to_string(), id.to_string())).cloned())
    }

    async fn set_keys(&self, entries: Vec<KeyEntry>) -> Result<(), StoreError> {
        let mut keys = self.keys.write().unwrap();
        for entry in entries {
            keys.insert((entry.key_type, entry.id), entry.data);
        }
        Ok(())
    }

    async fn remove_keys(&self, key_type: &str, ids: &[String]) -> Result<(), StoreError> {
        let mut keys = self.keys.write().unwrap();
        for id in ids {
            keys.remove(&(key_type.to_string(), id.clone()));
        }
        Ok(())
    }

    async fn get_key_count(&self, key_type: &str) -> Result<usize, StoreError> {
        let keys = self.keys.read().unwrap();
        Ok(keys.keys().filter(|(t, _)| t == key_type).count())
    }

    async fn get_all_keys(&self, key_type: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let keys = self.keys.read().unwrap();
        Ok(keys
            .iter()
            .filter(|((t, _), _)| t == key_type)
            .map(|((_, id), data)| (id.clone(), data.clone()))
            .collect())
    }
}

#[async_trait]
impl ChatStore for MemoryStore {}

#[async_trait]
impl NewsletterStore for MemoryStore {}

#[async_trait]
impl AppStateStore for MemoryStore {}
