//! Message persistence and history retrieval.

use std::collections::HashMap;

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use rusqlite::params;
use serde_json::{Map, Value};
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
            let mut stmt = conn.prepare_cached(
                "SELECT id, content FROM messages WHERE channel_id = ?1 AND id < ?2 \
                 ORDER BY id DESC LIMIT ?3",
            )?;

            stmt.query_map(params![channel_id, id, limit], row_to_id_content)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare_cached(
                "SELECT id, content FROM messages WHERE channel_id = ?1 ORDER BY id DESC LIMIT ?2",
            )?;

            stmt.query_map(params![channel_id, limit], row_to_id_content)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    })
    .await
}

/// Insert a message into a channel and return its id.
pub async fn insert_message(db: &Db, channel_id: i32, content: &str) -> Result<i64, DbError> {
    let content = content.to_owned();
    db.call_db(move |conn| {
        let id = conn
            .prepare_cached(
                "INSERT INTO messages (channel_id, content) VALUES (?1, ?2) RETURNING id",
            )?
            .query_row(params![channel_id, content], |row| row.get(0))?;
        Ok(id)
    })
    .await
}

/// Turn stored `(id, content)` rows into message frames: parse the JSON,
/// stamp `id`, fill a missing `channelId` and attach the reactions and poll
/// votes. History, search, threads and the bot API all serve messages through
/// this so they cannot drift apart in shape. Rows that fail to parse are
/// dropped.
pub async fn hydrate_messages(db: &Db, rows: Vec<(i64, String)>, channel_id: i32) -> Vec<Value> {
    let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
    let reaction_map = get_reactions_for_messages(db, &ids)
        .await
        .unwrap_or_else(|e| {
            error!("db reaction load error in channel {channel_id}: {e}");
            HashMap::new()
        });

    let mut messages: Vec<Value> = rows
        .into_iter()
        .filter_map(|(id, content)| {
            let mut msg = serde_json::from_str::<Value>(&content).ok()?;
            msg["id"] = Value::from(id);
            if msg.get("channelId").is_none() {
                msg["channelId"] = Value::from(channel_id);
            }
            msg["reactions"] = reaction_map
                .get(&id)
                .and_then(|r| serde_json::to_value(r).ok())
                .unwrap_or_else(|| Value::Object(Map::new()));
            Some(msg)
        })
        .collect();
    super::polls::attach_poll_votes(db, &mut messages).await;
    messages
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
        Ok(mut rows) => {
            rows.reverse();
            let msgs = hydrate_messages(db, rows, channel_id).await;
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
///
/// Shared with the wiki search so both indexes answer the same query text
/// the same way.
pub(super) fn fts_match_expression(query: &str) -> Option<String> {
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

/// Narrowing for a message search beyond its words: `from:`, `has:file`,
/// `before:` and `after:` in the search box. The two dates are RFC 3339 UTC
/// strings, compared as Julian days against the indexed `sent_at` column.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SearchFilters {
    /// Account name of the author; never a display name.
    pub from: Option<String>,
    /// Only messages carrying an uploaded file or image.
    pub has_file: bool,
    /// Only messages stamped strictly before this time.
    pub before: Option<String>,
    /// Only messages stamped at or after this time.
    pub after: Option<String>,
}

/// Search for messages within a channel using the full-text index over the
/// message text, newest first.
///
/// Filters alone are a search too ("everything alice posted"), so the words
/// may be empty when a filter is set. They read the author and timestamp the
/// server stamped into the stored JSON, which stay plaintext in an encrypted
/// channel: author and date filters reach there even though words cannot
/// (an attachment there is sealed, so `has:file` cannot).
pub async fn search_messages(
    db: &Db,
    channel_id: i32,
    query: &str,
    filters: &SearchFilters,
    limit: i64,
) -> Result<Vec<(i64, String)>, DbError> {
    let match_expr = fts_match_expression(query);
    if match_expr.is_none() && *filters == SearchFilters::default() {
        return Ok(Vec::new());
    }
    let filters = filters.clone();
    db.call_db(move |conn| {
        // `j` is NULL for a malformed row, so json_extract never sees it: an
        // error there would abort the whole search, not just skip the row.
        // Author and dates read the indexed generated columns (see
        // `run_schema`), so they narrow the rows before any JSON is parsed.
        let mut stmt = conn.prepare_cached(
            "SELECT id, content FROM (SELECT id, content, \
             CASE WHEN json_valid(content) THEN content END AS j \
             FROM messages WHERE channel_id = ?1 \
             AND (?3 IS NULL OR author = ?3) \
             AND (?5 IS NULL OR sent_at < julianday(?5)) \
             AND (?6 IS NULL OR sent_at >= julianday(?6))) \
             WHERE (?2 IS NULL OR id IN \
               (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?2)) \
             AND (?4 = 0 OR json_extract(j, '$.attachment') IS NOT NULL \
               OR json_extract(j, '$.image') IS NOT NULL) \
             ORDER BY id DESC LIMIT ?7",
        )?;
        let rows = stmt
            .query_map(
                params![
                    channel_id,
                    match_expr,
                    filters.from,
                    filters.has_file,
                    filters.before,
                    filters.after,
                    limit
                ],
                row_to_id_content,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// The newest messages in `channel_ids` that may mention `user`, as
/// `(id, channel_id, content)` rows, newest first.
///
/// A prefilter only, like [`fetch_thread`]'s: LIKE finds `@user` anywhere in
/// the stored JSON (case-insensitively for ASCII, as the client's rule is)
/// plus every message that pinged a group, and the caller decides with
/// [`crate::mentions::is_mention_of`]. The channel list arrives as a JSON
/// array because a variable-length `IN (...)` cannot be a cached statement.
pub async fn recent_mention_candidates(
    db: &Db,
    channel_ids: &[i32],
    user: &str,
    limit: i64,
) -> Result<Vec<(i64, i32, String)>, DbError> {
    let channels = serde_json::to_string(channel_ids).unwrap_or_else(|_| "[]".to_string());
    // Account names may contain `_`, a LIKE wildcard; escape it and its kin.
    let escaped: String = user
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            _ => vec![c],
        })
        .collect();
    let named = format!("%@{escaped}%");
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, channel_id, content FROM messages \
             WHERE channel_id IN (SELECT value FROM json_each(?1)) \
             AND (content LIKE ?2 ESCAPE '\\' OR content LIKE '%\"mentions\":{%') \
             ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![channels, named, limit], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
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
            let mut stmt = conn.prepare_cached(
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
        let mut stmt = conn.prepare_cached(
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
            .prepare_cached("SELECT channel_id FROM messages WHERE id = ?1")?
            .query_row(params![message_id], |row| row.get(0))
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
                .prepare_cached("SELECT channel_id, content FROM messages WHERE id = ?1")?
                .query_row(params![message_id], |row| {
                    Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
                })
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
        let affected = conn
            .prepare_cached("UPDATE messages SET content = ?2 WHERE id = ?1")?
            .execute(params![message_id, content])?;
        Ok(affected > 0)
    })
    .await
}

/// Delete the newest `count` messages of a channel, with their pins and
/// reactions, in one transaction. Returns the ids removed, so each client can
/// drop exactly those.
pub async fn delete_latest_messages(
    db: &Db,
    channel_id: i32,
    count: usize,
) -> Result<Vec<i64>, DbError> {
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let ids = tx
            .prepare_cached(
                "SELECT id FROM messages WHERE channel_id = ?1 ORDER BY id DESC LIMIT ?2",
            )?
            .query_map(params![channel_id, count as i64], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        for id in &ids {
            tx.prepare_cached("DELETE FROM reactions WHERE message_id = ?1")?
                .execute(params![id])?;
            tx.prepare_cached("DELETE FROM pins WHERE message_id = ?1")?
                .execute(params![id])?;
            tx.prepare_cached("DELETE FROM messages WHERE id = ?1")?
                .execute(params![id])?;
        }
        tx.commit()?;
        Ok(ids)
    })
    .await
}

/// Delete a message by ID, along with any pin referencing it.
/// Returns `true` if a message row was removed.
pub async fn delete_message(db: &Db, message_id: i64) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        conn.prepare_cached("DELETE FROM pins WHERE message_id = ?1")?
            .execute(params![message_id])?;
        let affected = conn
            .prepare_cached("DELETE FROM messages WHERE id = ?1")?
            .execute(params![message_id])?;
        Ok(affected > 0)
    })
    .await
}
