//! WebSocket handler and helper utilities.
//!
//! This module handles the main WebSocket connection logic for the Murmer chat server.
//!
//! ## Message Flow
//! 1. Clients connect and authenticate using Ed25519 signatures
//! 2. Server validates signatures and implements anti-replay protection
//! 3. Authenticated clients can send chat messages, create channels, and join voice channels
//! 4. All messages are broadcast to relevant subscribers
//!
//! ## Security Features
//! - Ed25519 signature-based authentication
//! - Replay attack prevention using nonces
//! - Rate limiting on messages and authentication attempts
//! - Input validation for channel names and user names
//! - Bounds checking on history requests
//!
//! ## Message Types
//! - `presence`: Authentication and user registration
//! - `chat`: Text messages with optional images
//! - `switch-channel`: Change active text channel
//! - `load-history`: Request message history
//! - `create-channel`/`delete-channel`: Channel management
//! - `voice-*`: Voice channel operations
//!
//! Messages are JSON objects with a `type` field. Clients must authenticate with a
//! presence message before sending other events.

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{PublicKey, Signature, Verifier};
use futures::{SinkExt, StreamExt, stream::SplitSink};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

use crate::{
    AppState, db,
    roles::{RoleInfo, default_color},
    security,
};

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

/// Broadcast the users currently in a voice channel to all clients.
async fn broadcast_voice(state: &Arc<AppState>, channel: &str) {
    let vc = state.voice_channels.lock().await;
    let list: Vec<String> = vc
        .get(channel)
        .map(|set| set.iter().cloned().collect())
        .unwrap_or_default();
    drop(vc);
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-users",
        "channel": channel,
        "users": list,
    })) {
        let _ = state.tx.send(msg);
    }
}

fn sanitize_message_timestamp(value: &mut Value) -> DateTime<Utc> {
    let now = Utc::now();
    let parsed = value
        .get("timestamp")
        .and_then(|ts| ts.as_str())
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(now);
    value["timestamp"] = Value::String(parsed.to_rfc3339());
    parsed
}

/// Broadcast a user's role to all connected clients.
async fn broadcast_role(state: &Arc<AppState>, user: &str, info: &RoleInfo) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "role-update",
        "user": user,
        "role": info.role,
        "color": info.color,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send all known roles to a newly connected client.
async fn send_all_roles(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let roles = state.roles.lock().await.clone();
    for (user, info) in roles {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "role-update",
            "user": user,
            "role": info.role,
            "color": info.color,
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

/// Send the list of available voice channels to a client.
async fn send_voice_channels(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let list: Vec<String> = state.voice_channels.lock().await.keys().cloned().collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-list",
        "channels": list,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
    }
}

/// Send all voice channel member lists to a client.
async fn send_all_voice(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let map = state.voice_channels.lock().await.clone();
    for (chan, users) in map {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "voice-users",
            "channel": chan,
            "users": users.into_iter().collect::<Vec<_>>(),
        })) {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
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

/// Broadcast to all clients that a new voice channel was created.
async fn broadcast_new_voice_channel(state: &Arc<AppState>, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-add",
        "channel": name,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a channel was deleted.
async fn broadcast_remove_channel(state: &Arc<AppState>, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-remove",
        "channel": name,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a voice channel was deleted.
async fn broadcast_remove_voice_channel(state: &Arc<AppState>, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-remove",
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
    let mut voice_channel: Option<String> = None;

    let mut authenticated = state.password.is_none();

    loop {
        tokio::select! {
            Some(result) = receiver.next() => {
                let text = match result {
                    Ok(Message::Text(t)) => t,
                    _ => break,
                };
                if let Ok(mut v) = serde_json::from_str::<Value>(&text) {
                    if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
                        if t.starts_with("voice-") {
                            debug!("Received voice message: {t}");
                        } else {
                            info!("Received message: {text}");
                        }
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
                                        // SECURITY: Rate limit authentication attempts to prevent brute force attacks
                                        // TODO: Use actual client IP address instead of hardcoded localhost
                                        if !security::check_auth_rate_limit(&state.rate_limiter, "127.0.0.1").await {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"auth-rate-limit\"}".into()))
                                                .await;
                                            break;
                                        }

                                        // SECURITY: Validate timestamp is within acceptable window (60 seconds)
                                        // This prevents very old signatures from being replayed
                                        let timestamp = match security::validate_timestamp(ts) {
                                            Ok(ts) => ts,
                                            Err(err) => {
                                                error!("Authentication failed - {}: {}", err, ts);
                                                let _ = sender
                                                    .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-timestamp\"}".into()))
                                                    .await;
                                                break;
                                            }
                                        };

                                        // SECURITY: Create nonce from public key + timestamp to prevent replay attacks
                                        // Each signature can only be used once within the nonce expiry window
                                        let nonce = format!("{}:{}", pk, timestamp);
                                        if !security::check_and_store_nonce(&state.rate_limiter, &nonce).await {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"replay-attack\"}".into()))
                                                .await;
                                            break;
                                        }

                                        if let (Ok(pk_bytes), Ok(sig_bytes)) = (
                                            general_purpose::STANDARD.decode(pk),
                                            general_purpose::STANDARD.decode(sig),
                                        ) {
                                            if pk_bytes.len() == 32 {
                                                match PublicKey::from_bytes(&pk_bytes[..32]) {
                                                    Ok(key) => {
                                                        match Signature::from_bytes(&sig_bytes) {
                                                            Ok(signature) => {
                                                                if key.verify(ts.as_bytes(), &signature).is_ok() {
                                                                    authenticated = true;
                                                                } else {
                                                                    error!("Authentication failed - signature verification failed for key: {}", pk);
                                                                    let _ = sender
                                                                        .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-signature\"}".into()))
                                                                        .await;
                                                                    break;
                                                                }
                                                            }
                                                            Err(e) => {
                                                                error!("Authentication failed - invalid signature format: {}", e);
                                                                let _ = sender
                                                                    .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-signature-format\"}".into()))
                                                                    .await;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        error!("Authentication failed - invalid public key: {}", e);
                                                        let _ = sender
                                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-public-key\"}".into()))
                                                            .await;
                                                        break;
                                                    }
                                                }
                                            } else {
                                                error!("Authentication failed - public key wrong length: {}", pk_bytes.len());
                                                let _ = sender
                                                    .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-key-length\"}".into()))
                                                    .await;
                                                break;
                                            }
                                        } else {
                                            error!("Authentication failed - invalid base64 encoding");
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-encoding\"}".into()))
                                                .await;
                                            break;
                                        }
                                    }
                                }
                                if authenticated {
                                    if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
                                        // Validate user name
                                        if !security::validate_user_name(u) {
                                            error!("Invalid user name: {}", u);
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-username\"}".into()))
                                                .await;
                                            break;
                                        }
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
                                            if let Some((role, color)) = db::get_role(&state.db, pk).await {
                                                let info = RoleInfo { role: role.clone(), color: color.or_else(|| default_color(&role)) };
                                                {
                                                    let mut roles = state.roles.lock().await;
                                                    roles.insert(u.to_string(), info.clone());
                                                }
                                                broadcast_role(&state, u, &info).await;
                                            }
                                        }
                                        send_all_roles(&state, &mut sender).await;
                                        send_channels(&state, &mut sender).await;
                                        send_voice_channels(&state, &mut sender).await;
                                        send_users(&state, &mut sender).await;
                                        send_all_voice(&state, &mut sender).await;
                                        db::send_history(&state.db, &mut sender, &channel, None, 50).await;
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
                                    db::send_history(&state.db, &mut sender, &channel, None, 50).await;
                                }
                            }
                            "load-history" => {
                                let before = v.get("before").and_then(|b| b.as_i64());
                                let mut limit = v.get("limit").and_then(|l| l.as_i64()).unwrap_or(50);

                                // Prevent excessive history requests
                                if limit > 200 {
                                    limit = 200;
                                    tracing::warn!("History request limit capped at 200 for user: {:?}", user_name);
                                }

                                db::send_history(&state.db, &mut sender, &channel, before, limit).await;
                            }
                            "create-channel" => {
                                if let Some(ch) = v.get("name").and_then(|c| c.as_str()) {
                                    // Validate channel name
                                    if !security::validate_channel_name(ch) {
                                        error!("Invalid channel name: {}", ch);
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-channel-name\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    // TODO: Add role-based authorization check here
                                    // For now, any authenticated user can create channels

                                    if let Err(e) = db::add_channel(&state.db, ch).await {
                                        error!("db add channel error: {e}");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-creation-failed\"}".into()))
                                            .await;
                                    } else {
                                        get_or_create_channel(&state, ch).await;
                                        broadcast_new_channel(&state, ch).await;
                                    }
                                }
                            }
                            "delete-channel" => {
                                if let Some(ch) = v.get("name").and_then(|c| c.as_str()) {
                                    // Validate channel name
                                    if !security::validate_channel_name(ch) {
                                        error!("Invalid channel name for deletion: {}", ch);
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-channel-name\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    // Prevent deletion of general channel
                                    if ch == "general" {
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"cannot-delete-general\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    // TODO: Add role-based authorization check here
                                    // For now, any authenticated user can delete channels

                                    if let Err(e) = db::remove_channel(&state.db, ch).await {
                                        error!("db remove channel error: {e}");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-deletion-failed\"}".into()))
                                            .await;
                                    } else {
                                        {
                                            let mut chans = state.channels.lock().await;
                                            chans.remove(ch);
                                        }
                                        broadcast_remove_channel(&state, ch).await;
                                        if channel == ch {
                                            channel = "general".to_string();
                                            chan_tx = get_or_create_channel(&state, &channel).await;
                                            chan_rx = chan_tx.subscribe();
                                        }
                                    }
                                }
                            }
                            "create-voice-channel" => {
                                if let Some(ch) = v.get("name").and_then(|c| c.as_str()) {
                                    let mut map = state.voice_channels.lock().await;
                                    if !map.contains_key(ch) {
                                        map.insert(ch.to_string(), HashSet::new());
                                        drop(map);
                                        let _ = db::add_voice_channel(&state.db, ch).await;
                                        broadcast_new_voice_channel(&state, ch).await;
                                    }
                                }
                            }
                            "delete-voice-channel" => {
                                if let Some(ch) = v.get("name").and_then(|c| c.as_str()) {
                                    let mut map = state.voice_channels.lock().await;
                                    map.remove(ch);
                                    drop(map);
                                    let _ = db::remove_voice_channel(&state.db, ch).await;
                                    broadcast_remove_voice_channel(&state, ch).await;
                                    if voice_channel.as_deref() == Some(ch) {
                                        voice_channel = None;
                                    }
                                }
                            }
                            "chat" => {
                                // Rate limit messages
                                if let Some(ref user) = user_name {
                                    if !security::check_message_rate_limit(&state.rate_limiter, user).await {
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"message-rate-limit\"}".into()))
                                            .await;
                                        continue;
                                    }
                                }

                                v["channel"] = Value::String(channel.clone());
                                let timestamp = sanitize_message_timestamp(&mut v);
                                if !v.get("reactions").is_some() {
                                    v["reactions"] = Value::Object(Map::new());
                                }
                                if !v.get("time").and_then(|t| t.as_str()).is_some() {
                                    v["time"] = Value::String(timestamp.format("%H:%M:%S").to_string());
                                }
                                let out = serde_json::to_string(&v).unwrap_or_else(|_| text.to_string());
                                match state
                                    .db
                                    .query_one(
                                        "INSERT INTO messages (channel, content) VALUES ($1, $2) RETURNING id::bigint",
                                        &[&channel, &out],
                                    )
                                    .await
                                {
                                    Ok(row) => {
                                        let id: i64 = row.get(0);
                                        v["id"] = Value::from(id);
                                        let out_with_id = serde_json::to_string(&v).unwrap_or_else(|_| out.clone());
                                        let _ = chan_tx.send(out_with_id);
                                    }
                                    Err(e) => error!("db insert error: {e}"),
                                }
                            }
                            "react" => {
                                let user = match user_name.clone() {
                                    Some(name) => name,
                                    None => {
                                        let msg = serde_json::json!({"type": "error", "message": "not-authenticated"}).to_string();
                                        let _ = sender.send(Message::Text(msg.into())).await;
                                        continue;
                                    }
                                };

                                let Some(message_id) = v.get("messageId").and_then(|m| m.as_i64()) else {
                                    let msg = serde_json::json!({"type": "error", "message": "invalid-message-id"}).to_string();
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                    continue;
                                };

                                let Some(action) = v.get("action").and_then(|a| a.as_str()) else {
                                    let msg = serde_json::json!({"type": "error", "message": "invalid-reaction-action"}).to_string();
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                    continue;
                                };

                                let Some(raw_emoji) = v.get("emoji").and_then(|e| e.as_str()) else {
                                    let msg = serde_json::json!({"type": "error", "message": "invalid-emoji"}).to_string();
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                    continue;
                                };

                                let emoji = raw_emoji.trim();
                                if emoji.is_empty()
                                    || emoji.len() > 16
                                    || emoji.chars().any(|c| c.is_control() || c.is_whitespace())
                                {
                                    let msg = serde_json::json!({"type": "error", "message": "invalid-emoji"}).to_string();
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                    continue;
                                }

                                let message_id32 = match i32::try_from(message_id) {
                                    Ok(val) => val,
                                    Err(_) => {
                                        let msg = serde_json::json!({"type": "error", "message": "invalid-message-id"}).to_string();
                                        let _ = sender.send(Message::Text(msg.into())).await;
                                        continue;
                                    }
                                };

                                let target_channel = match db::get_message_channel(&state.db, message_id32).await {
                                    Ok(Some(ch)) => ch,
                                    Ok(None) => {
                                        let msg = serde_json::json!({"type": "error", "message": "message-not-found"}).to_string();
                                        let _ = sender.send(Message::Text(msg.into())).await;
                                        continue;
                                    }
                                    Err(e) => {
                                        error!("failed to lookup message channel for reaction: {e}");
                                        let msg = serde_json::json!({"type": "error", "message": "reaction-failed"}).to_string();
                                        let _ = sender.send(Message::Text(msg.into())).await;
                                        continue;
                                    }
                                };

                                let result = match action {
                                    "add" => db::add_reaction(&state.db, message_id32, &user, emoji).await,
                                    "remove" => db::remove_reaction(&state.db, message_id32, &user, emoji).await,
                                    _ => {
                                        let msg = serde_json::json!({"type": "error", "message": "invalid-reaction-action"}).to_string();
                                        let _ = sender.send(Message::Text(msg.into())).await;
                                        continue;
                                    }
                                };

                                if let Err(e) = result {
                                    error!("db reaction error: {e}");
                                    let msg = serde_json::json!({"type": "error", "message": "reaction-failed"}).to_string();
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                    continue;
                                }

                                let reactions = match db::get_reaction_summary(&state.db, message_id32).await {
                                    Ok(map) => map,
                                    Err(e) => {
                                        error!("db reaction summary error: {e}");
                                        continue;
                                    }
                                };

                                let payload = serde_json::json!({
                                    "type": "reaction-update",
                                    "channel": target_channel.clone(),
                                    "messageId": message_id,
                                    "reactions": reactions,
                                });
                                let chan_sender = get_or_create_channel(&state, &target_channel).await;
                                let _ = chan_sender.send(payload.to_string());
                            }
                            "ping" => {
                                let id = v.get("id").cloned().unwrap_or(Value::Null);
                                let msg = serde_json::json!({ "type": "pong", "id": id });
                                let _ = sender
                                    .send(Message::Text(msg.to_string().into()))
                                    .await;
                            }
                            "voice-join" => {
                                if let (Some(u), Some(ch)) = (
                                    v.get("user").and_then(|u| u.as_str()),
                                    v.get("channel").and_then(|c| c.as_str()),
                                ) {
                                    let mut map = state.voice_channels.lock().await;
                                    for users in map.values_mut() {
                                        users.remove(u);
                                    }
                                    let new_channel = !map.contains_key(ch);
                                    let entry = map.entry(ch.to_string()).or_default();
                                    entry.insert(u.to_string());
                                    if new_channel {
                                        broadcast_new_voice_channel(&state, ch).await;
                                    }
                                    voice_channel = Some(ch.to_string());
                                }
                                if let Some(ch) = v.get("channel").and_then(|c| c.as_str()) {
                                    broadcast_voice(&state, ch).await;
                                }
                                let _ = state.tx.send(text.to_string());
                            }
                            "voice-leave" => {
                                if let (Some(u), Some(ch)) = (
                                    v.get("user").and_then(|u| u.as_str()),
                                    v.get("channel").and_then(|c| c.as_str()),
                                ) {
                                    let mut map = state.voice_channels.lock().await;
                                    if let Some(set) = map.get_mut(ch) {
                                        set.remove(u);
                                    }
                                    drop(map);
                                    broadcast_voice(&state, ch).await;
                                    if voice_channel.as_deref() == Some(ch) {
                                        voice_channel = None;
                                    }
                                }
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
        let mut map = state.voice_channels.lock().await;
        let mut ch_to_broadcast = None;
        for (ch, users) in map.iter_mut() {
            if users.remove(&name) {
                ch_to_broadcast = Some(ch.clone());
                break;
            }
        }
        drop(map);
        if let Some(ch) = ch_to_broadcast {
            broadcast_voice(&state, &ch).await;
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
