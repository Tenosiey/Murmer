//! Direct message persistence between pairs of users.
//!
//! Messages are stored as opaque JSON text like channel messages, with the
//! participants duplicated into indexed columns so conversations can be
//! queried without parsing the payload. Since direct messages are end-to-end
//! encrypted, the stored JSON carries `nonce`/`ciphertext` fields — the
//! server never persists DM plaintext.

use rusqlite::params;

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
        let id = conn.query_row(
            "INSERT INTO direct_messages (sender, recipient, content) VALUES (?1, ?2, ?3) RETURNING id",
            params![sender, recipient, content],
            |row| row.get(0),
        )?;
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
            let mut stmt = conn.prepare(
                "SELECT id, content FROM direct_messages \
                 WHERE ((sender = ?1 AND recipient = ?2) OR (sender = ?2 AND recipient = ?1)) \
                 AND id < ?3 ORDER BY id DESC LIMIT ?4",
            )?;

            stmt.query_map(params![user_a, user_b, before, limit], map)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare(
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
