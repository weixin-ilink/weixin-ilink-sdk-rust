use libsql::{Builder, Connection, Database};

use crate::error::{Error, Result};

/// Storage backend for SDK state (accounts, sync cursors, context tokens).
///
/// Supports three modes:
/// - **Local**: `Store::open_local("path/to/ilink.db")` — embedded SQLite file
/// - **Remote**: `Store::open_remote(url, token)` — Turso cloud database
/// - **Replica**: `Store::open_replica("local.db", url, token)` — local replica synced with Turso
pub struct Store {
    db: Database,
    conn: Connection,
}

impl Store {
    /// Open a local embedded database.
    pub async fn open_local(path: &str) -> Result<Self> {
        let db = Builder::new_local(path)
            .build()
            .await
            .map_err(|e| Error::Other(format!("failed to open local db: {e}")))?;
        let conn = db
            .connect()
            .map_err(|e| Error::Other(format!("failed to connect: {e}")))?;
        let store = Self { db, conn };
        store.migrate().await?;
        Ok(store)
    }

    /// Connect to a remote Turso database.
    pub async fn open_remote(url: &str, token: &str) -> Result<Self> {
        let db = Builder::new_remote(url.to_string(), token.to_string())
            .build()
            .await
            .map_err(|e| Error::Other(format!("failed to open remote db: {e}")))?;
        let conn = db
            .connect()
            .map_err(|e| Error::Other(format!("failed to connect: {e}")))?;
        let store = Self { db, conn };
        store.migrate().await?;
        Ok(store)
    }

    /// Open a local replica that syncs with a remote Turso database.
    pub async fn open_replica(path: &str, url: &str, token: &str) -> Result<Self> {
        let db = Builder::new_remote_replica(path, url.to_string(), token.to_string())
            .build()
            .await
            .map_err(|e| Error::Other(format!("failed to open replica db: {e}")))?;
        let conn = db
            .connect()
            .map_err(|e| Error::Other(format!("failed to connect: {e}")))?;
        let store = Self { db, conn };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS accounts (
                    ilink_bot_id TEXT PRIMARY KEY,
                    token        TEXT NOT NULL,
                    base_url     TEXT,
                    user_id      TEXT,
                    saved_at     TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS sync_state (
                    ilink_bot_id    TEXT PRIMARY KEY,
                    get_updates_buf TEXT NOT NULL,
                    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS context_tokens (
                    ilink_bot_id TEXT NOT NULL,
                    user_id      TEXT NOT NULL,
                    token        TEXT NOT NULL,
                    updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
                    PRIMARY KEY (ilink_bot_id, user_id)
                );
                ",
            )
            .await
            .map_err(|e| Error::Other(format!("migration failed: {e}")))?;
        Ok(())
    }

    // ── Accounts ────────────────────────────────────────────────────────

    /// List all account IDs.
    pub async fn list_accounts(&self) -> Result<Vec<String>> {
        let mut rows = self
            .conn
            .query("SELECT ilink_bot_id FROM accounts", ())
            .await
            .map_err(|e| Error::Other(format!("query failed: {e}")))?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| Error::Other(e.to_string()))? {
            let id: String = row.get(0).map_err(|e| Error::Other(e.to_string()))?;
            ids.push(id);
        }
        Ok(ids)
    }

    /// Load account credentials.
    pub async fn load_account(&self, ilink_bot_id: &str) -> Result<Option<AccountRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT token, base_url, user_id, saved_at FROM accounts WHERE ilink_bot_id = ?1",
                [ilink_bot_id],
            )
            .await
            .map_err(|e| Error::Other(format!("query failed: {e}")))?;

        match rows.next().await.map_err(|e| Error::Other(e.to_string()))? {
            Some(row) => Ok(Some(AccountRow {
                ilink_bot_id: ilink_bot_id.to_string(),
                token: row.get(0).map_err(|e| Error::Other(e.to_string()))?,
                base_url: row.get(1).map_err(|e| Error::Other(e.to_string()))?,
                user_id: row.get(2).map_err(|e| Error::Other(e.to_string()))?,
                saved_at: row.get(3).map_err(|e| Error::Other(e.to_string()))?,
            })),
            None => Ok(None),
        }
    }

    /// Save or update account credentials.
    pub async fn save_account(
        &self,
        ilink_bot_id: &str,
        token: &str,
        base_url: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO accounts (ilink_bot_id, token, base_url, user_id, saved_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(ilink_bot_id) DO UPDATE SET
                     token = excluded.token,
                     base_url = excluded.base_url,
                     user_id = excluded.user_id,
                     saved_at = excluded.saved_at",
                libsql::params![ilink_bot_id, token, base_url, user_id],
            )
            .await
            .map_err(|e| Error::Other(format!("save account failed: {e}")))?;
        Ok(())
    }

    /// Remove an account and all related data.
    pub async fn remove_account(&self, ilink_bot_id: &str) -> Result<()> {
        self.conn
            .execute_batch(&format!(
                "DELETE FROM context_tokens WHERE ilink_bot_id = '{ilink_bot_id}';
                 DELETE FROM sync_state WHERE ilink_bot_id = '{ilink_bot_id}';
                 DELETE FROM accounts WHERE ilink_bot_id = '{ilink_bot_id}';"
            ))
            .await
            .map_err(|e| Error::Other(format!("remove account failed: {e}")))?;
        Ok(())
    }

    // ── Sync State ──────────────────────────────────────────────────────

    /// Load the sync cursor for an account.
    pub async fn load_sync_buf(&self, ilink_bot_id: &str) -> Result<Option<SyncStateRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT get_updates_buf, updated_at FROM sync_state WHERE ilink_bot_id = ?1",
                [ilink_bot_id],
            )
            .await
            .map_err(|e| Error::Other(format!("query failed: {e}")))?;

        match rows.next().await.map_err(|e| Error::Other(e.to_string()))? {
            Some(row) => Ok(Some(SyncStateRow {
                get_updates_buf: row.get(0).map_err(|e| Error::Other(e.to_string()))?,
                updated_at: row.get(1).map_err(|e| Error::Other(e.to_string()))?,
            })),
            None => Ok(None),
        }
    }

    /// Save the sync cursor for an account.
    pub async fn save_sync_buf(&self, ilink_bot_id: &str, buf: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO sync_state (ilink_bot_id, get_updates_buf, updated_at)
                 VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(ilink_bot_id) DO UPDATE SET
                     get_updates_buf = excluded.get_updates_buf,
                     updated_at = excluded.updated_at",
                libsql::params![ilink_bot_id, buf],
            )
            .await
            .map_err(|e| Error::Other(format!("save sync buf failed: {e}")))?;
        Ok(())
    }

    // ── Context Tokens ──────────────────────────────────────────────────

    /// Load a cached context token.
    pub async fn load_context_token(
        &self,
        ilink_bot_id: &str,
        user_id: &str,
    ) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT token FROM context_tokens WHERE ilink_bot_id = ?1 AND user_id = ?2",
                libsql::params![ilink_bot_id, user_id],
            )
            .await
            .map_err(|e| Error::Other(format!("query failed: {e}")))?;

        match rows.next().await.map_err(|e| Error::Other(e.to_string()))? {
            Some(row) => {
                let token: String = row.get(0).map_err(|e| Error::Other(e.to_string()))?;
                Ok(Some(token))
            }
            None => Ok(None),
        }
    }

    /// Save or update a context token.
    pub async fn save_context_token(
        &self,
        ilink_bot_id: &str,
        user_id: &str,
        token: &str,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO context_tokens (ilink_bot_id, user_id, token, updated_at)
                 VALUES (?1, ?2, ?3, datetime('now'))
                 ON CONFLICT(ilink_bot_id, user_id) DO UPDATE SET
                     token = excluded.token,
                     updated_at = excluded.updated_at",
                libsql::params![ilink_bot_id, user_id, token],
            )
            .await
            .map_err(|e| Error::Other(format!("save context token failed: {e}")))?;
        Ok(())
    }

    /// Sync with remote (for replica mode). No-op for local/remote-only.
    pub async fn sync(&self) -> Result<()> {
        // Database::sync() is only available on replicas; ignore errors for other modes.
        let _ = self.db.sync().await;
        Ok(())
    }
}

/// Account row from the database.
#[derive(Debug, Clone)]
pub struct AccountRow {
    pub ilink_bot_id: String,
    pub token: String,
    pub base_url: Option<String>,
    pub user_id: Option<String>,
    pub saved_at: String,
}

/// Sync state row from the database.
#[derive(Debug, Clone)]
pub struct SyncStateRow {
    pub get_updates_buf: String,
    pub updated_at: String,
}
