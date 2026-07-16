//! Server identity: name, description, welcome message and icon.
//!
//! All four values live in the generic `server_settings` key-value table
//! (shared with the stats toggle), so no schema changes are needed. Empty
//! strings mean "unset"; the icon value is a `/files/<key>` URL pointing at a
//! validated upload, or empty when no icon is configured.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};

/// `server_settings` key for the server's display name.
pub const IDENTITY_NAME_KEY: &str = "server_name";
/// `server_settings` key for the short server description.
pub const IDENTITY_DESCRIPTION_KEY: &str = "server_description";
/// `server_settings` key for the welcome message sent to first-time members.
pub const IDENTITY_WELCOME_KEY: &str = "welcome_message";
/// `server_settings` key for the server icon upload URL.
pub const IDENTITY_ICON_KEY: &str = "server_icon";

/// Snapshot of the configurable server identity. Unset fields are empty
/// strings (`icon` is `None`).
#[derive(Clone, Debug, Default)]
pub struct ServerIdentity {
    pub name: String,
    pub description: String,
    pub welcome_message: String,
    pub icon: Option<String>,
}

fn read_setting(conn: &rusqlite::Connection, key: &str) -> rusqlite::Result<String> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM server_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.unwrap_or_default())
}

/// Load the current server identity.
pub async fn get_server_identity(db: &Db) -> Result<ServerIdentity, DbError> {
    db.call_db(|conn| {
        let icon = read_setting(conn, IDENTITY_ICON_KEY)?;
        Ok(ServerIdentity {
            name: read_setting(conn, IDENTITY_NAME_KEY)?,
            description: read_setting(conn, IDENTITY_DESCRIPTION_KEY)?,
            welcome_message: read_setting(conn, IDENTITY_WELCOME_KEY)?,
            icon: if icon.is_empty() { None } else { Some(icon) },
        })
    })
    .await
}

/// Upsert a batch of identity fields (Owner/Admin action, checked by the
/// caller). Keys must be one of the `IDENTITY_*_KEY` constants — never
/// user-supplied strings.
pub async fn set_server_identity_fields(
    db: &Db,
    fields: Vec<(&'static str, String)>,
) -> Result<(), DbError> {
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        for (key, value) in fields {
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
