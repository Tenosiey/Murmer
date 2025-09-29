//! Database connection helpers and functions for loading chat history.
//!
//! The server persists all chat messages in a single `messages` table with a
//! `channel` column to distinguish between channels. These helpers create the
//! table on startup and provide utility functions to fetch history for clients.

use std::collections::HashMap;

use tokio_postgres::{Client, NoTls};
use tracing::{error, warn};

/// Initialize a PostgreSQL [`Client`] and ensure the `messages` table exists.
/// The connection is retried for a few seconds if the database is not ready.
pub async fn init(db_url: &str) -> Result<Client, tokio_postgres::Error> {
    let (client, connection) = {
        let mut attempts = 0u8;
        loop {
            match tokio_postgres::connect(db_url, NoTls).await {
                Ok(result) => break result,
                Err(e) if attempts < 30 => {
                    attempts += 1;
                    warn!("database not ready ({e}), retrying...");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                Err(e) => return Err(e),
            }
        }
    };

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("db connection error: {e}");
        }
    });

    client
        .batch_execute(
            r#"CREATE TABLE IF NOT EXISTS messages (
    id SERIAL PRIMARY KEY,
    channel TEXT NOT NULL,
    content TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS reactions (
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_name TEXT NOT NULL,
    emoji TEXT NOT NULL,
    PRIMARY KEY (message_id, user_name, emoji)
);
CREATE TABLE IF NOT EXISTS roles (
    public_key TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    color TEXT
);
CREATE TABLE IF NOT EXISTS channels (
    name TEXT PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS voice_channels (
    name TEXT PRIMARY KEY,
    quality TEXT NOT NULL DEFAULT 'standard',
    bitrate INTEGER
);
INSERT INTO channels (name) VALUES ('general') ON CONFLICT DO NOTHING;
"#,
        )
        .await
        .map_err(|e| {
            error!("Failed to initialize database schema: {}", e);
            e
        })?;

    client
        .batch_execute(
            "ALTER TABLE roles ADD COLUMN IF NOT EXISTS color TEXT;\nALTER TABLE voice_channels ADD COLUMN IF NOT EXISTS quality TEXT NOT NULL DEFAULT 'standard';\nALTER TABLE voice_channels ADD COLUMN IF NOT EXISTS bitrate INTEGER;\nUPDATE voice_channels SET quality = 'standard' WHERE quality IS NULL OR trim(quality) = '';\nUPDATE voice_channels SET bitrate = 64000 WHERE bitrate IS NULL;",
        )
        .await
        .ok();

    Ok(client)
}

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use serde_json::Value;

/// Fetch a slice of messages from the database.
pub async fn fetch_history(
    db: &Client,
    channel: &str,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let rows = if let Some(id) = before {
        // Safely convert i64 to i32 with bounds checking
        let id32 = match i32::try_from(id) {
            Ok(val) => val,
            Err(_) => {
                error!("Message ID too large for database query: {}", id);
                // Use a reasonable fallback - return early with empty results
                return Ok(Vec::new());
            }
        };
        db.query(
            "SELECT id::bigint, content FROM messages WHERE channel = $1 AND id < $2 ORDER BY id DESC LIMIT $3",
            &[&channel, &id32, &limit],
        )
        .await?
    } else {
        db.query(
            "SELECT id::bigint, content FROM messages WHERE channel = $1 ORDER BY id DESC LIMIT $2",
            &[&channel, &limit],
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
    channel: &str,
    before: Option<i64>,
    limit: i64,
) {
    match fetch_history(db, channel, before, limit).await {
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
            if !msgs.is_empty() {
                let payload = serde_json::json!({"type": "history", "messages": msgs});
                let _ = sender.send(Message::Text(payload.to_string().into())).await;
            }
        }
        Err(e) => error!("db history error: {e}"),
    }
}

/// Retrieve reactions for a set of message IDs, grouped by emoji.
pub async fn get_reactions_for_messages(
    db: &Client,
    ids: &[i32],
) -> Result<HashMap<i32, HashMap<String, Vec<String>>>, tokio_postgres::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = db
        .query(
            "SELECT message_id, emoji, user_name FROM reactions WHERE message_id = ANY($1)",
            &[&ids],
        )
        .await?;

    let mut map: HashMap<i32, HashMap<String, Vec<String>>> = HashMap::new();
    for row in rows {
        let message_id: i32 = row.get(0);
        let emoji: String = row.get(1);
        let user: String = row.get(2);
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

/// Retrieve all reactions for a single message ID, grouped by emoji.
pub async fn get_reaction_summary(
    db: &Client,
    message_id: i32,
) -> Result<HashMap<String, Vec<String>>, tokio_postgres::Error> {
    let mut map = get_reactions_for_messages(db, &[message_id]).await?;
    Ok(map.remove(&message_id).unwrap_or_default())
}

/// Add a reaction to a message. Duplicate reactions by the same user are ignored.
pub async fn add_reaction(
    db: &Client,
    message_id: i32,
    user: &str,
    emoji: &str,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO reactions (message_id, user_name, emoji) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        &[&message_id, &user, &emoji],
    )
    .await
    .map(|_| ())
}

/// Remove a reaction from a message.
pub async fn remove_reaction(
    db: &Client,
    message_id: i32,
    user: &str,
    emoji: &str,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "DELETE FROM reactions WHERE message_id = $1 AND user_name = $2 AND emoji = $3",
        &[&message_id, &user, &emoji],
    )
    .await
    .map(|_| ())
}

/// Return the channel a message belongs to, if it exists.
pub async fn get_message_channel(
    db: &Client,
    message_id: i32,
) -> Result<Option<String>, tokio_postgres::Error> {
    db.query_opt("SELECT channel FROM messages WHERE id = $1", &[&message_id])
        .await
        .map(|row| row.map(|r| r.get(0)))
}

/// Get the role for a user by public key, if any.
pub async fn get_role(db: &Client, key: &str) -> Option<(String, Option<String>)> {
    db.query_opt(
        "SELECT role, color FROM roles WHERE public_key = $1",
        &[&key],
    )
    .await
    .ok()
    .flatten()
    .map(|row| (row.get(0), row.get(1)))
}

/// Insert or update a user's role.
pub async fn set_role(
    db: &Client,
    key: &str,
    role: &str,
    color: Option<&str>,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO roles (public_key, role, color) VALUES ($1, $2, $3) \
        ON CONFLICT (public_key) DO UPDATE SET role = EXCLUDED.role, color = EXCLUDED.color",
        &[&key, &role, &color],
    )
    .await
    .map(|_| ())
}

/// Retrieve the list of text channels.
pub async fn get_channels(db: &Client) -> Vec<String> {
    match db
        .query("SELECT name FROM channels ORDER BY name", &[])
        .await
    {
        Ok(rows) => rows.into_iter().map(|row| row.get(0)).collect(),
        Err(_) => Vec::new(),
    }
}

/// Insert a new channel if it does not already exist.
pub async fn add_channel(db: &Client, name: &str) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO channels (name) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&name],
    )
    .await
    .map(|_| ())
}

/// Delete an existing channel.
pub async fn remove_channel(db: &Client, name: &str) -> Result<(), tokio_postgres::Error> {
    db.execute("DELETE FROM channels WHERE name = $1", &[&name])
        .await
        .map(|_| ())
}

/// Retrieve the list of voice channels.
#[derive(Clone)]
pub struct VoiceChannelRecord {
    pub name: String,
    pub quality: String,
    pub bitrate: Option<i32>,
}

pub async fn get_voice_channels(db: &Client) -> Vec<VoiceChannelRecord> {
    match db
        .query(
            "SELECT name, quality, bitrate FROM voice_channels ORDER BY name",
            &[],
        )
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|row| VoiceChannelRecord {
                name: row.get(0),
                quality: row.get(1),
                bitrate: row.get(2),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Insert a new voice channel if it does not already exist.
pub async fn add_voice_channel(
    db: &Client,
    name: &str,
    quality: &str,
    bitrate: Option<i32>,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO voice_channels (name, quality, bitrate) VALUES ($1, $2, $3) \
            ON CONFLICT (name) DO NOTHING",
        &[&name, &quality, &bitrate],
    )
    .await
    .map(|_| ())
}

/// Update an existing voice channel configuration.
pub async fn update_voice_channel(
    db: &Client,
    name: &str,
    quality: &str,
    bitrate: Option<i32>,
) -> Result<bool, tokio_postgres::Error> {
    db.execute(
        "UPDATE voice_channels SET quality = $2, bitrate = $3 WHERE name = $1",
        &[&name, &quality, &bitrate],
    )
    .await
    .map(|count| count > 0)
}

/// Delete an existing voice channel.
pub async fn remove_voice_channel(db: &Client, name: &str) -> Result<(), tokio_postgres::Error> {
    db.execute("DELETE FROM voice_channels WHERE name = $1", &[&name])
        .await
        .map(|_| ())
}
