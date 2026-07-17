//! Persistent user name to public key bindings.
//!
//! The first verified public key that authenticates under a name claims that
//! name permanently. Presence handling rejects any later connection that
//! claims the name with a different key (or none), so an offline user's name
//! — and the role attached to it — cannot be taken over. A binding can be
//! released by the operator with the `unbind-name` CLI subcommand, e.g. when
//! a user lost their keypair.
//!
//! The binding row also carries the user's avatar: a `/files/<key>` URL
//! pointing at a validated upload, or an empty string when unset.

use rusqlite::params;

use super::{Db, DbCall, DbError};

/// Look up the public key a user name is bound to, if any.
pub async fn get_user_key(db: &Db, user_name: &str) -> Result<Option<String>, DbError> {
    let user_name = user_name.to_owned();
    db.call_db(move |conn| {
        let key = conn
            .query_row(
                "SELECT public_key FROM user_keys WHERE user_name = ?1",
                params![user_name],
                |row| row.get(0),
            )
            .ok();
        Ok(key)
    })
    .await
}

/// Bind a user name to a public key. An existing binding is left untouched:
/// first key wins, later claims must match it. Returns `true` when the
/// binding was newly created, i.e. this is the user's first connection —
/// presence handling uses that to deliver the one-time welcome message.
pub async fn bind_user_key(db: &Db, user_name: &str, public_key: &str) -> Result<bool, DbError> {
    let user_name = user_name.to_owned();
    let public_key = public_key.to_owned();
    db.call_db(move |conn| {
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO user_keys (user_name, public_key) VALUES (?1, ?2)",
            params![user_name, public_key],
        )?;
        Ok(inserted > 0)
    })
    .await
}

/// Set (or clear, with an empty string) a user's avatar URL. Returns `true`
/// if the user has a binding row; users without one (e.g. bots) cannot carry
/// an avatar.
pub async fn set_user_avatar(db: &Db, user_name: &str, avatar: &str) -> Result<bool, DbError> {
    let user_name = user_name.to_owned();
    let avatar = avatar.to_owned();
    db.call_db(move |conn| {
        let updated = conn.execute(
            "UPDATE user_keys SET avatar = ?2 WHERE user_name = ?1",
            params![user_name, avatar],
        )?;
        Ok(updated > 0)
    })
    .await
}

/// Look up a user's avatar URL. `None` when unset or the user is unknown.
pub async fn get_user_avatar(db: &Db, user_name: &str) -> Result<Option<String>, DbError> {
    let user_name = user_name.to_owned();
    db.call_db(move |conn| {
        let avatar: Option<String> = conn
            .query_row(
                "SELECT avatar FROM user_keys WHERE user_name = ?1",
                params![user_name],
                |row| row.get(0),
            )
            .ok();
        Ok(avatar.filter(|a| !a.is_empty()))
    })
    .await
}

/// All users with a configured avatar, for the snapshot sent to new clients.
pub async fn get_all_avatars(db: &Db) -> Result<Vec<(String, String)>, DbError> {
    db.call_db(|conn| {
        let mut stmt =
            conn.prepare("SELECT user_name, avatar FROM user_keys WHERE avatar <> ''")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// How many users currently reference an avatar URL. Replaced avatar files
/// are only removed from disk when no other user still points at them.
pub async fn count_avatar_references(db: &Db, avatar: &str) -> Result<i64, DbError> {
    let avatar = avatar.to_owned();
    db.call_db(move |conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM user_keys WHERE avatar = ?1",
            params![avatar],
            |row| row.get(0),
        )
    })
    .await
}

/// Release the binding for a user name so a new key may claim it.
/// Returns `true` if a binding existed.
pub async fn unbind_user_name(db: &Db, user_name: &str) -> Result<bool, DbError> {
    let user_name = user_name.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute(
            "DELETE FROM user_keys WHERE user_name = ?1",
            params![user_name],
        )?;
        Ok(affected > 0)
    })
    .await
}
