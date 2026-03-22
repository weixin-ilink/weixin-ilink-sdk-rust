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
///   accounts.json          # ["account-id-1", "account-id-2"]
///   accounts/
///     account-id-1.json    # AccountData
///     account-id-2.json
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

    fn index_path(&self) -> PathBuf {
        self.base_dir.join("accounts.json")
    }

    fn account_path(&self, id: &str) -> PathBuf {
        self.accounts_dir().join(format!("{id}.json"))
    }

    /// List all registered account IDs.
    pub fn list_accounts(&self) -> Result<Vec<String>> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)?;
        let ids: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
        Ok(ids)
    }

    /// Load account data by ID.
    pub fn load_account(&self, id: &str) -> Result<Option<AccountData>> {
        let path = self.account_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)?;
        let data: AccountData = serde_json::from_str(&raw)?;
        Ok(Some(data))
    }

    /// Save account data (merges with existing).
    pub fn save_account(&self, id: &str, data: &AccountData) -> Result<()> {
        let dir = self.accounts_dir();
        fs::create_dir_all(&dir)?;

        let path = self.account_path(id);
        let json = serde_json::to_string_pretty(data)?;
        fs::write(&path, &json)?;

        // Best-effort: restrict file permissions on unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }

        // Register in index.
        self.register_account_id(id)?;
        Ok(())
    }

    /// Remove account data.
    pub fn remove_account(&self, id: &str) -> Result<()> {
        let path = self.account_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }

        // Remove from index.
        let mut ids = self.list_accounts()?;
        ids.retain(|i| i != id);
        self.write_index(&ids)?;
        Ok(())
    }

    fn register_account_id(&self, id: &str) -> Result<()> {
        let mut ids = self.list_accounts()?;
        if ids.iter().any(|i| i == id) {
            return Ok(());
        }
        ids.push(id.to_string());
        self.write_index(&ids)
    }

    fn write_index(&self, ids: &[String]) -> Result<()> {
        fs::create_dir_all(&self.base_dir)?;
        let json = serde_json::to_string_pretty(ids)?;
        fs::write(self.index_path(), json)?;
        Ok(())
    }
}

/// Normalize an account ID to a filesystem-safe string.
///
/// e.g. `"b0f5860fdecb@im.bot"` → `"b0f5860fdecb-im-bot"`
pub fn normalize_account_id(raw: &str) -> String {
    raw.replace('@', "-").replace('.', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ids() {
        assert_eq!(normalize_account_id("abc@im.bot"), "abc-im-bot");
        assert_eq!(normalize_account_id("abc@im.wechat"), "abc-im-wechat");
        assert_eq!(normalize_account_id("plain-id"), "plain-id");
    }

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
        store.save_account("test-id", &data).unwrap();

        let ids = store.list_accounts().unwrap();
        assert_eq!(ids, vec!["test-id"]);

        let loaded = store.load_account("test-id").unwrap().unwrap();
        assert_eq!(loaded.token.as_deref(), Some("tok123"));

        store.remove_account("test-id").unwrap();
        assert!(store.list_accounts().unwrap().is_empty());
        assert!(store.load_account("test-id").unwrap().is_none());
    }
}
