use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::traits::{
    AppStateStore, AuthCreds, ChatStore, KeyEntry, NewsletterStore, SessionStore, SignalKeysStore,
};
use crate::StoreError;

/// Storage implementation reading and writing JSON files on disk, compatible with Baileys `useMultiFileAuthState`.
pub struct FileStore {
    folder: PathBuf,
}

impl FileStore {
    pub async fn new(folder: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = folder.as_ref().to_path_buf();
        if !path.exists() {
            fs::create_dir_all(&path)
                .await
                .map_err(|e| StoreError::Io(e.to_string()))?;
        }
        Ok(Self { folder: path })
    }

    fn fix_file_name(file: &str) -> String {
        file.replace('/', "__").replace(':', "-")
    }

    fn key_file_name(key_type: &str, id: &str) -> String {
        Self::fix_file_name(&format!("{}-{}.json", key_type, id))
    }
}

#[async_trait]
impl SessionStore for FileStore {
    async fn get_creds(&self) -> Result<Option<AuthCreds>, StoreError> {
        let creds_path = self.folder.join("creds.json");
        if !creds_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&creds_path)
            .await
            .map_err(|e| StoreError::Io(e.to_string()))?;

        let creds: AuthCreds =
            serde_json::from_str(&content).map_err(|e| StoreError::Serialization(e.to_string()))?;

        Ok(Some(creds))
    }

    async fn save_creds(&self, creds: &AuthCreds) -> Result<(), StoreError> {
        let creds_path = self.folder.join("creds.json");
        let json = serde_json::to_string_pretty(creds)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        fs::write(&creds_path, json)
            .await
            .map_err(|e| StoreError::Io(e.to_string()))?;

        Ok(())
    }

    async fn clear(&self) -> Result<(), StoreError> {
        let mut read_dir = fs::read_dir(&self.folder)
            .await
            .map_err(|e| StoreError::Io(e.to_string()))?;

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let _ = fs::remove_file(entry.path()).await;
        }

        Ok(())
    }
}

#[async_trait]
impl SignalKeysStore for FileStore {
    async fn get_key(&self, key_type: &str, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.folder.join(Self::key_file_name(key_type, id));
        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read(&path)
            .await
            .map_err(|e| StoreError::Io(e.to_string()))?;

        Ok(Some(data))
    }

    async fn set_keys(&self, entries: Vec<KeyEntry>) -> Result<(), StoreError> {
        for entry in entries {
            let path = self
                .folder
                .join(Self::key_file_name(&entry.key_type, &entry.id));
            fs::write(&path, &entry.data)
                .await
                .map_err(|e| StoreError::Io(e.to_string()))?;
        }
        Ok(())
    }

    async fn remove_keys(&self, key_type: &str, ids: &[String]) -> Result<(), StoreError> {
        for id in ids {
            let path = self.folder.join(Self::key_file_name(key_type, id));
            if path.exists() {
                let _ = fs::remove_file(&path).await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ChatStore for FileStore {}

#[async_trait]
impl NewsletterStore for FileStore {}

#[async_trait]
impl AppStateStore for FileStore {}
