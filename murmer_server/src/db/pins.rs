//! Persistent message pins per channel.
//!
//! Pins reference messages by id without a foreign key; rows are cleaned up
//! manually when the pinned message is deleted.

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::Value;

use super::{Db, DbCall, DbError};

/// Pin a message. Pinning an already pinned message succeeds as a no-op.
/// Returns `false` when the channel already carries `max_per_channel` pins.
pub async fn add_pin(
    db: &Db,
    message_id: i32,
    channel_id: i32,
    pinned_by: &str,
    max_per_channel: i64,
) -> Result<bool, DbError> {
    let pinned_by = pinned_by.to_owned();
    db.call_db(move |conn| {
        let already_pinned = conn
            .query_row(
                "SELECT 1 FROM pins WHERE message_id = ?1",
                params![message_id],
                |_| Ok(()),
            )
            .is_ok();
        if already_pinned {
            return Ok(true);
        }

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pins WHERE channel_id = ?1",
            params![channel_id],
            |row| row.get(0),
        )?;
        if count >= max_per_channel {
            return Ok(false);
        }

        conn.execute(
            "INSERT INTO pins (message_id, channel_id, pinned_by) VALUES (?1, ?2, ?3) \
             ON CONFLICT (message_id) DO NOTHING",
            params![message_id, channel_id, pinned_by],
        )?;
        Ok(true)
    })
    .await
}

/// Unpin a message. Returns the channel it was pinned in, if it was pinned.
pub async fn remove_pin(db: &Db, message_id: i32) -> Result<Option<i32>, DbError> {
    db.call_db(move |conn| {
        let channel_id = conn
            .query_row(
                "DELETE FROM pins WHERE message_id = ?1 RETURNING channel_id",
                params![message_id],
                |row| row.get(0),
            )
            .ok();
        Ok(channel_id)
    })
    .await
}

/// Load the pins of a channel as client-facing JSON entries, newest first.
pub async fn get_pins_for_channel(db: &Db, channel_id: i32) -> Result<Vec<Value>, DbError> {
    let rows = db
        .call_db(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT p.message_id, p.pinned_by, p.pinned_at, m.content \
                 FROM pins p JOIN messages m ON m.id = p.message_id \
                 WHERE p.channel_id = ?1 ORDER BY p.pinned_at DESC, p.message_id DESC",
            )?;
            let rows = stmt
                .query_map(params![channel_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, DateTime<Utc>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, pinned_by, pinned_at, content)| {
            let msg = serde_json::from_str::<Value>(&content).ok()?;
            Some(serde_json::json!({
                "id": id,
                "user": msg.get("user").cloned().unwrap_or(Value::Null),
                "text": msg.get("text").cloned().unwrap_or(Value::Null),
                "image": msg.get("image").cloned().unwrap_or(Value::Null),
                "timestamp": msg.get("timestamp").cloned().unwrap_or(Value::Null),
                "pinnedAt": pinned_at.to_rfc3339(),
                "pinnedBy": pinned_by,
            }))
        })
        .collect())
}
