use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use sqlx::{Pool, Postgres, Row, postgres::PgPoolOptions};
use tracing::warn;

pub type Db = Pool<Postgres>;

/// Initialize the PostgreSQL connection pool and ensure the `messages` table exists.
/// Retries for a few seconds if the database is not ready.
pub async fn init(db_url: &str) -> Db {
    let mut attempts = 0u8;
    let pool = loop {
        match PgPoolOptions::new()
            .max_connections(5)
            .connect(db_url)
            .await
        {
            Ok(pool) => break pool,
            Err(e) if attempts < 30 => {
                attempts += 1;
                warn!("database not ready ({e}), retrying...");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => panic!("connect db: {e}"),
        }
    };

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS messages (
    id SERIAL PRIMARY KEY,
    channel TEXT NOT NULL,
    content TEXT NOT NULL
);"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

/// Send all messages from the given channel over the provided WebSocket.
pub async fn send_history(
    db: &Db,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    channel: &str,
) {
    if let Ok(rows) = sqlx::query("SELECT content FROM messages WHERE channel = $1 ORDER BY id")
        .bind(channel)
        .fetch_all(db)
        .await
    {
        for row in rows {
            let content: String = row.get("content");
            if sender.send(Message::Text(content.into())).await.is_err() {
                break;
            }
        }
    }
}

/// Send the full message history across all channels.
pub async fn send_all_history(
    db: &Db,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) {
    if let Ok(rows) = sqlx::query("SELECT content FROM messages ORDER BY id")
        .fetch_all(db)
        .await
    {
        for row in rows {
            let content: String = row.get("content");
            if sender.send(Message::Text(content.into())).await.is_err() {
                break;
            }
        }
    }
}
