use axum::{
    Json, Router,
    extract::{
        Multipart, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, broadcast};
use tokio_postgres::{Client, NoTls};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
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

async fn broadcast_voice(state: &Arc<AppState>) {
    let v = state.voice_users.lock().await;
    let list: Vec<String> = v.iter().cloned().collect();
    drop(v);
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-users",
        "users": list,
    })) {
        let _ = state.tx.send(msg);
    }
}

async fn get_or_create_channel(state: &Arc<AppState>, name: &str) -> broadcast::Sender<String> {
    let mut channels = state.channels.lock().await;
    channels
        .entry(name.to_string())
        .or_insert_with(|| broadcast::channel::<String>(100).0)
        .clone()
}

async fn send_history(
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
            if sender.send(Message::Text(content)).await.is_err() {
                break;
            }
        }
    }
}

struct AppState {
    tx: broadcast::Sender<String>,
    channels: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
    db: Arc<Client>,
    users: Arc<Mutex<HashSet<String>>>,
    voice_users: Arc<Mutex<HashSet<String>>>,
    upload_dir: PathBuf,
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

    let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| "uploads".to_string());
    if let Err(e) = tokio::fs::create_dir_all(&upload_dir).await {
        panic!("create uploads dir: {e}");
    }

    let state = Arc::new(AppState {
        tx: tx.clone(),
        channels: Arc::new(Mutex::new(HashMap::new())),
        db: Arc::new(db_client),
        users: Arc::new(Mutex::new(HashSet::new())),
        voice_users: Arc::new(Mutex::new(HashSet::new())),
        upload_dir: PathBuf::from(upload_dir.clone()),
    });

    let cors = CorsLayer::permissive();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/upload", post(upload))
        .nest_service("/files", ServeDir::new(upload_dir))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    info!("WebSocket server running on {addr}");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn upload(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload".to_string());
        if let Ok(data) = field.bytes().await {
            let key = format!("{}-{}", chrono::Utc::now().timestamp_millis(), filename);
            let path = state.upload_dir.join(&key);
            match tokio::fs::write(&path, &data).await {
                Ok(_) => {
                    let url = format!("/files/{}", key);
                    return Json(serde_json::json!({"url": url})).into_response();
                }
                Err(e) => {
                    error!("upload error: {e}");
                    return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
    }
    axum::http::StatusCode::BAD_REQUEST.into_response()
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    info!("Client connected");
    let (mut sender, mut receiver) = socket.split();
    let mut global_rx = state.tx.subscribe();
    let mut channel = String::from("general");
    let mut chan_tx = get_or_create_channel(&state, &channel).await;
    let mut chan_rx = chan_tx.subscribe();
    let mut user_name: Option<String> = None;

    send_history(&state.db, &mut sender, &channel).await;
    broadcast_voice(&state).await;

    loop {
        tokio::select! {
            Some(result) = receiver.next() => {
                let text = match result {
                    Ok(Message::Text(t)) => t,
                    _ => break,
                };
                info!("Received message: {text}");
                if let Ok(mut v) = serde_json::from_str::<Value>(&text) {
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
                            }
                            "join" => {
                                if let Some(ch) = v.get("channel").and_then(|c| c.as_str()) {
                                    channel = ch.to_string();
                                    chan_tx = get_or_create_channel(&state, &channel).await;
                                    chan_rx = chan_tx.subscribe();
                                    send_history(&state.db, &mut sender, &channel).await;
                                }
                            }
                            "chat" => {
                                v["channel"] = Value::String(channel.clone());
                                let out = serde_json::to_string(&v).unwrap_or(text.clone());
                                if let Err(e) = state
                                    .db
                                    .execute(
                                        "INSERT INTO messages (channel, content) VALUES ($1, $2)",
                                        &[&channel, &out],
                                    )
                                    .await
                                {
                                    error!("db insert error: {e}");
                                }
                                let _ = chan_tx.send(out);
                            }
                            "voice-join" => {
                                if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
                                    let mut voice = state.voice_users.lock().await;
                                    voice.insert(u.to_string());
                                }
                                broadcast_voice(&state).await;
                                let _ = state.tx.send(text.clone());
                            }
                            "voice-leave" => {
                                if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
                                    let mut voice = state.voice_users.lock().await;
                                    voice.remove(u);
                                }
                                broadcast_voice(&state).await;
                                let _ = state.tx.send(text.clone());
                            }
                            "voice-offer" | "voice-answer" | "voice-candidate" => {
                                let _ = state.tx.send(text.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
            result = chan_rx.recv() => {
                match result {
                    Ok(msg) => {
                        if sender.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
            result = global_rx.recv() => {
                match result {
                    Ok(msg) => {
                        if sender.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }

    if let Some(name) = user_name {
        let mut users = state.users.lock().await;
        users.remove(&name);
        drop(users);
        broadcast_users(&state).await;
        let mut voice = state.voice_users.lock().await;
        if voice.remove(&name) {
            drop(voice);
            broadcast_voice(&state).await;
        } else {
            drop(voice);
        }
    }
    info!("Client disconnected");
}
