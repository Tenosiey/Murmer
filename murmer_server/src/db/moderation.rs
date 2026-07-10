//! Ban and mute persistence.
//!
//! Both bans and mutes are keyed by the target's Ed25519 public key (the
//! closest thing to a stable identity) but also record the user name so
//! moderators can lift a ban or mute after the target has disconnected.

use chrono::{DateTime, Utc};
use tokio_postgres::Client;

/// Persist a ban for a public key.
pub async fn add_ban(
    db: &Client,
    key: &str,
    user_name: &str,
    banned_by: &str,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO bans (public_key, user_name, banned_by) VALUES ($1, $2, $3) \
         ON CONFLICT (public_key) DO UPDATE SET user_name = EXCLUDED.user_name, banned_by = EXCLUDED.banned_by",
        &[&key, &user_name, &banned_by],
    )
    .await
    .map(|_| ())
}

/// Remove any bans recorded for a user name. Returns `true` if a ban was lifted.
pub async fn remove_ban_by_name(
    db: &Client,
    user_name: &str,
) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM bans WHERE user_name = $1", &[&user_name])
        .await
        .map(|affected| affected > 0)
}

/// Check whether a public key or user name is banned.
pub async fn is_banned(
    db: &Client,
    key: Option<&str>,
    user_name: &str,
) -> Result<bool, tokio_postgres::Error> {
    let row = match key {
        Some(key) => {
            db.query_opt(
                "SELECT 1 FROM bans WHERE public_key = $1 OR user_name = $2 LIMIT 1",
                &[&key, &user_name],
            )
            .await?
        }
        None => {
            db.query_opt(
                "SELECT 1 FROM bans WHERE user_name = $1 LIMIT 1",
                &[&user_name],
            )
            .await?
        }
    };
    Ok(row.is_some())
}

/// Persist a mute for a public key. `muted_until` of `None` means indefinite.
pub async fn add_mute(
    db: &Client,
    key: &str,
    user_name: &str,
    muted_by: &str,
    muted_until: Option<DateTime<Utc>>,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO mutes (public_key, user_name, muted_by, muted_until) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (public_key) DO UPDATE SET user_name = EXCLUDED.user_name, \
         muted_by = EXCLUDED.muted_by, muted_until = EXCLUDED.muted_until",
        &[&key, &user_name, &muted_by, &muted_until],
    )
    .await
    .map(|_| ())
}

/// Remove any mutes recorded for a user name. Returns the public keys that
/// were muted so callers can purge in-memory state.
pub async fn remove_mute_by_name(
    db: &Client,
    user_name: &str,
) -> Result<Vec<String>, tokio_postgres::Error> {
    let rows = db
        .query(
            "DELETE FROM mutes WHERE user_name = $1 RETURNING public_key",
            &[&user_name],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

/// Remove a mute by public key. Returns `true` if a mute was lifted.
pub async fn remove_mute_by_key(db: &Client, key: &str) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM mutes WHERE public_key = $1", &[&key])
        .await
        .map(|affected| affected > 0)
}

/// Load all persisted mutes as `(public_key, muted_until)` pairs.
pub async fn get_all_mutes(
    db: &Client,
) -> Result<Vec<(String, Option<DateTime<Utc>>)>, tokio_postgres::Error> {
    let rows = db
        .query("SELECT public_key, muted_until FROM mutes", &[])
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect())
}
