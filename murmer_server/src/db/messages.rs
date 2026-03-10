//! Message persistence and history retrieval.

use std::collections::HashMap;

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use serde_json::Value;
use tokio_postgres::Client;
use tracing::error;

use super::reactions::get_reactions_for_messages;

fn escape_like_pattern(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Fetch a slice of messages from the database for a channel by ID.
pub async fn fetch_history(
    db: &Client,
    channel_id: i32,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let rows = if let Some(id) = before {
        let id32 = match i32::try_from(id) {
            Ok(val) => val,
            Err(_) => {
                error!("Message ID too large for database query: {}", id);
                return Ok(Vec::new());
            }
        };
        db.query(
            "SELECT id::bigint, content FROM messages WHERE channel_id = $1 AND id < $2 ORDER BY id DESC LIMIT $3",
            &[&channel_id, &id32, &limit],
        )
        .await?
    } else {
        db.query(
            "SELECT id::bigint, content FROM messages WHERE channel_id = $1 ORDER BY id DESC LIMIT $2",
            &[&channel_id, &limit],
        )
        .await?
    };
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<_, i64>(0), row.get(1)))
        .collect())
}

/// Send a slice of messages over the WebSocket as a `history` payload.
pub async fn send_history(
    db: &Client,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    channel_id: i32,
    before: Option<i64>,
    limit: i64,
) {
    match fetch_history(db, channel_id, before, limit).await {
        Ok(rows) => {
            let mut ids32 = Vec::new();
            for (id, _) in &rows {
                match i32::try_from(*id) {
                    Ok(val) => ids32.push(val),
                    Err(_) => error!("Message ID too large for reaction lookup: {}", id),
                }
            }

            let reaction_map = if ids32.is_empty() {
                HashMap::new()
            } else {
                match get_reactions_for_messages(db, &ids32).await {
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
                    #[allow(clippy::collapsible_if)]
                    if let Ok(id32) = i32::try_from(id) {
                        if let Some(reactions) = reaction_map.get(&id32) {
                            if let Ok(value) = serde_json::to_value(reactions) {
                                val["reactions"] = value;
                            }
                        }
                    }
                    msgs.push(val);
                }
            }
            let payload = serde_json::json!({"type": "history", "messages": msgs});
            let _ = sender.send(Message::Text(payload.to_string())).await;
        }
        Err(e) => error!("db history error: {e}"),
    }
}

/// Search for messages containing a query string within a channel.
pub async fn search_messages(
    db: &Client,
    channel_id: i32,
    query: &str,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let pattern = format!("%{}%", escape_like_pattern(query));
    let rows = db
        .query(
            "SELECT id::bigint, content FROM messages WHERE channel_id = $1 AND content ILIKE $2 ESCAPE '\\\\' ORDER BY id DESC LIMIT $3",
            &[&channel_id, &pattern, &limit],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<_, i64>(0), row.get(1)))
        .collect())
}

/// Return the channel ID a message belongs to, if it exists.
pub async fn get_message_channel_id(
    db: &Client,
    message_id: i32,
) -> Result<Option<i32>, tokio_postgres::Error> {
    db.query_opt(
        "SELECT channel_id FROM messages WHERE id = $1",
        &[&message_id],
    )
    .await
    .map(|row| row.map(|r| r.get(0)))
}

/// Metadata for a stored message.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub channel_id: i32,
    pub content: Value,
}

/// Fetch a message record including its `channel_id` and JSON payload.
pub async fn get_message_record(
    db: &Client,
    message_id: i32,
) -> Result<Option<MessageRecord>, tokio_postgres::Error> {
    match db
        .query_opt(
            "SELECT channel_id, content FROM messages WHERE id = $1",
            &[&message_id],
        )
        .await?
    {
        Some(row) => {
            let channel_id: i32 = row.get(0);
            let raw_content: String = row.get(1);
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

/// Delete a message by ID. Returns `true` if a row was removed.
pub async fn delete_message(db: &Client, message_id: i32) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM messages WHERE id = $1", &[&message_id])
        .await
        .map(|affected| affected > 0)
}
