//! Database connection helpers and functions for loading chat history.
//!
//! The server persists all chat messages in a single `messages` table with a
//! `channel` column to distinguish between channels. These helpers create the
//! table on startup and provide utility functions to fetch history for clients.

use tokio_postgres::{Client, NoTls};
use tracing::warn;

/// Initialize a PostgreSQL [`Client`] and ensure the `messages` table exists.
/// The connection is retried for a few seconds if the database is not ready.
pub async fn init(db_url: &str) -> Client {
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
                Err(e) => panic!("connect db: {e}"),
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
CREATE TABLE IF NOT EXISTS roles (
    public_key TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    color TEXT
);
CREATE TABLE IF NOT EXISTS channels (
    name TEXT PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS voice_channels (
    name TEXT PRIMARY KEY
);
INSERT INTO channels (name) VALUES ('general') ON CONFLICT DO NOTHING;
"#,
        )
        .await
        .unwrap();

    client
        .batch_execute("ALTER TABLE roles ADD COLUMN IF NOT EXISTS color TEXT;")
        .await
        .ok();

    client
}

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use serde_json::Value;
use tracing::error;

/// Fetch a slice of messages from the database.
pub async fn fetch_history(
    db: &Client,
    channel: &str,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let rows = if let Some(id) = before {
        let id32 = id as i32;
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
            let mut msgs = Vec::new();
            for (id, content) in rows.into_iter().rev() {
                if let Ok(mut val) = serde_json::from_str::<Value>(&content) {
                    val["id"] = Value::from(id);
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
pub async fn get_voice_channels(db: &Client) -> Vec<String> {
    match db
        .query("SELECT name FROM voice_channels ORDER BY name", &[])
        .await
    {
        Ok(rows) => rows.into_iter().map(|row| row.get(0)).collect(),
        Err(_) => Vec::new(),
    }
}

/// Insert a new voice channel if it does not already exist.
pub async fn add_voice_channel(db: &Client, name: &str) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO voice_channels (name) VALUES ($1) ON CONFLICT DO NOTHING",
        &[&name],
    )
    .await
    .map(|_| ())
}

/// Delete an existing voice channel.
pub async fn remove_voice_channel(db: &Client, name: &str) -> Result<(), tokio_postgres::Error> {
    db.execute("DELETE FROM voice_channels WHERE name = $1", &[&name])
        .await
        .map(|_| ())
}
