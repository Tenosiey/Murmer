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
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures::{SinkExt, StreamExt};
use hex;
use rand::RngCore;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::{AppState, db};

/// Broadcast the current list of online users to all connected clients.
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

    let mut authenticated = false;
    let mut is_admin = false;
    let mut nonce = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut nonce);
    let nonce_hex = hex::encode(nonce);
    let challenge = serde_json::json!({"type": "challenge", "nonce": nonce_hex});
    if sender
        .send(Message::Text(challenge.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    db::send_all_history(&state.db, &mut sender).await;
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
                                    let pk_str = match v.get("publicKey").and_then(|p| p.as_str()) {
                                        Some(s) => s,
                                        None => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"missing-key\"}".into()))
                                                .await;
                                            break;
                                        }
                                    };
                                    let sig_str = match v.get("signature").and_then(|s| s.as_str()) {
                                        Some(s) => s,
                                        None => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"missing-signature\"}".into()))
                                                .await;
                                            break;
                                        }
                                    };
                                    let nonce_field = match v.get("nonce").and_then(|n| n.as_str()) {
                                        Some(n) => n,
                                        None => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"missing-nonce\"}".into()))
                                                .await;
                                            break;
                                        }
                                    };
                                    if nonce_field != nonce_hex {
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-nonce\"}".into()))
                                            .await;
                                        break;
                                    }
                                    let pk_bytes = match hex::decode(pk_str) {
                                        Ok(b) => b,
                                        Err(_) => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-key\"}".into()))
                                                .await;
                                            break;
                                        }
                                    };
                                    let sig_bytes = match hex::decode(sig_str) {
                                        Ok(b) => b,
                                        Err(_) => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-signature\"}".into()))
                                                .await;
                                            break;
                                        }
                                    };
                                    let pk_array: [u8; 32] = match pk_bytes.as_slice().try_into() {
                                        Ok(a) => a,
                                        Err(_) => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-key\"}".into()))
                                                .await;
                                            break;
                                        }
                                    };
                                    let sig_array: [u8; 64] = match sig_bytes.as_slice().try_into() {
                                        Ok(a) => a,
                                        Err(_) => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-signature\"}".into()))
                                                .await;
                                            break;
                                        }
                                    };
                                    let verified = VerifyingKey::from_bytes(&pk_array)
                                        .and_then(|pk| {
                                            let sig = Signature::from_bytes(&sig_array);
                                            pk.verify(nonce_hex.as_bytes(), &sig)
                                        })
                                        .is_ok();
                                    if !verified {
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-signature\"}".into()))
                                            .await;
                                        break;
                                    }
                                    authenticated = true;
                                    if state.admins.contains(pk_str) {
                                        is_admin = true;
                                    }
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
