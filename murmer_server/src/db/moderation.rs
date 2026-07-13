//! Ban and mute persistence.
//!
//! Both bans and mutes are keyed by the target's Ed25519 public key (the
//! closest thing to a stable identity) but also record the user name so
//! moderators can lift a ban or mute after the target has disconnected.

use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{Db, DbCall, DbError};

/// Persist a ban for a public key.
pub async fn add_ban(db: &Db, key: &str, user_name: &str, banned_by: &str) -> Result<(), DbError> {
    let key = key.to_owned();
    let user_name = user_name.to_owned();
    let banned_by = banned_by.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "INSERT INTO bans (public_key, user_name, banned_by) VALUES (?1, ?2, ?3) \
             ON CONFLICT (public_key) DO UPDATE SET user_name = excluded.user_name, banned_by = excluded.banned_by",
            params![key, user_name, banned_by],
        )?;
        Ok(())
    })
    .await
}

/// Remove any bans recorded for a user name. Returns `true` if a ban was lifted.
pub async fn remove_ban_by_name(db: &Db, user_name: &str) -> Result<bool, DbError> {
    let user_name = user_name.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute("DELETE FROM bans WHERE user_name = ?1", params![user_name])?;
        Ok(affected > 0)
    })
    .await
}

/// Check whether a public key or user name is banned.
pub async fn is_banned(db: &Db, key: Option<&str>, user_name: &str) -> Result<bool, DbError> {
    let key = key.map(str::to_owned);
    let user_name = user_name.to_owned();
    db.call_db(move |conn| {
        let found = match key {
            Some(key) => conn
                .query_row(
                    "SELECT 1 FROM bans WHERE public_key = ?1 OR user_name = ?2 LIMIT 1",
                    params![key, user_name],
                    |_| Ok(()),
                )
                .is_ok(),
            None => conn
                .query_row(
                    "SELECT 1 FROM bans WHERE user_name = ?1 LIMIT 1",
                    params![user_name],
                    |_| Ok(()),
                )
                .is_ok(),
        };
        Ok(found)
    })
    .await
}

/// Persist a mute for a public key. `muted_until` of `None` means indefinite.
pub async fn add_mute(
    db: &Db,
    key: &str,
    user_name: &str,
    muted_by: &str,
    muted_until: Option<DateTime<Utc>>,
) -> Result<(), DbError> {
    let key = key.to_owned();
    let user_name = user_name.to_owned();
    let muted_by = muted_by.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "INSERT INTO mutes (public_key, user_name, muted_by, muted_until) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (public_key) DO UPDATE SET user_name = excluded.user_name, \
             muted_by = excluded.muted_by, muted_until = excluded.muted_until",
            params![key, user_name, muted_by, muted_until],
        )?;
        Ok(())
    })
    .await
}

/// Remove any mutes recorded for a user name. Returns the public keys that
/// were muted so callers can purge in-memory state.
pub async fn remove_mute_by_name(db: &Db, user_name: &str) -> Result<Vec<String>, DbError> {
    let user_name = user_name.to_owned();
    db.call_db(move |conn| {
        let mut stmt =
            conn.prepare("DELETE FROM mutes WHERE user_name = ?1 RETURNING public_key")?;
        let keys = stmt
            .query_map(params![user_name], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(keys)
    })
    .await
}

/// Remove a mute by public key. Returns `true` if a mute was lifted.
pub async fn remove_mute_by_key(db: &Db, key: &str) -> Result<bool, DbError> {
    let key = key.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute("DELETE FROM mutes WHERE public_key = ?1", params![key])?;
        Ok(affected > 0)
    })
    .await
}

/// Load all persisted mutes as `(public_key, muted_until)` pairs.
pub async fn get_all_mutes(db: &Db) -> Result<Vec<(String, Option<DateTime<Utc>>)>, DbError> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare("SELECT public_key, muted_until FROM mutes")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}
