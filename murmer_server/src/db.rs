use tokio_postgres::{Client, NoTls};
use tracing::warn;

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
ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS channel TEXT NOT NULL DEFAULT 'general';
"#,
        )
        .await
        .unwrap();

    client
}

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use serde_json::Value;

pub async fn fetch_messages(
    db: &Client,
    channel: &str,
    before: i64,
    limit: i64,
) -> Vec<(i64, String)> {
    if let Ok(rows) = db
        .query(
            "SELECT id, content FROM messages WHERE channel = $1 AND id < $2 ORDER BY id DESC LIMIT $3",
            &[&channel, &before, &limit],
        )
        .await
    {
        rows.iter()
            .map(|row| (row.get::<_, i64>(0), row.get::<_, String>(1)))
            .collect()
    } else {
        Vec::new()
    }
}

pub async fn send_history(
    db: &Client,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    channel: &str,
) {
    let msgs = fetch_messages(db, channel, i64::MAX, 25).await;
    for (id, content) in msgs.into_iter().rev() {
        if let Ok(mut v) = serde_json::from_str::<Value>(&content) {
            v["id"] = Value::from(id);
            if let Ok(out) = serde_json::to_string(&v) {
                if sender.send(Message::Text(out.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
