//! Message persistence and history retrieval.

use std::collections::HashMap;

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use rusqlite::params;
use serde_json::Value;
use tracing::error;

use super::reactions::get_reactions_for_messages;
use super::{Db, DbCall, DbError};

fn row_to_id_content(row: &rusqlite::Row) -> rusqlite::Result<(i64, String)> {
    Ok((row.get(0)?, row.get(1)?))
}

/// Fetch a slice of messages from the database for a channel by ID.
pub async fn fetch_history(
    db: &Db,
    channel_id: i32,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<(i64, String)>, DbError> {
    db.call_db(move |conn| {
        let rows = if let Some(id) = before {
            let mut stmt = conn.prepare(
                "SELECT id, content FROM messages WHERE channel_id = ?1 AND id < ?2 \
                 ORDER BY id DESC LIMIT ?3",
            )?;
            let rows = stmt
                .query_map(params![channel_id, id, limit], row_to_id_content)?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, content FROM messages WHERE channel_id = ?1 ORDER BY id DESC LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![channel_id, limit], row_to_id_content)?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        Ok(rows)
    })
    .await
}

/// Insert a message into a channel and return its id.
pub async fn insert_message(db: &Db, channel_id: i32, content: &str) -> Result<i64, DbError> {
    let content = content.to_owned();
    db.call_db(move |conn| {
        let id = conn.query_row(
            "INSERT INTO messages (channel_id, content) VALUES (?1, ?2) RETURNING id",
            params![channel_id, content],
            |row| row.get(0),
        )?;
        Ok(id)
    })
    .await
}

/// Send a slice of messages over the WebSocket as a `history` payload.
pub async fn send_history(
    db: &Db,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    channel_id: i32,
    before: Option<i64>,
    limit: i64,
) {
    match fetch_history(db, channel_id, before, limit).await {
        Ok(rows) => {
            let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
            let reaction_map = if ids.is_empty() {
                HashMap::new()
            } else {
                match get_reactions_for_messages(db, &ids).await {
                    Ok(map) => map,
                    Err(e) => {
                        error!("db reaction load error: {e}");
                        HashMap::new()
                    }
                }
            };

            let mut msgs = Vec::new();
            for (id, content) in rows.into_iter().rev() {
                if let Ok(mut val) = serde_json::from_str::<Value>(&content) {
                    val["id"] = Value::from(id);
                    if let Some(reactions) = reaction_map.get(&id) {
                        if let Ok(value) = serde_json::to_value(reactions) {
                            val["reactions"] = value;
                        }
                    }
                    msgs.push(val);
                }
            }
            let payload = serde_json::json!({"type": "history", "messages": msgs});
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
        }
        Err(e) => error!("db history error: {e}"),
    }
}

/// Build an FTS5 MATCH expression from a raw user query.
///
/// User input must never reach the MATCH parser directly — its operators
/// (quotes, AND/OR/NOT, `*`, ...) would cause syntax errors or surprising
/// behaviour. The query is reduced to its alphanumeric tokens and wrapped in
/// a single quoted phrase with a prefix match on the last token, mirroring
/// the substring feel of the old LIKE search for word-aligned input.
/// Returns `None` when no searchable tokens remain.
fn fts_match_expression(query: &str) -> Option<String> {
    let cleaned: String = query
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.is_empty() {
        None
    } else {
        Some(format!("\"{}\"*", tokens.join(" ")))
    }
}

/// Search for messages within a channel using the full-text index over the
/// message text, newest first.
pub async fn search_messages(
    db: &Db,
    channel_id: i32,
    query: &str,
    limit: i64,
) -> Result<Vec<(i64, String)>, DbError> {
    let Some(match_expr) = fts_match_expression(query) else {
        return Ok(Vec::new());
    };
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content FROM messages WHERE channel_id = ?1 AND id IN \
             (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?2) \
             ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![channel_id, match_expr, limit], row_to_id_content)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// Fetch a thread: the root message plus every reply that carries the root's
/// id as its `threadId`, ordered oldest first.
///
/// Message content is stored as opaque JSON text, so candidate rows are
/// prefiltered with LIKE and then verified by parsing the JSON. This avoids a
/// JSON cast that would abort the whole query on a single malformed row.
pub async fn fetch_thread(
    db: &Db,
    channel_id: i32,
    root_id: i64,
    limit: i64,
) -> Result<Vec<(i64, String)>, DbError> {
    let pattern = format!("%\"threadId\":{root_id}%");
    let rows = db
        .call_db(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, content FROM messages WHERE id = ?1 \
                 OR (channel_id = ?2 AND content LIKE ?3) ORDER BY id ASC LIMIT ?4",
            )?;
            let rows = stmt
                .query_map(
                    params![root_id, channel_id, pattern, limit],
                    row_to_id_content,
                )?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, content)| {
            if id == root_id {
                return Some((id, content));
            }
            // The LIKE prefilter also matches ids with more digits
            // (e.g. threadId 12 matches 123), so verify the parsed value.
            let parsed = serde_json::from_str::<Value>(&content).ok()?;
            let thread_id = parsed.get("threadId").and_then(|t| t.as_i64())?;
            (thread_id == root_id).then_some((id, content))
        })
        .collect())
}

/// List messages flagged as ephemeral, as `(id, channel_id, content)` rows.
///
/// Rows are prefiltered with LIKE on the serialized JSON; callers must verify
/// the parsed `ephemeral` field (the pattern also matches the text appearing
/// inside a message body).
pub async fn get_ephemeral_messages(db: &Db) -> Result<Vec<(i64, i32, String)>, DbError> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, content FROM messages \
             WHERE content LIKE '%\"ephemeral\":true%'",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// Return the channel ID a message belongs to, if it exists.
pub async fn get_message_channel_id(db: &Db, message_id: i64) -> Result<Option<i32>, DbError> {
    db.call_db(move |conn| {
        let id = conn
            .query_row(
                "SELECT channel_id FROM messages WHERE id = ?1",
                params![message_id],
                |row| row.get(0),
            )
            .ok();
        Ok(id)
    })
    .await
}

/// Metadata for a stored message.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub channel_id: i32,
    pub content: Value,
}

/// Fetch a message record including its `channel_id` and JSON payload.
pub async fn get_message_record(
    db: &Db,
    message_id: i64,
) -> Result<Option<MessageRecord>, DbError> {
    let row = db
        .call_db(move |conn| {
            let row = conn
                .query_row(
                    "SELECT channel_id, content FROM messages WHERE id = ?1",
                    params![message_id],
                    |row| Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?)),
                )
                .ok();
            Ok(row)
        })
        .await?;
    match row {
        Some((channel_id, raw_content)) => {
            let content = match serde_json::from_str::<Value>(&raw_content) {
                Ok(value) => value,
                Err(error) => {
                    error!("Failed to parse stored message JSON (id {message_id}): {error}");
                    Value::Null
                }
            };
            Ok(Some(MessageRecord {
                channel_id,
                content,
            }))
        }
        None => Ok(None),
    }
}

/// Replace the JSON content of a message. Returns `true` if a row was updated.
pub async fn update_message_content(
    db: &Db,
    message_id: i64,
    content: &str,
) -> Result<bool, DbError> {
    let content = content.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute(
            "UPDATE messages SET content = ?2 WHERE id = ?1",
            params![message_id, content],
        )?;
        Ok(affected > 0)
    })
    .await
}

/// Delete a message by ID, along with any pin referencing it.
/// Returns `true` if a message row was removed.
pub async fn delete_message(db: &Db, message_id: i64) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        conn.execute(
            "DELETE FROM pins WHERE message_id = ?1",
            params![message_id],
        )?;
        let affected = conn.execute("DELETE FROM messages WHERE id = ?1", params![message_id])?;
        Ok(affected > 0)
    })
    .await
}
