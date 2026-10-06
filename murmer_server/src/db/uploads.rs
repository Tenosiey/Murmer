//! Upload policy and bookkeeping: the per-file size cap, the enabled file
//! categories, and who stored how many bytes.
//!
//! The policy lives in the generic `server_settings` key-value table (shared
//! with the stats toggle, screen share cap and server identity). Owners/Admins
//! edit it from the Server Dashboard; the `/upload` endpoint reads it on every
//! request, so a change takes effect immediately without a restart.
//!
//! The `uploads` table records each stored file's owner and size. Without it
//! there was no quota to enforce and nothing tied a file to the message that
//! carried it, so a deleted message's photo stayed downloadable forever. A
//! trigger marks a file `released` when a message naming it is deleted —
//! by its author, a moderator, retention, a purge or a reset alike — and
//! [`take_unreferenced_uploads`] hands back the released files nothing else
//! still names. Files inside encrypted messages are never released: the
//! server cannot see which file a sealed message carries.

use rusqlite::params;

use super::{Db, DbCall, DbError, read_setting, write_setting};
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

        if let Some(bytes) = read_setting(conn, UPLOAD_MAX_BYTES_KEY)?
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&v| v > 0)
        {
            config.max_bytes = bytes;
        }

        // An empty stored value is meaningful: every category is disabled.
        if let Some(raw) = read_setting(conn, UPLOAD_CATEGORIES_KEY)? {
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
            write_setting(&tx, key, &value)?;
        }
        tx.commit()?;
        Ok(())
    })
    .await
}

/// Schema for upload bookkeeping. Depends on `messages`.
///
/// Clients store the absolute URL (`https://host/files/<key>`), so the
/// trigger matches on what follows `/files/`. A message naming somebody
/// else's key can only release it; [`take_unreferenced_uploads`] still keeps
/// the file while their message names it.
pub(super) fn uploads_schema() -> &'static str {
    r#"CREATE TABLE IF NOT EXISTS uploads (
    key TEXT PRIMARY KEY,
    user_name TEXT NOT NULL,
    size INTEGER NOT NULL,
    released INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_uploads_user ON uploads (user_name);
CREATE INDEX IF NOT EXISTS idx_uploads_released ON uploads (released) WHERE released = 1;
CREATE TRIGGER IF NOT EXISTS messages_release_uploads AFTER DELETE ON messages
WHEN json_valid(old.content) BEGIN
    UPDATE uploads SET released = 1 WHERE key IN (
        SELECT substr(url, instr(url, '/files/') + 7) FROM (
            SELECT json_extract(old.content, '$.image') AS url
            UNION ALL SELECT json_extract(old.content, '$.attachment.url'))
        WHERE instr(url, '/files/') > 0);
END;
"#
}

/// Record `size` bytes stored under `key` for `user`, unless that would take
/// the user past `per_user` or the server past `total` bytes (0 = no limit).
/// Returns whether the upload fits. Check and insert share one transaction on
/// the single connection thread, so concurrent uploads cannot both squeeze
/// under the same remaining allowance.
pub async fn reserve_upload(
    db: &Db,
    key: &str,
    user: &str,
    size: u64,
    per_user: u64,
    total: u64,
) -> Result<bool, DbError> {
    let key = key.to_owned();
    let user = user.to_owned();
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let (used_by_user, used_total): (i64, i64) = tx
            .prepare_cached(
                "SELECT coalesce(sum(CASE WHEN user_name = ?1 THEN size END), 0), \
                 coalesce(sum(size), 0) FROM uploads",
            )?
            .query_row(params![user], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let over = |used: i64, limit: u64| limit > 0 && used as u64 + size > limit;
        if over(used_by_user, per_user) || over(used_total, total) {
            return Ok(false);
        }
        tx.prepare_cached("INSERT INTO uploads (key, user_name, size) VALUES (?1, ?2, ?3)")?
            .execute(params![key, user, size as i64])?;
        tx.commit()?;
        Ok(true)
    })
    .await
}

/// Forget an upload whose file is gone, returning its bytes to the quota.
pub async fn forget_upload(db: &Db, key: &str) -> Result<(), DbError> {
    let key = key.to_owned();
    db.call_db(move |conn| {
        conn.prepare_cached("DELETE FROM uploads WHERE key = ?1")?
            .execute(params![key])?;
        Ok(())
    })
    .await
}

/// Take every released upload that nothing names any more, deleting its row;
/// the caller removes the files. A released upload that is still named — a
/// forwarded copy of the message, an avatar, an emoji, a sound, the server
/// icon or a wiki page — is un-released instead, until a later delete
/// releases it again.
///
/// ponytail: one `instr` scan of `messages` per released key. Released keys
/// are few per sweep; index message attachments if that stops being true.
pub async fn take_unreferenced_uploads(db: &Db) -> Result<Vec<String>, DbError> {
    db.call_db(|conn| {
        let tx = conn.transaction()?;
        let keys: Vec<String> = tx
            .prepare_cached(
                "DELETE FROM uploads WHERE released = 1 AND NOT EXISTS ( \
                     SELECT 1 FROM messages WHERE instr(content, '/files/' || uploads.key) > 0 \
                     UNION ALL SELECT 1 FROM user_keys WHERE avatar = '/files/' || uploads.key \
                     UNION ALL SELECT 1 FROM emojis WHERE url = '/files/' || uploads.key \
                     UNION ALL SELECT 1 FROM soundboard_sounds WHERE url = '/files/' || uploads.key \
                     UNION ALL SELECT 1 FROM server_settings WHERE value = '/files/' || uploads.key \
                     UNION ALL SELECT 1 FROM wiki_pages \
                         WHERE instr(body, '/files/' || uploads.key) > 0) \
                 RETURNING key",
            )?
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        tx.execute("UPDATE uploads SET released = 0 WHERE released = 1", [])?;
        tx.commit()?;
        Ok(keys)
    })
    .await
}
