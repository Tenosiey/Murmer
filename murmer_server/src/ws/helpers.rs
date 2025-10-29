//! Helper functions for WebSocket message handling.

use crate::{AppState, VoiceChannelState};
use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Utc};
use futures::stream::SplitSink;
use futures::SinkExt;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Collect online users and all known users without holding locks.
pub async fn get_user_lists(state: &Arc<AppState>) -> (Vec<String>, Vec<String>) {
    let online = {
        let users = state.users.lock().await;
        users.iter().cloned().collect()
    };
    let all = {
        let known = state.known_users.lock().await;
        known.iter().cloned().collect()
    };
    (online, all)
}

/// Broadcast the current list of online users to all connected clients.
pub async fn broadcast_users(state: &Arc<AppState>) {
    let (online, all) = get_user_lists(state).await;
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "online-users",
        "users": online,
        "all": all,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send the current list of online and known users to a single client.
pub async fn send_users(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let (online, all) = get_user_lists(state).await;
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "online-users",
        "users": online,
        "all": all,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
    }
}

/// Broadcast the users currently in a voice channel to all clients.
pub async fn broadcast_voice(state: &Arc<AppState>, channel: &str) {
    let list: Vec<String> = {
        let vc = state.voice_channels.lock().await;
        vc.get(channel)
            .map(|info| info.users.iter().cloned().collect())
            .unwrap_or_default()
    };
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-users",
        "channel": channel,
        "users": list,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Sanitize and normalize a message timestamp.
///
/// If the message contains a valid timestamp, it's parsed and normalized.
/// Otherwise, the current time is used.
pub fn sanitize_message_timestamp(value: &mut Value) -> DateTime<Utc> {
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

/// Create a JSON descriptor for a voice channel.
pub fn voice_channel_descriptor(name: &str, info: &VoiceChannelState) -> Value {
    serde_json::json!({
        "name": name,
        "quality": info.quality,
        "bitrate": info.bitrate,
    })
}

/// Broadcast a user's role to all connected clients.
pub async fn broadcast_role(state: &Arc<AppState>, user: &str, role: &str, color: Option<&str>) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "role-update",
        "user": user,
        "role": role,
        "color": color,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send all known roles to a newly connected client.
pub async fn send_all_roles(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
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
pub async fn send_all_statuses(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
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

/// Broadcast a user's status change to all clients.
pub async fn broadcast_status(state: &Arc<AppState>, user: &str, status: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "status-update",
        "user": user,
        "status": status,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a new channel was created.
pub async fn broadcast_new_channel(state: &Arc<AppState>, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-add",
        "channel": name,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a channel was deleted.
pub async fn broadcast_remove_channel(state: &Arc<AppState>, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-remove",
        "channel": name,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a new voice channel was created.
pub async fn broadcast_new_voice_channel(
    state: &Arc<AppState>,
    name: &str,
    info: &VoiceChannelState,
) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-add",
        "channel": name,
        "quality": info.quality,
        "bitrate": info.bitrate,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast an update to a voice channel's configuration.
pub async fn broadcast_voice_channel_update(
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

/// Broadcast to all clients that a voice channel was deleted.
pub async fn broadcast_remove_voice_channel(state: &Arc<AppState>, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-remove",
        "channel": name,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send the list of available channels to a client.
pub async fn send_channels(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let list = crate::db::get_channels(&state.db).await;
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-list",
        "channels": list,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
    }
}

/// Send the list of available voice channels to a client.
pub async fn send_voice_channels(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let mut entries: Vec<(String, VoiceChannelState)> = {
        let map = state.voice_channels.lock().await;
        map.iter()
            .map(|(name, info)| (name.clone(), info.clone()))
            .collect()
    };
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let channels: Vec<Value> = entries
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
pub async fn send_all_voice(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
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

/// Determine whether a user is authorised to manage channel state.
///
/// When the server is running without an `ADMIN_TOKEN` every authenticated user
/// is allowed to manage channels for backwards compatibility. Once an admin
/// token is configured we restrict the action to privileged roles only.
pub async fn can_manage_channels(state: &Arc<AppState>, user: &str) -> bool {
    if state.admin_token.is_none() {
        return true;
    }

    let roles = state.roles.lock().await;
    roles
        .get(user)
        .map(|info| {
            super::constants::CHANNEL_MANAGE_ROLES
                .iter()
                .any(|allowed| info.role.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

/// Retrieve the broadcast channel for the given chat room, creating it if necessary.
pub async fn get_or_create_channel(
    state: &Arc<AppState>,
    name: &str,
) -> tokio::sync::broadcast::Sender<String> {
    let mut channels = state.channels.lock().await;
    channels
        .entry(name.to_string())
        .or_insert_with(|| tokio::sync::broadcast::channel::<String>(100).0)
        .clone()
}

/// Ensure reactions field exists and is a valid empty object if missing.
pub fn ensure_reactions(value: &mut Value) {
    if value.get("reactions").is_none() {
        value["reactions"] = Value::Object(Map::new());
    }
}

/// Ensure time field exists if missing, using the provided timestamp.
pub fn ensure_time(value: &mut Value, timestamp: &DateTime<Utc>) {
    if value.get("time").and_then(|t| t.as_str()).is_none() {
        value["time"] = Value::String(timestamp.format("%H:%M:%S").to_string());
    }
}

