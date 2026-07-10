//! Persistent message pins per channel.
//!
//! Pins reference messages by id without a foreign key (the messages table is
//! created by a later migration batch); rows are cleaned up manually when the
//! pinned message is deleted.

use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::Client;

/// Pin a message. Pinning an already pinned message succeeds as a no-op.
/// Returns `false` when the channel already carries `max_per_channel` pins.
pub async fn add_pin(
    db: &Client,
    message_id: i32,
    channel_id: i32,
    pinned_by: &str,
    max_per_channel: i64,
) -> Result<bool, tokio_postgres::Error> {
    let already_pinned = db
        .query_opt("SELECT 1 FROM pins WHERE message_id = $1", &[&message_id])
        .await?
        .is_some();
    if already_pinned {
        return Ok(true);
    }

    let count: i64 = db
        .query_one(
            "SELECT COUNT(*) FROM pins WHERE channel_id = $1",
            &[&channel_id],
        )
        .await?
        .get(0);
    if count >= max_per_channel {
        return Ok(false);
    }

    db.execute(
        "INSERT INTO pins (message_id, channel_id, pinned_by) VALUES ($1, $2, $3) \
         ON CONFLICT (message_id) DO NOTHING",
        &[&message_id, &channel_id, &pinned_by],
    )
    .await?;
    Ok(true)
}

/// Unpin a message. Returns the channel it was pinned in, if it was pinned.
pub async fn remove_pin(
    db: &Client,
    message_id: i32,
) -> Result<Option<i32>, tokio_postgres::Error> {
    let row = db
        .query_opt(
            "DELETE FROM pins WHERE message_id = $1 RETURNING channel_id",
            &[&message_id],
        )
        .await?;
    Ok(row.map(|r| r.get(0)))
}

/// Load the pins of a channel as client-facing JSON entries, newest first.
pub async fn get_pins_for_channel(
    db: &Client,
    channel_id: i32,
) -> Result<Vec<Value>, tokio_postgres::Error> {
    let rows = db
        .query(
            "SELECT p.message_id::bigint, p.pinned_by, p.pinned_at, m.content \
             FROM pins p JOIN messages m ON m.id = p.message_id \
             WHERE p.channel_id = $1 ORDER BY p.pinned_at DESC, p.message_id DESC",
            &[&channel_id],
        )
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id: i64 = row.get(0);
            let pinned_by: String = row.get(1);
            let pinned_at: DateTime<Utc> = row.get(2);
            let content: String = row.get(3);
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
