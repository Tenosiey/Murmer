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
        ConnectInfo, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Utc};
use ed25519_dalek::{PublicKey, Signature, Verifier};
use futures::{SinkExt, StreamExt, stream::SplitSink};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

use crate::{
    AppState, VoiceChannelState, db,
    roles::{RoleInfo, default_color},
    security,
};

/// Roles that are allowed to create or delete channels when administrative
/// controls are enabled.
const CHANNEL_MANAGE_ROLES: &[&str] = &["Admin", "Mod", "Owner"];

/// Default quality label assigned to new voice channels.
const DEFAULT_VOICE_QUALITY: &str = "standard";
/// Default bitrate (in bits per second) assigned to new voice channels.
const DEFAULT_VOICE_BITRATE: i32 = 64_000;
/// Upper bound to reject unreasonable bitrate configuration values.
const MAX_ALLOWED_VOICE_BITRATE: i32 = 320_000;

/// Allowed user status values broadcast to clients.
const USER_STATUSES: &[&str] = &["online", "away", "busy", "offline"];

fn normalize_status(value: &str) -> Option<&'static str> {
    USER_STATUSES
        .iter()
        .copied()
        .find(|status| status.eq_ignore_ascii_case(value))
}

fn validate_voice_quality(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 32
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ')
}

fn validate_bitrate(value: i64) -> Option<i32> {
    if value <= 0 || value > MAX_ALLOWED_VOICE_BITRATE as i64 {
        return None;
    }
    i32::try_from(value).ok()
}

fn voice_channel_descriptor(name: &str, info: &VoiceChannelState) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "quality": info.quality,
        "bitrate": info.bitrate,
    })
}

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
    let entry = vc.get(channel).cloned();
    drop(vc);
    let list: Vec<String> = entry
        .map(|info| info.users.into_iter().collect())
        .unwrap_or_default();
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

/// Send all known user statuses to a newly connected client.
async fn send_all_statuses(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let statuses: HashMap<String, String> = state.statuses.lock().await.clone();
    if statuses.is_empty() {
        return;
    }
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "status-snapshot",
        "statuses": statuses,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
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
    let map = state.voice_channels.lock().await;
    let mut entries: Vec<(String, VoiceChannelState)> = map
        .iter()
        .map(|(name, info)| (name.clone(), info.clone()))
        .collect();
    drop(map);
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let channels: Vec<serde_json::Value> = entries
        .iter()
        .map(|(name, info)| voice_channel_descriptor(name, info))
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-list",
        "channels": channels,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
    }
}

/// Send all voice channel member lists to a client.
async fn send_all_voice(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let map = state.voice_channels.lock().await.clone();
    for (chan, info) in map {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "voice-users",
            "channel": chan,
            "users": info.users.into_iter().collect::<Vec<_>>(),
        })) {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    }
}

/// Broadcast a user's status change to all clients.
async fn broadcast_status(state: &Arc<AppState>, user: &str, status: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "status-update",
        "user": user,
        "status": status,
    })) {
        let _ = state.tx.send(msg);
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
async fn broadcast_new_voice_channel(state: &Arc<AppState>, name: &str, info: &VoiceChannelState) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-add",
        "channel": name,
        "quality": info.quality,
        "bitrate": info.bitrate,
    })) {
        let _ = state.tx.send(msg);
    }
}

async fn broadcast_voice_channel_update(
    state: &Arc<AppState>,
    name: &str,
    info: &VoiceChannelState,
) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-update",
        "channel": name,
        "quality": info.quality,
        "bitrate": info.bitrate,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Determine whether a user is authorised to manage channel state.
///
/// When the server is running without an `ADMIN_TOKEN` every authenticated user
/// is allowed to manage channels for backwards compatibility. Once an admin
/// token is configured we restrict the action to privileged roles only.
async fn can_manage_channels(state: &Arc<AppState>, user: &str) -> bool {
    if state.admin_token.is_none() {
        return true;
    }

    let roles = state.roles.lock().await;
    roles
        .get(user)
        .map(|info| {
            CHANNEL_MANAGE_ROLES
                .iter()
                .any(|allowed| info.role.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
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
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, peer_addr: SocketAddr) {
    let client_ip = peer_addr.ip().to_string();
    info!(%client_ip, "Client connected");
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
                                        if !security::check_auth_rate_limit(&state.rate_limiter, &client_ip).await {
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
                                        {
                                            let mut statuses = state.statuses.lock().await;
                                            statuses.insert(u.to_string(), "online".to_string());
                                        }
                                        broadcast_status(&state, u, "online").await;
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
                                        send_all_statuses(&state, &mut sender).await;
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

                                    let requester = if let Some(name) = user_name.as_deref() {
                                        name
                                    } else {
                                        error!("create-channel requested before presence was fully processed");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    };

                                    if !can_manage_channels(&state, requester).await {
                                        error!("User {requester} attempted to create channel without permission");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    }

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

                                    let requester = if let Some(name) = user_name.as_deref() {
                                        name
                                    } else {
                                        error!("delete-channel requested before presence was fully processed");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    };

                                    if !can_manage_channels(&state, requester).await {
                                        error!("User {requester} attempted to delete channel without permission");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    }

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
                                    if !security::validate_channel_name(ch) {
                                        error!("Invalid voice channel name: {}", ch);
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-channel-name\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    let requester = if let Some(name) = user_name.as_deref() {
                                        name
                                    } else {
                                        error!("create-voice-channel requested before presence was fully processed");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    };

                                    if !can_manage_channels(&state, requester).await {
                                        error!("User {requester} attempted to create voice channel without permission");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    let quality_value = v
                                        .get("quality")
                                        .and_then(|q| q.as_str())
                                        .map(str::trim)
                                        .filter(|q| !q.is_empty())
                                        .map(str::to_string)
                                        .unwrap_or_else(|| DEFAULT_VOICE_QUALITY.to_string());
                                    if !validate_voice_quality(&quality_value) {
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-voice-quality\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    let bitrate_value = match v.get("bitrate") {
                                        Some(val) if val.is_null() => None,
                                        Some(val) => match val.as_i64().and_then(validate_bitrate) {
                                            Some(valid) => Some(valid),
                                            None => {
                                                let _ = sender
                                                    .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-voice-bitrate\"}".into()))
                                                    .await;
                                                continue;
                                            }
                                        },
                                        None => Some(DEFAULT_VOICE_BITRATE),
                                    };

                                    let mut map = state.voice_channels.lock().await;
                                    if !map.contains_key(ch) {
                                        let info = VoiceChannelState {
                                            users: HashSet::new(),
                                            quality: quality_value.clone(),
                                            bitrate: bitrate_value,
                                        };
                                        map.insert(ch.to_string(), info.clone());
                                        drop(map);
                                        let _ =
                                            db::add_voice_channel(&state.db, ch, &info.quality, info.bitrate).await;
                                        broadcast_new_voice_channel(&state, ch, &info).await;
                                    }
                                }
                            }
                            "update-voice-channel" => {
                                if let Some(ch) = v.get("name").and_then(|c| c.as_str()) {
                                    if !security::validate_channel_name(ch) {
                                        error!("Invalid voice channel name for update: {}", ch);
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-channel-name\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    let requester = if let Some(name) = user_name.as_deref() {
                                        name
                                    } else {
                                        error!("update-voice-channel requested before presence was fully processed");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    };

                                    if !can_manage_channels(&state, requester).await {
                                        error!("User {requester} attempted to update voice channel without permission");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    let quality_override = if let Some(raw) =
                                        v.get("quality").and_then(|q| q.as_str())
                                    {
                                        let trimmed = raw.trim();
                                        if !validate_voice_quality(trimmed) {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-voice-quality\"}".into()))
                                                .await;
                                            continue;
                                        }
                                        Some(trimmed.to_string())
                                    } else {
                                        None
                                    };

                                    let bitrate_override = if let Some(val) = v.get("bitrate") {
                                        if val.is_null() {
                                            Some(None)
                                        } else {
                                            match val.as_i64().and_then(validate_bitrate) {
                                                Some(valid) => Some(Some(valid)),
                                                None => {
                                                    let _ = sender
                                                        .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-voice-bitrate\"}".into()))
                                                        .await;
                                                    continue;
                                                }
                                            }
                                        }
                                    } else {
                                        None
                                    };

                                    let current = state.voice_channels.lock().await;
                                    let Some(existing) = current.get(ch).cloned() else {
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"unknown-voice-channel\"}".into()))
                                            .await;
                                        continue;
                                    };
                                    drop(current);

                                    let next_quality =
                                        quality_override.unwrap_or_else(|| existing.quality.clone());
                                    let next_bitrate = match bitrate_override {
                                        Some(value) => value,
                                        None => existing.bitrate,
                                    };

                                    match db::update_voice_channel(
                                        &state.db,
                                        ch,
                                        &next_quality,
                                        next_bitrate,
                                    )
                                    .await
                                    {
                                        Ok(true) => {
                                            let mut map = state.voice_channels.lock().await;
                                            if let Some(entry) = map.get_mut(ch) {
                                                entry.quality = next_quality.clone();
                                                entry.bitrate = next_bitrate;
                                                let snapshot = entry.clone();
                                                drop(map);
                                                broadcast_voice_channel_update(
                                                    &state,
                                                    ch,
                                                    &snapshot,
                                                )
                                                .await;
                                            }
                                        }
                                        Ok(false) => {
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"unknown-voice-channel\"}".into()))
                                                .await;
                                        }
                                        Err(e) => {
                                            error!("Failed to update voice channel {ch}: {e}");
                                            let _ = sender
                                                .send(Message::Text("{\"type\":\"error\",\"message\":\"voice-channel-update-failed\"}".into()))
                                                .await;
                                        }
                                    }
                                }
                            }
                            "delete-voice-channel" => {
                                if let Some(ch) = v.get("name").and_then(|c| c.as_str()) {
                                    if !security::validate_channel_name(ch) {
                                        error!("Invalid voice channel name for deletion: {}", ch);
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"invalid-channel-name\"}".into()))
                                            .await;
                                        continue;
                                    }

                                    let requester = if let Some(name) = user_name.as_deref() {
                                        name
                                    } else {
                                        error!("delete-voice-channel requested before presence was fully processed");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    };

                                    if !can_manage_channels(&state, requester).await {
                                        error!("User {requester} attempted to delete voice channel without permission");
                                        let _ = sender
                                            .send(Message::Text("{\"type\":\"error\",\"message\":\"channel-permission-denied\"}".into()))
                                            .await;
                                        continue;
                                    }

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
                            "status-update" => {
                                let user = match user_name.clone() {
                                    Some(name) => name,
                                    None => {
                                        let msg = serde_json::json!({
                                            "type": "error",
                                            "message": "not-authenticated",
                                        })
                                        .to_string();
                                        let _ = sender.send(Message::Text(msg.into())).await;
                                        continue;
                                    }
                                };

                                let Some(raw_status) = v.get("status").and_then(|s| s.as_str()) else {
                                    let msg = serde_json::json!({
                                        "type": "error",
                                        "message": "invalid-status",
                                    })
                                    .to_string();
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                    continue;
                                };

                                let Some(status) = normalize_status(raw_status) else {
                                    let msg = serde_json::json!({
                                        "type": "error",
                                        "message": "invalid-status",
                                    })
                                    .to_string();
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                    continue;
                                };

                                {
                                    let mut statuses = state.statuses.lock().await;
                                    statuses.insert(user.clone(), status.to_string());
                                }
                                broadcast_status(&state, &user, status).await;
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
                                    for info in map.values_mut() {
                                        info.users.remove(u);
                                    }
                                    let new_channel = !map.contains_key(ch);
                                    let entry = map.entry(ch.to_string()).or_insert_with(|| VoiceChannelState {
                                        users: HashSet::new(),
                                        quality: DEFAULT_VOICE_QUALITY.to_string(),
                                        bitrate: Some(DEFAULT_VOICE_BITRATE),
                                    });
                                    entry.users.insert(u.to_string());
                                    voice_channel = Some(ch.to_string());
                                    let descriptor = entry.clone();
                                    drop(map);
                                    if new_channel {
                                        broadcast_new_voice_channel(&state, ch, &descriptor).await;
                                    }
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
                                    if let Some(info) = map.get_mut(ch) {
                                        info.users.remove(u);
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
        for (ch, info) in map.iter_mut() {
            if info.users.remove(&name) {
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
        {
            let mut statuses = state.statuses.lock().await;
            statuses.insert(name.clone(), "offline".to_string());
        }
        broadcast_status(&state, &name, "offline").await;
    }
    info!(%client_ip, "Client disconnected");
}

/// Axum handler that upgrades the HTTP connection to a WebSocket and
/// spawns [`handle_socket`] for message processing.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}
