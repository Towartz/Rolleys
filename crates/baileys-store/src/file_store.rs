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

    async fn get_key_count(&self, key_type: &str) -> Result<usize, StoreError> {
        Ok(self.get_all_keys(key_type).await?.len())
    }

    async fn get_all_keys(&self, key_type: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let prefix = Self::fix_file_name(&format!("{key_type}-"));
        let mut dir = fs::read_dir(&self.folder)
            .await
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| StoreError::Io(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".json"))
            else {
                continue;
            };
            let data = fs::read(entry.path())
                .await
                .map_err(|e| StoreError::Io(e.to_string()))?;
            // `/` was stored as `__`; `:` became `-` and cannot be told apart from a real `-`, but
            // the same file name results when the id is used again, so lookups and removal work.
            out.push((id.replace("__", "/"), data));
        }
        Ok(out)
    }
}

#[async_trait]
impl ChatStore for FileStore {}

#[async_trait]
impl NewsletterStore for FileStore {}

#[async_trait]
impl AppStateStore for FileStore {}

#[cfg(test)]
mod key_listing_tests {
    use super::*;

    #[tokio::test]
    async fn lists_counts_and_removes_keys_of_one_type() {
        let dir = std::env::temp_dir().join(format!("rolleys-filestore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileStore::new(&dir).await.unwrap();
        let entry = |t: &str, id: &str, d: &[u8]| KeyEntry {
            key_type: t.into(),
            id: id.into(),
            data: d.to_vec(),
        };
        store
            .set_keys(vec![
                entry("pre-key", "1", b"a"),
                entry("pre-key", "2", b"b"),
                entry("session", "628111:5@s.whatsapp.net", b"s"),
                entry("app-state-sync-key", "AAAA/BBB=", b"k"),
            ])
            .await
            .unwrap();

        assert_eq!(store.get_key_count("pre-key").await.unwrap(), 2);
        assert_eq!(store.get_key_count("session").await.unwrap(), 1);
        assert_eq!(store.get_key_count("sender-key").await.unwrap(), 0);

        let mut pre = store.get_all_keys("pre-key").await.unwrap();
        pre.sort();
        assert_eq!(
            pre,
            vec![
                ("1".to_string(), b"a".to_vec()),
                ("2".to_string(), b"b".to_vec())
            ]
        );
        // `/` survives the round trip through the file name.
        assert_eq!(
            store.get_all_keys("app-state-sync-key").await.unwrap()[0].0,
            "AAAA/BBB="
        );

        // Ids returned by the listing can be used to remove, even when ':' was folded to '-'.
        let sessions = store.get_all_keys("session").await.unwrap();
        let ids: Vec<String> = sessions.into_iter().map(|(id, _)| id).collect();
        store.remove_keys("session", &ids).await.unwrap();
        assert_eq!(store.get_key_count("session").await.unwrap(), 0);
        assert_eq!(
            store.get_key_count("pre-key").await.unwrap(),
            2,
            "other types untouched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
