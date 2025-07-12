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
"#,
        )
        .await
        .unwrap();

    client
}

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;

/// Send all messages from the given channel over the provided WebSocket.
pub async fn send_history(
    db: &Client,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    channel: &str,
) {
    if let Ok(rows) = db
        .query(
            "SELECT content FROM messages WHERE channel = $1 ORDER BY id",
            &[&channel],
        )
        .await
    {
        for row in rows {
            let content: String = row.get(0);
            if sender.send(Message::Text(content.into())).await.is_err() {
                break;
            }
        }
    }
}

/// Send the full message history across all channels.
pub async fn send_all_history(
    db: &Client,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) {
    if let Ok(rows) = db
        .query("SELECT content FROM messages ORDER BY id", &[])
        .await
    {
        for row in rows {
            let content: String = row.get(0);
            if sender.send(Message::Text(content.into())).await.is_err() {
                break;
            }
        }
    }
}
