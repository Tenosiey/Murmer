//! Upload policy: the per-file size cap and the enabled file categories.
//!
//! Both values live in the generic `server_settings` key-value table (shared
//! with the stats toggle, screen share cap and server identity), so no schema
//! changes are needed. Owners/Admins edit them from the Server Dashboard; the
//! `/upload` endpoint reads them on every request, so a change takes effect
//! immediately without a restart.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};
use crate::upload::{DEFAULT_MAX_FILE_SIZE, default_category_ids, is_known_category};

/// `server_settings` key for the per-file upload cap in bytes.
const UPLOAD_MAX_BYTES_KEY: &str = "upload_max_bytes";

/// `server_settings` key for the comma-separated list of enabled category ids.
const UPLOAD_CATEGORIES_KEY: &str = "upload_categories";

/// The server-wide upload policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadConfig {
    /// Largest accepted file size in bytes.
    pub max_bytes: u64,
    /// Ids of the enabled upload categories (see [`crate::upload::UPLOAD_CATEGORIES`]).
    pub categories: Vec<String>,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_FILE_SIZE as u64,
            categories: default_category_ids(),
        }
    }
}

/// Parse a stored category list, dropping ids the build no longer knows so a
/// stale row can never widen the safe-list.
fn parse_categories(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty() && is_known_category(id))
        .map(str::to_string)
        .collect()
}

/// Load the upload policy, falling back to the defaults for missing or
/// unparseable settings.
pub async fn upload_config(db: &Db) -> Result<UploadConfig, DbError> {
    db.call_db(|conn| {
        let mut config = UploadConfig::default();

        let raw_max: Option<String> = conn
            .query_row(
                "SELECT value FROM server_settings WHERE key = ?1",
                params![UPLOAD_MAX_BYTES_KEY],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(bytes) = raw_max
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&v| v > 0)
        {
            config.max_bytes = bytes;
        }

        let raw_categories: Option<String> = conn
            .query_row(
                "SELECT value FROM server_settings WHERE key = ?1",
                params![UPLOAD_CATEGORIES_KEY],
                |row| row.get(0),
            )
            .optional()?;
        // An empty stored value is meaningful: every category is disabled.
        if let Some(raw) = raw_categories {
            config.categories = parse_categories(&raw);
        }

        Ok(config)
    })
    .await
}

/// Store the upload policy (Owner/Admin action, validated by the caller).
pub async fn set_upload_config(db: &Db, config: &UploadConfig) -> Result<(), DbError> {
    let max_bytes = config.max_bytes.to_string();
    let categories = config.categories.join(",");
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        for (key, value) in [
            (UPLOAD_MAX_BYTES_KEY, max_bytes),
            (UPLOAD_CATEGORIES_KEY, categories),
        ] {
            tx.execute(
                "INSERT INTO server_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await
}
