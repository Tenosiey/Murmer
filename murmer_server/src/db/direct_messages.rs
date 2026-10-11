//! Direct message persistence between pairs of users.
//!
//! Messages are stored as opaque JSON text like channel messages, with the
//! participants duplicated into indexed columns so conversations can be
//! queried without parsing the payload. Since direct messages are end-to-end
//! encrypted, the stored JSON carries `nonce`/`ciphertext` fields — the
//! server never persists DM plaintext.
//!
//! A DM can be edited or deleted by its sender only; a delete removes the one
//! stored row, which is both participants' copy.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};

/// Insert a direct message and return its id.
pub async fn insert_direct_message(
    db: &Db,
    sender: &str,
    recipient: &str,
    content: &str,
) -> Result<i64, DbError> {
    let sender = sender.to_owned();
    let recipient = recipient.to_owned();
    let content = content.to_owned();
    db.call_db(move |conn| {
        let id = conn
            .prepare_cached(
                "INSERT INTO direct_messages (sender, recipient, content)                  VALUES (?1, ?2, ?3) RETURNING id",
            )?
            .query_row(params![sender, recipient, content], |row| row.get(0))?;
        Ok(id)
    })
    .await
}

/// Fetch messages exchanged between two users, newest first.
pub async fn fetch_dm_history(
    db: &Db,
    user_a: &str,
    user_b: &str,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<(i64, String)>, DbError> {
    let user_a = user_a.to_owned();
    let user_b = user_b.to_owned();
    db.call_db(move |conn| {
        let map = |row: &rusqlite::Row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?));
        let rows = if let Some(before) = before {
            let mut stmt = conn.prepare_cached(
                "SELECT id, content FROM direct_messages \
                 WHERE ((sender = ?1 AND recipient = ?2) OR (sender = ?2 AND recipient = ?1)) \
                 AND id < ?3 ORDER BY id DESC LIMIT ?4",
            )?;

            stmt.query_map(params![user_a, user_b, before, limit], map)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare_cached(
                "SELECT id, content FROM direct_messages \
                 WHERE (sender = ?1 AND recipient = ?2) OR (sender = ?2 AND recipient = ?1) \
                 ORDER BY id DESC LIMIT ?3",
            )?;

            stmt.query_map(params![user_a, user_b, limit], map)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    })
    .await
}

/// Fetch one direct message as `(sender, recipient, content)`.
pub async fn get_direct_message(
    db: &Db,
    id: i64,
) -> Result<Option<(String, String, String)>, DbError> {
    db.call_db(move |conn| {
        let row = conn
            .prepare_cached("SELECT sender, recipient, content FROM direct_messages WHERE id = ?1")?
            .query_row(params![id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?;
        Ok(row)
    })
    .await
}

/// Replace a direct message's stored JSON. Returns `true` if a row changed.
pub async fn update_direct_message(db: &Db, id: i64, content: &str) -> Result<bool, DbError> {
    let content = content.to_owned();
    db.call_db(move |conn| {
        let affected = conn
            .prepare_cached("UPDATE direct_messages SET content = ?2 WHERE id = ?1")?
            .execute(params![id, content])?;
        Ok(affected > 0)
    })
    .await
}

/// Delete a direct message for both participants. Returns `true` if a row
/// was removed.
pub async fn delete_direct_message(db: &Db, id: i64) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let affected = conn
            .prepare_cached("DELETE FROM direct_messages WHERE id = ?1")?
            .execute(params![id])?;
        Ok(affected > 0)
    })
    .await
}
