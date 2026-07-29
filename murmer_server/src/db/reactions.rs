//! Reaction persistence operations.

use rusqlite::params;
use std::collections::HashMap;

use super::{Db, DbCall, DbError};

/// Retrieve reactions for a set of message IDs, grouped by message and emoji.
pub async fn get_reactions_for_messages(
    db: &Db,
    ids: &[i64],
) -> Result<HashMap<i64, HashMap<String, Vec<String>>>, DbError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    // SQLite has no array parameter, and expanding the ids into one
    // placeholder each would make the SQL text depend on how many messages
    // were requested — a different statement per batch size, so none of them
    // could be reused from the prepared-statement cache. Passing the ids as a
    // JSON array and unnesting them with `json_each` keeps the statement
    // fixed, so this (which runs on every history load, thread and search)
    // compiles once for the lifetime of the process.
    let ids_json = {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(ids.len() * 8 + 2);
        out.push('[');
        for (i, id) in ids.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            // Writing an i64 into a String cannot fail.
            let _ = write!(out, "{id}");
        }
        out.push(']');
        out
    };
    let rows = db
        .call_db(move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT message_id, emoji, user_name FROM reactions \
                 WHERE message_id IN (SELECT value FROM json_each(?1))",
            )?;
            let rows = stmt
                .query_map(params![ids_json], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;

    let mut map: HashMap<i64, HashMap<String, Vec<String>>> = HashMap::new();
    for (message_id, emoji, user) in rows {
        let emoji_map = map.entry(message_id).or_default();
        let users = emoji_map.entry(emoji).or_default();
        users.push(user);
    }

    for emoji_map in map.values_mut() {
        for users in emoji_map.values_mut() {
            users.sort();
            users.dedup();
        }
    }

    Ok(map)
}

/// Retrieve all reactions for a single message, grouped by emoji.
pub async fn get_reaction_summary(
    db: &Db,
    message_id: i64,
) -> Result<HashMap<String, Vec<String>>, DbError> {
    let mut map = get_reactions_for_messages(db, &[message_id]).await?;
    Ok(map.remove(&message_id).unwrap_or_default())
}

/// Add a reaction to a message. Duplicate reactions by the same user are ignored.
pub async fn add_reaction(
    db: &Db,
    message_id: i64,
    user: &str,
    emoji: &str,
) -> Result<(), DbError> {
    let user = user.to_owned();
    let emoji = emoji.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO reactions (message_id, user_name, emoji) VALUES (?1, ?2, ?3)",
            params![message_id, user, emoji],
        )?;
        Ok(())
    })
    .await
}

/// Remove a reaction from a message.
pub async fn remove_reaction(
    db: &Db,
    message_id: i64,
    user: &str,
    emoji: &str,
) -> Result<(), DbError> {
    let user = user.to_owned();
    let emoji = emoji.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "DELETE FROM reactions WHERE message_id = ?1 AND user_name = ?2 AND emoji = ?3",
            params![message_id, user, emoji],
        )?;
        Ok(())
    })
    .await
}
