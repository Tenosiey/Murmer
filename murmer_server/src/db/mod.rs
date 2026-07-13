//! Database connection, schema initialization and domain-specific queries.
//!
//! Persistence uses an embedded SQLite database. The connection runs on a
//! dedicated thread (via `tokio-rusqlite`); [`Db`] is a cheap clonable handle
//! that serializes all queries onto that thread.
//!
//! Channels are identified by an auto-incrementing integer `id`. The `name`
//! column remains for display purposes. Messages reference channels via
//! `channel_id`.
//!
//! Submodules group queries by domain:
//! - [`channels`] – text channels, voice channels and categories
//! - [`direct_messages`] – private messages between two users
//! - [`messages`] – message CRUD and history retrieval
//! - [`moderation`] – ban and mute persistence
//! - [`pins`] – persisted message pins per channel
//! - [`reactions`] – emoji reaction operations
//! - [`roles`] – user role persistence

mod channels;
mod direct_messages;
mod messages;
mod moderation;
mod pins;
mod reactions;
mod roles;

pub use channels::*;
pub use direct_messages::*;
pub use messages::*;
pub use moderation::*;
pub use pins::*;
pub use reactions::*;
pub use roles::*;

use std::path::Path;
use tracing::error;

/// Handle to the SQLite connection thread. Clonable and shared across tasks.
pub type Db = tokio_rusqlite::Connection;
/// Error type returned by all database operations.
pub type DbError = tokio_rusqlite::Error;

/// Runs a closure on the connection thread with the error type pinned to
/// [`rusqlite::Error`], which the generic `Connection::call` cannot infer
/// when a closure never propagates an error.
pub trait DbCall {
    fn call_db<F, R>(&self, f: F) -> impl std::future::Future<Output = Result<R, DbError>> + Send
    where
        F: FnOnce(&mut rusqlite::Connection) -> rusqlite::Result<R> + Send + 'static,
        R: Send + 'static;
}

impl DbCall for Db {
    async fn call_db<F, R>(&self, f: F) -> Result<R, DbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> rusqlite::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        self.call(f).await
    }
}

/// Timestamps are stored as RFC 3339 TEXT in UTC so they parse back into
/// `chrono::DateTime<Utc>` and sort correctly as strings.
const NOW_UTC: &str = "strftime('%Y-%m-%dT%H:%M:%fZ','now')";

/// Open (or create) the SQLite database at `db_path` and ensure the schema
/// exists. Parent directories are created if missing.
#[tracing::instrument(skip(db_path))]
pub async fn init(db_path: &str) -> Result<Db, DbError> {
    if let Some(parent) = Path::new(db_path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                error!("failed to create database directory: {e}");
                DbError::Error(rusqlite::Error::InvalidPath(parent.to_path_buf()))
            })?;
        }
    }

    let conn = Db::open(db_path).await?;

    conn.call_db(|conn| {
        // WAL keeps readers non-blocking; the busy timeout covers the rare
        // moment a checkpoint holds the write lock.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        conn.execute_batch(&format!(
            r#"CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS channels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    description TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS voice_channels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    quality TEXT NOT NULL DEFAULT 'standard',
    bitrate INTEGER,
    category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels(id),
    content TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_channel_id ON messages (channel_id);
CREATE TABLE IF NOT EXISTS reactions (
    message_id INTEGER NOT NULL,
    user_name TEXT NOT NULL,
    emoji TEXT NOT NULL,
    PRIMARY KEY (message_id, user_name, emoji)
);
CREATE TABLE IF NOT EXISTS roles (
    public_key TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    color TEXT
);
CREATE TABLE IF NOT EXISTS bans (
    public_key TEXT PRIMARY KEY,
    user_name TEXT NOT NULL,
    banned_by TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
CREATE TABLE IF NOT EXISTS mutes (
    public_key TEXT PRIMARY KEY,
    user_name TEXT NOT NULL,
    muted_by TEXT NOT NULL DEFAULT '',
    muted_until TEXT,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
CREATE TABLE IF NOT EXISTS direct_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sender TEXT NOT NULL,
    recipient TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
CREATE INDEX IF NOT EXISTS idx_direct_messages_participants
    ON direct_messages (sender, recipient, id);
CREATE TABLE IF NOT EXISTS pins (
    message_id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL,
    pinned_by TEXT NOT NULL DEFAULT '',
    pinned_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
CREATE INDEX IF NOT EXISTS idx_pins_channel_id ON pins (channel_id);
CREATE TABLE IF NOT EXISTS bots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    owner_key TEXT NOT NULL DEFAULT '',
    permissions INTEGER NOT NULL DEFAULT 0,
    description TEXT NOT NULL DEFAULT '',
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
INSERT OR IGNORE INTO channels (name) VALUES ('general');
"#
        ))?;
        Ok(())
    })
    .await
    .map_err(|e| {
        error!("Failed to initialize database schema: {e}");
        e
    })?;

    Ok(conn)
}
