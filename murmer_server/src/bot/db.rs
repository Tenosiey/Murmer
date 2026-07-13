//! Database operations for bot management.

use super::models::BotRecord;
use crate::db::{Db, DbCall, DbError};
use rusqlite::params;

fn row_to_bot(row: &rusqlite::Row) -> rusqlite::Result<BotRecord> {
    Ok(BotRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        token_hash: row.get(2)?,
        owner_key: row.get(3)?,
        permissions: row.get(4)?,
        description: row.get(5)?,
        active: row.get(6)?,
        created_at: row.get(7)?,
    })
}

/// `created_at` is stored as RFC 3339 TEXT, so it is selected verbatim.
const SELECT_COLS: &str =
    "id, name, token_hash, owner_key, permissions, description, active, created_at";

pub async fn create_bot(
    db: &Db,
    id: &str,
    name: &str,
    token_hash: &str,
    owner_key: &str,
    permissions: i32,
    description: &str,
) -> Result<BotRecord, DbError> {
    let id = id.to_owned();
    let name = name.to_owned();
    let token_hash = token_hash.to_owned();
    let owner_key = owner_key.to_owned();
    let description = description.to_owned();
    db.call_db(move |conn| {
        let query = format!(
            "INSERT INTO bots (id, name, token_hash, owner_key, permissions, description) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             RETURNING {SELECT_COLS}"
        );
        let bot = conn.query_row(
            &query,
            params![id, name, token_hash, owner_key, permissions, description],
            row_to_bot,
        )?;
        Ok(bot)
    })
    .await
}

pub async fn get_bot_by_id(db: &Db, id: &str) -> Result<Option<BotRecord>, DbError> {
    let id = id.to_owned();
    db.call_db(move |conn| {
        let query = format!("SELECT {SELECT_COLS} FROM bots WHERE id = ?1");
        let bot = conn.query_row(&query, params![id], row_to_bot).ok();
        Ok(bot)
    })
    .await
}

pub async fn get_bot_by_token_hash(
    db: &Db,
    token_hash: &str,
) -> Result<Option<BotRecord>, DbError> {
    let token_hash = token_hash.to_owned();
    db.call_db(move |conn| {
        let query = format!("SELECT {SELECT_COLS} FROM bots WHERE token_hash = ?1");
        let bot = conn.query_row(&query, params![token_hash], row_to_bot).ok();
        Ok(bot)
    })
    .await
}

pub async fn list_bots(db: &Db) -> Result<Vec<BotRecord>, DbError> {
    db.call_db(|conn| {
        let query = format!("SELECT {SELECT_COLS} FROM bots ORDER BY created_at");
        let mut stmt = conn.prepare(&query)?;
        let bots = stmt
            .query_map([], row_to_bot)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(bots)
    })
    .await
}

pub async fn update_bot(
    db: &Db,
    id: &str,
    name: Option<&str>,
    permissions: Option<i32>,
    description: Option<&str>,
    active: Option<bool>,
) -> Result<bool, DbError> {
    let current = match get_bot_by_id(db, id).await? {
        Some(bot) => bot,
        None => return Ok(false),
    };

    let id = id.to_owned();
    let new_name = name.unwrap_or(&current.name).to_owned();
    let new_perms = permissions.unwrap_or(current.permissions);
    let new_desc = description.unwrap_or(&current.description).to_owned();
    let new_active = active.unwrap_or(current.active);

    db.call_db(move |conn| {
        let affected = conn.execute(
            "UPDATE bots SET name = ?2, permissions = ?3, description = ?4, active = ?5 \
             WHERE id = ?1",
            params![id, new_name, new_perms, new_desc, new_active],
        )?;
        Ok(affected > 0)
    })
    .await
}

pub async fn update_bot_token(db: &Db, id: &str, new_token_hash: &str) -> Result<bool, DbError> {
    let id = id.to_owned();
    let new_token_hash = new_token_hash.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute(
            "UPDATE bots SET token_hash = ?2 WHERE id = ?1",
            params![id, new_token_hash],
        )?;
        Ok(affected > 0)
    })
    .await
}

pub async fn delete_bot(db: &Db, id: &str) -> Result<bool, DbError> {
    let id = id.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute("DELETE FROM bots WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    })
    .await
}

/// Fetch messages newer than a given ID (ascending order).
pub async fn fetch_messages_after(
    db: &Db,
    channel_id: i32,
    after: i64,
    limit: i64,
) -> Result<Vec<(i64, String)>, DbError> {
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content FROM messages \
             WHERE channel_id = ?1 AND id > ?2 ORDER BY id ASC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![channel_id, after, limit], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}
