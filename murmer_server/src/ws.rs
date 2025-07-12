use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::{AppState, db};

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

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    info!("Client connected");
    let (mut sender, mut receiver) = socket.split();
    let mut global_rx = state.tx.subscribe();
    let mut channel = String::from("general");
    let mut chan_tx = get_or_create_channel(&state, &channel).await;
    let mut chan_rx = chan_tx.subscribe();
    let mut user_name: Option<String> = None;

    let mut authenticated = state.password.is_none();

    db::send_history(&state.db, &mut sender, &channel).await;
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
                        if !authenticated && t != "presence" {
                            let _ = sender
                                .send(Message::Text("{\"type\":\"error\",\"message\":\"unauthenticated\"}".into()))
                                .await;
                            break;
                        }
                        match t {
                            "presence" => {
                                if !authenticated {
                                    if let Some(required) = &state.password {
                                        let provided = v.get("password").and_then(|p| p.as_str());
                                        if provided != Some(required) {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-password\"}".into()))
                                                .await;
                                            break;
                                        }
                                    }
                                    authenticated = true;
                                }
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
                                    db::send_history(&state.db, &mut sender, &channel).await;
                                }
                            }
                            "chat" => {
                                v["channel"] = Value::String(channel.clone());
                                let out = serde_json::to_string(&v).unwrap_or_else(|_| text.to_string());
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
                            "ping" => {
                                let id = v.get("id").cloned().unwrap_or(Value::Null);
                                let msg = serde_json::json!({ "type": "pong", "id": id });
                                let _ = sender.send(Message::Text(msg.to_string())).await;
                            }
                            "voice-join" => {
                                if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
                                    let mut voice = state.voice_users.lock().await;
                                    voice.insert(u.to_string());
                                }
                                broadcast_voice(&state).await;
                                let _ = state.tx.send(text.to_string());
                            }
                            "voice-leave" => {
                                if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
                                    let mut voice = state.voice_users.lock().await;
                                    voice.remove(u);
                                }
                                broadcast_voice(&state).await;
                                let _ = state.tx.send(text.to_string());
                            }
                            "voice-offer" | "voice-answer" | "voice-candidate" => {
                                let _ = state.tx.send(text.to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
            result = chan_rx.recv() => {
                match result {
                    Ok(msg) => {
                        if sender.send(Message::Text(msg.into())).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
            result = global_rx.recv() => {
                match result {
                    Ok(msg) => {
                        if sender.send(Message::Text(msg.into())).await.is_err() { break; }
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

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}
