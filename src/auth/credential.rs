use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Per-account credential data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Persistent store for account credentials.
///
/// Layout:
/// ```text
/// base_dir/
///   accounts/
///     abc@im.bot/
///       credential.json    # AccountData
///       sync_buf.txt       # message cursor
///       downloads/         # media files
/// ```
pub struct CredentialStore {
    base_dir: PathBuf,
}

impl CredentialStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn accounts_dir(&self) -> PathBuf {
        self.base_dir.join("accounts")
    }

    /// Resolve the directory for a specific account.
    pub fn account_dir(&self, id: &str) -> PathBuf {
        self.accounts_dir().join(id)
    }

    fn credential_path(&self, id: &str) -> PathBuf {
        self.account_dir(id).join("credential.json")
    }

    /// Resolve the sync_buf file path for an account.
    pub fn sync_buf_path(&self, id: &str) -> PathBuf {
        self.account_dir(id).join("sync_buf.txt")
    }

    /// Resolve the downloads directory for an account.
    pub fn downloads_dir(&self, id: &str) -> PathBuf {
        self.account_dir(id).join("downloads")
    }

    /// List all account IDs (scans subdirectories under `accounts/`).
    pub fn list_accounts(&self) -> Result<Vec<String>> {
        let dir = self.accounts_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Only include dirs that have a credential.json
                    if self.credential_path(name).exists() {
                        ids.push(name.to_string());
                    }
                }
            }
        }
        Ok(ids)
    }

    /// Load account data by ID.
    pub fn load_account(&self, id: &str) -> Result<Option<AccountData>> {
        let path = self.credential_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)?;
        let data: AccountData = serde_json::from_str(&raw)?;
        Ok(Some(data))
    }

    /// Save account data.
    pub fn save_account(&self, id: &str, data: &AccountData) -> Result<()> {
        let dir = self.account_dir(id);
        fs::create_dir_all(&dir)?;

        let path = self.credential_path(id);
        let json = serde_json::to_string_pretty(data)?;
        fs::write(&path, &json)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    /// Remove an account and all its data.
    pub fn remove_account(&self, id: &str) -> Result<()> {
        let dir = self.account_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path());

        assert!(store.list_accounts().unwrap().is_empty());

        let data = AccountData {
            token: Some("tok123".into()),
            saved_at: Some(Utc::now()),
            base_url: Some("https://example.com".into()),
            user_id: Some("user1".into()),
        };
        store.save_account("abc@im.bot", &data).unwrap();

        let ids = store.list_accounts().unwrap();
        assert_eq!(ids, vec!["abc@im.bot"]);

        // Verify directory structure
        assert!(store.account_dir("abc@im.bot").exists());
        assert!(store.credential_path("abc@im.bot").exists());
        assert_eq!(
            store.sync_buf_path("abc@im.bot"),
            dir.path().join("accounts/abc@im.bot/sync_buf.txt")
        );

        let loaded = store.load_account("abc@im.bot").unwrap().unwrap();
        assert_eq!(loaded.token.as_deref(), Some("tok123"));

        store.remove_account("abc@im.bot").unwrap();
        assert!(store.list_accounts().unwrap().is_empty());
        assert!(!store.account_dir("abc@im.bot").exists());
    }
}
