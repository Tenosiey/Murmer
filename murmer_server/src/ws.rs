//! WebSocket handler and helper utilities.
//!
//! Messages are JSON objects with a `type` field. Clients authenticate with a
//! presence message and then send chat or voice events.

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{PublicKey, Signature, Verifier};
use futures::{SinkExt, StreamExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::{AppState, db};

/// Broadcast the current list of online users to all connected clients.
async fn broadcast_users(state: &Arc<AppState>) {
    let users = state.users.lock().await;
    let online: Vec<String> = users.iter().cloned().collect();
    drop(users);
    let known = state.known_users.lock().await;
    let all: Vec<String> = known.iter().cloned().collect();
    drop(known);
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "online-users",
        "users": online,
        "all": all,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send the current list of online and known users to a single client.
async fn send_users(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let users = state.users.lock().await;
    let online: Vec<String> = users.iter().cloned().collect();
    drop(users);
    let known = state.known_users.lock().await;
    let all: Vec<String> = known.iter().cloned().collect();
    drop(known);
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "online-users",
        "users": online,
        "all": all,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
    }
}

/// Broadcast the users currently in the voice channel to all clients.
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

/// Broadcast a user's role to all connected clients.
async fn broadcast_role(state: &Arc<AppState>, user: &str, role: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "role-update",
        "user": user,
        "role": role,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send all known roles to a newly connected client.
async fn send_all_roles(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let roles = state.roles.lock().await.clone();
    for (user, role) in roles {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "role-update",
            "user": user,
            "role": role,
        })) {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    }
}

/// Send the list of available channels to a client.
async fn send_channels(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let list = db::get_channels(&state.db).await;
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-list",
        "channels": list,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
    }
}

/// Broadcast to all clients that a new channel was created.
async fn broadcast_new_channel(state: &Arc<AppState>, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-add",
        "channel": name,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Retrieve the broadcast channel for the given chat room, creating it if necessary.
async fn get_or_create_channel(state: &Arc<AppState>, name: &str) -> broadcast::Sender<String> {
    let mut channels = state.channels.lock().await;
    channels
        .entry(name.to_string())
        .or_insert_with(|| broadcast::channel::<String>(100).0)
        .clone()
}

/// Main WebSocket loop handling incoming messages and broadcasting events.
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
    send_all_roles(&state, &mut sender).await;
    send_channels(&state, &mut sender).await;
    send_users(&state, &mut sender).await;

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
                                    if let (Some(pk), Some(sig), Some(ts)) = (
                                        v.get("publicKey").and_then(|p| p.as_str()),
                                        v.get("signature").and_then(|s| s.as_str()),
                                        v.get("timestamp").and_then(|t| t.as_str()),
                                    ) {
                                        if let (Ok(pk_bytes), Ok(sig_bytes)) = (
                                            general_purpose::STANDARD.decode(pk),
                                            general_purpose::STANDARD.decode(sig),
                                        ) {
                                            if pk_bytes.len() == 32 {
                                                if let Ok(key) = PublicKey::from_bytes(&pk_bytes[..32]) {
                                                    if let Ok(signature) = Signature::from_bytes(&sig_bytes) {
                                                        let within = match ts.parse::<i64>() {
                                                            Ok(num) => (chrono::Utc::now().timestamp_millis() - num).abs() < 60_000,
                                                            Err(_) => false,
                                                        };
                                                        if within && key.verify(ts.as_bytes(), &signature).is_ok() {
                                                            authenticated = true;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if authenticated {
                                    if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
                                        {
                                            let mut users = state.users.lock().await;
                                            users.insert(u.to_string());
                                        }
                                        {
                                            let mut known = state.known_users.lock().await;
                                            known.insert(u.to_string());
                                        }
                                        broadcast_users(&state).await;
                                        user_name = Some(u.to_string());
                                        if let Some(pk) = v.get("publicKey").and_then(|p| p.as_str()) {
                                            {
                                                let mut user_keys = state.user_keys.lock().await;
                                                user_keys.insert(u.to_string(), pk.to_string());
                                            }
                                            if let Some(role) = db::get_role(&state.db, pk).await {
                                                {
                                                    let mut roles = state.roles.lock().await;
                                                    roles.insert(u.to_string(), role.clone());
                                                }
                                                broadcast_role(&state, u, &role).await;
                                            }
                                        }
                                    }
                                } else {
                                    let _ = sender
                                        .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-signature\"}".into()))
                                        .await;
                                    break;
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
                            "create-channel" => {
                                if let Some(ch) = v.get("name").and_then(|c| c.as_str()) {
                                    if let Err(e) = db::add_channel(&state.db, ch).await {
                                        error!("db add channel error: {e}");
                                    } else {
                                        get_or_create_channel(&state, ch).await;
                                        broadcast_new_channel(&state, ch).await;
                                    }
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
                                let _ = sender
                                    .send(Message::Text(msg.to_string().into()))
                                    .await;
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
                            _ => {
                                error!("unknown message type: {t}");
                            }
                        }
                    }
                } else {
                    error!("invalid json message: {text}");
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
        // Keep role and key mappings so clients can display roles
        // even when the user is offline.
    }
    info!("Client disconnected");
}

/// Axum handler that upgrades the HTTP connection to a WebSocket and
/// spawns [`handle_socket`] for message processing.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}
