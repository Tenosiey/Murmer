//! Database operations for bot management.

use super::models::BotRecord;
use tokio_postgres::Client;

fn row_to_bot(row: &tokio_postgres::Row) -> BotRecord {
    BotRecord {
        id: row.get(0),
        name: row.get(1),
        token_hash: row.get(2),
        owner_key: row.get(3),
        permissions: row.get(4),
        description: row.get(5),
        active: row.get(6),
        created_at: row.get(7),
    }
}

const SELECT_COLS: &str = "id, name, token_hash, owner_key, permissions, description, active, \
     to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')";

pub async fn create_bot(
    db: &Client,
    id: &str,
    name: &str,
    token_hash: &str,
    owner_key: &str,
    permissions: i32,
    description: &str,
) -> Result<BotRecord, tokio_postgres::Error> {
    let query = format!(
        "INSERT INTO bots (id, name, token_hash, owner_key, permissions, description) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING {SELECT_COLS}"
    );
    let row = db
        .query_one(
            &query,
            &[
                &id,
                &name,
                &token_hash,
                &owner_key,
                &permissions,
                &description,
            ],
        )
        .await?;
    Ok(row_to_bot(&row))
}

pub async fn get_bot_by_id(
    db: &Client,
    id: &str,
) -> Result<Option<BotRecord>, tokio_postgres::Error> {
    let query = format!("SELECT {SELECT_COLS} FROM bots WHERE id = $1");
    let row = db.query_opt(&query, &[&id]).await?;
    Ok(row.as_ref().map(row_to_bot))
}

pub async fn get_bot_by_token_hash(
    db: &Client,
    token_hash: &str,
) -> Result<Option<BotRecord>, tokio_postgres::Error> {
    let query = format!("SELECT {SELECT_COLS} FROM bots WHERE token_hash = $1");
    let row = db.query_opt(&query, &[&token_hash]).await?;
    Ok(row.as_ref().map(row_to_bot))
}

pub async fn list_bots(db: &Client) -> Result<Vec<BotRecord>, tokio_postgres::Error> {
    let query = format!("SELECT {SELECT_COLS} FROM bots ORDER BY created_at");
    let rows = db.query(&query, &[]).await?;
    Ok(rows.iter().map(row_to_bot).collect())
}

pub async fn update_bot(
    db: &Client,
    id: &str,
    name: Option<&str>,
    permissions: Option<i32>,
    description: Option<&str>,
    active: Option<bool>,
) -> Result<bool, tokio_postgres::Error> {
    let current = match get_bot_by_id(db, id).await? {
        Some(bot) => bot,
        None => return Ok(false),
    };

    let new_name = name.unwrap_or(&current.name);
    let new_perms = permissions.unwrap_or(current.permissions);
    let new_desc = description.unwrap_or(&current.description);
    let new_active = active.unwrap_or(current.active);

    let affected = db
        .execute(
            "UPDATE bots SET name = $2, permissions = $3, description = $4, active = $5 \
             WHERE id = $1",
            &[&id, &new_name, &new_perms, &new_desc, &new_active],
        )
        .await?;
    Ok(affected > 0)
}

pub async fn update_bot_token(
    db: &Client,
    id: &str,
    new_token_hash: &str,
) -> Result<bool, tokio_postgres::Error> {
    let affected = db
        .execute(
            "UPDATE bots SET token_hash = $2 WHERE id = $1",
            &[&id, &new_token_hash],
        )
        .await?;
    Ok(affected > 0)
}

pub async fn delete_bot(db: &Client, id: &str) -> Result<bool, tokio_postgres::Error> {
    let affected = db.execute("DELETE FROM bots WHERE id = $1", &[&id]).await?;
    Ok(affected > 0)
}

/// Fetch messages newer than a given ID (ascending order).
pub async fn fetch_messages_after(
    db: &Client,
    channel_id: i32,
    after: i64,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let after32 = match i32::try_from(after) {
        Ok(val) => val,
        Err(_) => return Ok(Vec::new()),
    };
    let rows = db
        .query(
            "SELECT id::bigint, content FROM messages \
             WHERE channel_id = $1 AND id > $2 ORDER BY id ASC LIMIT $3",
            &[&channel_id, &after32, &limit],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<_, i64>(0), row.get(1)))
        .collect())
}
