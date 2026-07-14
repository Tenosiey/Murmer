//! Persistent user name to public key bindings.
//!
//! The first verified public key that authenticates under a name claims that
//! name permanently. Presence handling rejects any later connection that
//! claims the name with a different key (or none), so an offline user's name
//! — and the role attached to it — cannot be taken over. A binding can be
//! released by the operator with the `unbind-name` CLI subcommand, e.g. when
//! a user lost their keypair.

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
/// first key wins, later claims must match it.
pub async fn bind_user_key(db: &Db, user_name: &str, public_key: &str) -> Result<(), DbError> {
    let user_name = user_name.to_owned();
    let public_key = public_key.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO user_keys (user_name, public_key) VALUES (?1, ?2)",
            params![user_name, public_key],
        )?;
        Ok(())
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
