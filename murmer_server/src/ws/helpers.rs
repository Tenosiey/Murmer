//! Helper functions for WebSocket message handling.

use crate::{db, AppState, VoiceChannelState};
use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::stream::SplitSink;
use futures::SinkExt;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::error;

/// Send a pre-serialized error frame (see [`crate::ws::errors`]) to one client.
/// Send failures are ignored; the socket loop notices a dead connection itself.
pub async fn send_error(sender: &mut SplitSink<WebSocket, Message>, error_json: &str) {
    let _ = sender
        .send(Message::Text(error_json.to_string().into()))
        .await;
}

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
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Broadcast the users currently in a voice channel to all clients.
pub async fn broadcast_voice(state: &Arc<AppState>, channel_id: i32) {
    let list: Vec<String> = {
        let vc = state.voice_channels.lock().await;
        vc.get(&channel_id)
            .map(|info| info.users.iter().cloned().collect())
            .unwrap_or_default()
    };
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-users",
        "channelId": channel_id,
        "users": list,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Sanitize and normalize a message timestamp.
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
pub fn voice_channel_descriptor(id: i32, info: &VoiceChannelState) -> Value {
    serde_json::json!({
        "id": id,
        "name": info.name,
        "quality": info.quality,
        "bitrate": info.bitrate,
        "categoryId": info.category_id,
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
        #[allow(clippy::collapsible_if)]
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "role-update",
            "user": user,
            "role": info.role,
            "color": info.color,
        })) {
            if sender.send(Message::Text(msg.into())).await.is_err() {
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
        let _ = sender.send(Message::Text(msg.into())).await;
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
pub async fn broadcast_new_channel(state: &Arc<AppState>, record: &crate::db::ChannelRecord) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-add",
        "channelId": record.id,
        "name": record.name,
        "categoryId": record.category_id,
        "topic": record.description,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast a channel's updated topic/description to all clients.
pub async fn broadcast_channel_topic(state: &Arc<AppState>, channel_id: i32, topic: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-topic",
        "channelId": channel_id,
        "topic": topic,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a channel was deleted.
pub async fn broadcast_remove_channel(state: &Arc<AppState>, channel_id: i32) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-remove",
        "channelId": channel_id,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a new voice channel was created.
pub async fn broadcast_new_voice_channel(state: &Arc<AppState>, id: i32, info: &VoiceChannelState) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-add",
        "channelId": id,
        "name": info.name,
        "quality": info.quality,
        "bitrate": info.bitrate,
        "categoryId": info.category_id,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast an update to a voice channel's configuration.
pub async fn broadcast_voice_channel_update(
    state: &Arc<AppState>,
    channel_id: i32,
    info: &VoiceChannelState,
) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-update",
        "channelId": channel_id,
        "name": info.name,
        "quality": info.quality,
        "bitrate": info.bitrate,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a voice channel was deleted.
pub async fn broadcast_remove_voice_channel(state: &Arc<AppState>, channel_id: i32) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-remove",
        "channelId": channel_id,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send the list of available channels to a client.
pub async fn send_channels(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let list = crate::db::get_channels(&state.db).await;
    let channels: Vec<Value> = list
        .iter()
        .map(|ch| {
            serde_json::json!({
                "id": ch.id,
                "name": ch.name,
                "categoryId": ch.category_id,
                "topic": ch.description,
            })
        })
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-list",
        "channels": channels,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Send the list of categories to a client.
pub async fn send_categories(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let list = crate::db::get_categories(&state.db).await;
    let categories: Vec<Value> = list
        .iter()
        .map(|cat| {
            serde_json::json!({
                "id": cat.id,
                "name": cat.name,
                "position": cat.position,
            })
        })
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "category-list",
        "categories": categories,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Send the list of available voice channels to a client.
pub async fn send_voice_channels(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let mut entries: Vec<(i32, VoiceChannelState)> = {
        let map = state.voice_channels.lock().await;
        map.iter().map(|(id, info)| (*id, info.clone())).collect()
    };
    entries.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    let channels: Vec<Value> = entries
        .iter()
        .map(|(id, info)| voice_channel_descriptor(*id, info))
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-list",
        "channels": channels,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Send all voice channel member lists to a client.
pub async fn send_all_voice(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let map = state.voice_channels.lock().await.clone();
    for (id, info) in map {
        #[allow(clippy::collapsible_if)]
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "voice-users",
            "channelId": id,
            "users": info.users.into_iter().collect::<Vec<_>>(),
        })) {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    }
}

/// Broadcast to all clients that a new category was created.
pub async fn broadcast_new_category(state: &Arc<AppState>, id: i32, name: &str, position: i32) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "category-add",
        "id": id,
        "name": name,
        "position": position,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a category was renamed.
pub async fn broadcast_rename_category(state: &Arc<AppState>, id: i32, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "category-update",
        "id": id,
        "name": name,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a category was deleted.
pub async fn broadcast_remove_category(state: &Arc<AppState>, id: i32) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "category-remove",
        "id": id,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast to all clients that a channel was moved to a different category.
pub async fn broadcast_channel_move(
    state: &Arc<AppState>,
    channel_id: i32,
    category_id: Option<i32>,
    voice: bool,
) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-move",
        "channelId": channel_id,
        "categoryId": category_id,
        "voice": voice,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Determine whether a user is authorised to manage channel state.
///
/// Without an `ADMIN_TOKEN` configured every authenticated user may manage
/// channels, so a small unadministered server stays fully usable. Once an
/// admin token is configured the action is restricted to privileged roles.
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

/// Rank of a role for moderation purposes. Moderation actions require the
/// requester to strictly outrank the target, so Mods cannot act against
/// Admins/Owners and equally ranked users cannot act against each other.
pub fn moderation_rank(role: Option<&str>) -> u8 {
    match role {
        Some(role) if role.eq_ignore_ascii_case("Owner") => 3,
        Some(role) if role.eq_ignore_ascii_case("Admin") => 2,
        Some(role) if role.eq_ignore_ascii_case("Mod") => 1,
        _ => 0,
    }
}

/// Fetch the moderation rank of a user from the in-memory role map.
pub async fn user_moderation_rank(state: &Arc<AppState>, user: &str) -> u8 {
    let roles = state.roles.lock().await;
    moderation_rank(roles.get(user).map(|info| info.role.as_str()))
}

/// Determine whether a user is authorised to see server details such as the
/// running version. Restricted to Owner and Admin roles.
pub async fn can_view_server_info(state: &Arc<AppState>, user: &str) -> bool {
    let roles = state.roles.lock().await;
    roles
        .get(user)
        .map(|info| {
            super::constants::SERVER_INFO_ROLES
                .iter()
                .any(|allowed| info.role.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

/// Determine whether a user is authorised to view other users' self-reported
/// connection stats. Restricted to Owner and Admin roles.
pub async fn can_view_connection_stats(state: &Arc<AppState>, user: &str) -> bool {
    let roles = state.roles.lock().await;
    roles
        .get(user)
        .map(|info| {
            super::constants::CONNECTION_STATS_ROLES
                .iter()
                .any(|allowed| info.role.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

/// Determine whether a user is authorised to manage roles.
pub async fn can_manage_roles(state: &Arc<AppState>, user: &str) -> bool {
    let roles = state.roles.lock().await;
    roles
        .get(user)
        .map(|info| {
            super::constants::ROLE_MANAGE_ROLES
                .iter()
                .any(|allowed| info.role.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

/// Delete an ephemeral message once its expiry passes and announce the
/// deletion to the message's channel. Runs as a background task; the delay is
/// clamped to the ephemeral maximum so a corrupted expiry cannot schedule a
/// task years into the future.
pub fn schedule_ephemeral_deletion(
    state: Arc<AppState>,
    message_id: i64,
    channel_id: i32,
    expiry: DateTime<Utc>,
) {
    tokio::spawn(async move {
        let delay = expiry.signed_duration_since(Utc::now()).clamp(
            ChronoDuration::zero(),
            ChronoDuration::seconds(super::constants::MAX_EPHEMERAL_SECONDS),
        );
        if let Ok(duration) = delay.to_std() {
            tokio::time::sleep(duration).await;
        }
        let Ok(id32) = i32::try_from(message_id) else {
            return;
        };
        match db::delete_message(&state.db, id32).await {
            Ok(true) => {
                let payload = serde_json::json!({
                    "type": "message-deleted",
                    "id": message_id,
                    "channelId": channel_id,
                });
                let chan_tx = get_or_create_channel(&state, channel_id).await;
                let _ = chan_tx.send(payload.to_string());
            }
            // Already gone (deleted manually or by an earlier run).
            Ok(false) => {}
            Err(e) => error!("failed to delete ephemeral message {message_id}: {e}"),
        }
    });
}

/// Re-schedule the deletion of ephemeral messages found in the database at
/// startup. Deletion timers only live in memory, so without this sweep an
/// ephemeral message whose timer was lost to a server restart would never be
/// removed.
pub async fn resume_ephemeral_deletions(state: &Arc<AppState>) {
    let rows = match db::get_ephemeral_messages(&state.db).await {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to scan for leftover ephemeral messages: {e}");
            return;
        }
    };
    for (id, channel_id, content) in rows {
        let Ok(parsed) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if parsed.get("ephemeral").and_then(|e| e.as_bool()) != Some(true) {
            continue;
        }
        let expiry = parsed
            .get("expiresAt")
            .and_then(|e| e.as_str())
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|dt| dt.with_timezone(&Utc))
            // No parseable expiry on an ephemeral message: delete right away.
            .unwrap_or_else(Utc::now);
        schedule_ephemeral_deletion(Arc::clone(state), id, channel_id, expiry);
    }
}

/// Retrieve the broadcast channel for the given channel ID, creating it if necessary.
pub async fn get_or_create_channel(
    state: &Arc<AppState>,
    channel_id: i32,
) -> tokio::sync::broadcast::Sender<String> {
    let mut channels = state.channels.lock().await;
    channels
        .entry(channel_id)
        .or_insert_with(|| tokio::sync::broadcast::channel::<String>(100).0)
        .clone()
}

/// Truncate quoted text for a reply snippet, respecting UTF-8 character
/// boundaries so multi-byte characters are never split.
pub fn reply_preview(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Whether `user` is the sender or recipient of a direct-message frame.
/// The socket loop uses this to keep DMs private on the shared broadcast.
pub fn dm_involves(v: &Value, user: Option<&str>) -> bool {
    let Some(user) = user else {
        return false;
    };
    v.get("from").and_then(|f| f.as_str()) == Some(user)
        || v.get("to").and_then(|t| t.as_str()) == Some(user)
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
