use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use std::{collections::HashSet, env, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, broadcast};
use tokio_postgres::{Client, NoTls};
use tracing::{error, info};
use tracing_subscriber;

async fn broadcast_users(state: &Arc<AppState>) {
    let users = state.users.lock().await;
    let list: Vec<String> = users.iter().cloned().collect();
    drop(users);
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "online-users",
        "users": list,
    })) {
        let _ = state.tx.send(msg);
    }
}

struct AppState {
    tx: broadcast::Sender<String>,
    db: Arc<Client>,
    users: Arc<Mutex<HashSet<String>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let (tx, _rx) = broadcast::channel::<String>(100);

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    // Connect to the database with simple retry logic so that the server
    // doesn't panic if Postgres is still starting up when this container
    // begins. This can happen when using Docker Compose.
    let (db_client, connection) = {
        let mut attempts = 0u8;
        loop {
            match tokio_postgres::connect(&db_url, NoTls).await {
                Ok(result) => break result,
                Err(e) if attempts < 30 => {
                    attempts += 1;
                    tracing::warn!("database not ready ({e}), retrying...");
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
    db_client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS messages (id SERIAL PRIMARY KEY, content TEXT NOT NULL)",
        )
        .await
        .unwrap();
    let state = Arc::new(AppState {
        tx: tx.clone(),
        db: Arc::new(db_client),
        users: Arc::new(Mutex::new(HashSet::new())),
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    info!("WebSocket server running on {addr}");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    info!("Client connected");
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();
    let mut user_name: Option<String> = None;

    // send previous messages from DB
    if let Ok(rows) = state
        .db
        .query("SELECT content FROM messages ORDER BY id", &[])
        .await
    {
        for row in rows {
            let content: String = row.get(0);
            if sender.send(Message::Text(content)).await.is_err() {
                return;
            }
        }
    }

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(Message::Text(text))) = receiver.next().await {
        info!("Received message: {text}");
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
                match t {
                    "presence" => {
                        if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
                            {
                                let mut users = state.users.lock().await;
                                users.insert(u.to_string());
                            }
                            broadcast_users(&state).await;
                            user_name = Some(u.to_string());
                        }
                        continue;
                    }
                    "chat" => {
                        if let Err(e) = state
                            .db
                            .execute("INSERT INTO messages (content) VALUES ($1)", &[&text])
                            .await
                        {
                            error!("db insert error: {e}");
                        }
                    }
                    _ => {}
                }
            }
        }
        let _ = state.tx.send(text);
    }

    send_task.abort();
    if let Some(name) = user_name {
        let mut users = state.users.lock().await;
        users.remove(&name);
        drop(users);
        broadcast_users(&state).await;
    }
    info!("Client disconnected");
}
