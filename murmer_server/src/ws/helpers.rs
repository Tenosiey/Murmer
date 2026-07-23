//! Helper functions for WebSocket message handling.

use crate::channel_overrides::ChannelKind;
use crate::permissions::{self, Permissions};
use crate::roles::RoleDef;
use crate::{AppState, VoiceChannelState, db};
use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::SinkExt;
use futures::stream::SplitSink;
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
        "position": info.position,
    })
}

/// Serialize the full role-definition list as a `role-definitions` frame,
/// ordered from lowest to highest position.
fn role_definitions_frame(defs: &HashMap<i64, RoleDef>) -> Option<String> {
    let mut list: Vec<&RoleDef> = defs.values().collect();
    list.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
    let roles: Vec<Value> = list
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "name": d.name,
                "color": d.color,
                "permissions": d.permissions,
                "position": d.position,
                "isDefault": d.is_default,
                "isOwner": d.is_owner,
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "type": "role-definitions",
        "roles": roles,
    }))
    .ok()
}

/// Broadcast the full set of role definitions to all connected clients.
pub async fn broadcast_role_definitions(state: &Arc<AppState>) {
    let defs = state.role_defs.lock().await;
    if let Some(msg) = role_definitions_frame(&defs) {
        let _ = state.tx.send(msg);
    }
}

/// Send the full set of role definitions to a single client.
pub async fn send_role_definitions(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let defs = state.role_defs.lock().await;
    if let Some(msg) = role_definitions_frame(&defs) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Broadcast one user's assigned role ids to all connected clients.
pub async fn broadcast_user_roles(state: &Arc<AppState>, user: &str, role_ids: &[i64]) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "user-roles",
        "user": user,
        "roleIds": role_ids,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Send every connected user's role assignments to a newly connected client.
pub async fn send_all_user_roles(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let assignments = state.user_roles.lock().await.clone();
    for (user, ids) in assignments {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "user-roles",
            "user": user,
            "roleIds": ids,
        })) && sender.send(Message::Text(msg.into())).await.is_err()
        {
            break;
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
        "position": record.position,
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

/// Broadcast a channel's new name to all clients.
pub async fn broadcast_channel_rename(state: &Arc<AppState>, channel_id: i32, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-rename",
        "channelId": channel_id,
        "name": name,
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
        "position": info.position,
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

/// Broadcast a voice channel's new name to all clients.
pub async fn broadcast_voice_channel_rename(state: &Arc<AppState>, channel_id: i32, name: &str) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-rename",
        "channelId": channel_id,
        "name": name,
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
pub async fn send_channels(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: Option<&str>,
) {
    if let Ok(msg) = channel_list_frame(state, user).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Build a `channel-list` frame containing only the text channels `user` can
/// see, each carrying a `private` flag for the lock indicator.
pub async fn channel_list_frame(
    state: &Arc<AppState>,
    user: Option<&str>,
) -> serde_json::Result<String> {
    let list = crate::db::get_channels(&state.db).await;
    let mut channels: Vec<Value> = Vec::new();
    for ch in &list {
        if !user_can_see_channel(state, user, ChannelKind::Text, ch.id).await {
            continue;
        }
        channels.push(serde_json::json!({
            "id": ch.id,
            "name": ch.name,
            "categoryId": ch.category_id,
            "topic": ch.description,
            "position": ch.position,
            "private": channel_is_private(state, ChannelKind::Text, ch.id).await,
        }));
    }
    serde_json::to_string(&serde_json::json!({
        "type": "channel-list",
        "channels": channels,
    }))
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
    user: Option<&str>,
) {
    if let Ok(msg) = voice_channel_list_frame(state, user).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Build a `voice-channel-list` frame containing only the voice channels `user`
/// can see, each carrying a `private` flag.
pub async fn voice_channel_list_frame(
    state: &Arc<AppState>,
    user: Option<&str>,
) -> serde_json::Result<String> {
    let mut entries: Vec<(i32, VoiceChannelState)> = {
        let map = state.voice_channels.lock().await;
        map.iter().map(|(id, info)| (*id, info.clone())).collect()
    };
    entries.sort_by(|a, b| (a.1.position, &a.1.name).cmp(&(b.1.position, &b.1.name)));
    let mut channels: Vec<Value> = Vec::new();
    for (id, info) in &entries {
        if !user_can_see_channel(state, user, ChannelKind::Voice, *id).await {
            continue;
        }
        let mut descriptor = voice_channel_descriptor(*id, info);
        descriptor["private"] =
            Value::Bool(channel_is_private(state, ChannelKind::Voice, *id).await);
        channels.push(descriptor);
    }
    serde_json::to_string(&serde_json::json!({
        "type": "voice-channel-list",
        "channels": channels,
    }))
}

/// Send all voice channel member lists to a client.
pub async fn send_all_voice(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let map = state.voice_channels.lock().await.clone();
    for (id, info) in map {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "voice-users",
            "channelId": id,
            "users": info.users.into_iter().collect::<Vec<_>>(),
        })) && sender.send(Message::Text(msg.into())).await.is_err()
        {
            break;
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

/// Broadcast to all clients that a channel was moved to a different category
/// (appended at `position`, the end of the target category).
pub async fn broadcast_channel_move(
    state: &Arc<AppState>,
    channel_id: i32,
    category_id: Option<i32>,
    position: i32,
    voice: bool,
) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-move",
        "channelId": channel_id,
        "categoryId": category_id,
        "position": position,
        "voice": voice,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast the new order of one category's text or voice channels; the
/// listed channels now live in `category_id` at their list index.
pub async fn broadcast_channel_reorder(
    state: &Arc<AppState>,
    category_id: Option<i32>,
    order: &[i32],
    voice: bool,
) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-reorder",
        "categoryId": category_id,
        "order": order,
        "voice": voice,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Broadcast the new order of all categories.
pub async fn broadcast_category_reorder(state: &Arc<AppState>, order: &[i32]) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "category-reorder",
        "order": order,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// A user's effective permission mask: the union of the default `@everyone`
/// role and every role assigned to them. Holding [`ADMINISTRATOR`] expands to
/// the full permission set. Every authorization check funnels through this so
/// the in-memory role state is the single source of truth.
///
/// Server-wide only for now; a future per-channel override phase will resolve
/// against a channel id here without changing the call sites.
pub async fn effective_permissions(state: &Arc<AppState>, user: &str) -> Permissions {
    // Lock order is always role_defs before user_roles; keep it consistent
    // with `top_position` to avoid deadlocks.
    let defs = state.role_defs.lock().await;
    let mut mask = defs
        .values()
        .find(|d| d.is_default)
        .map(|d| d.permissions)
        .unwrap_or(0);
    {
        let assignments = state.user_roles.lock().await;
        if let Some(ids) = assignments.get(user) {
            for id in ids {
                if let Some(def) = defs.get(id) {
                    mask |= def.permissions;
                }
            }
        }
    }
    if mask & permissions::ADMINISTRATOR != 0 {
        permissions::ALL
    } else {
        mask
    }
}

/// Whether `user` is authorised for `required`.
///
/// Without an `ADMIN_TOKEN` configured, channel and wiki management stay open
/// to everyone so a small unadministered server remains usable (mirrors the
/// historical fallback). Every other permission is always role-gated.
pub async fn has_permission(state: &Arc<AppState>, user: &str, required: Permissions) -> bool {
    if state.admin_token.is_none()
        && (required == permissions::MANAGE_CHANNELS || required == permissions::MANAGE_WIKI)
    {
        return true;
    }
    permissions::mask_allows(effective_permissions(state, user).await, required)
}

/// A user's hierarchy position: the highest `position` among their roles, with
/// the default role's position as the floor. Administrators sit above everyone
/// (used so moderation and role management require strictly outranking the
/// target). Returns [`i64::MAX`] for administrators.
pub async fn top_position(state: &Arc<AppState>, user: &str) -> i64 {
    let defs = state.role_defs.lock().await;
    let default = defs.values().find(|d| d.is_default);
    let mut pos = default.map(|d| d.position).unwrap_or(0);
    let mut is_admin = default
        .map(|d| d.permissions & permissions::ADMINISTRATOR != 0)
        .unwrap_or(false);
    {
        let assignments = state.user_roles.lock().await;
        if let Some(ids) = assignments.get(user) {
            for id in ids {
                if let Some(def) = defs.get(id) {
                    pos = pos.max(def.position);
                    if def.permissions & permissions::ADMINISTRATOR != 0 {
                        is_admin = true;
                    }
                }
            }
        }
    }
    if is_admin { i64::MAX } else { pos }
}

/// A user's effective permission mask **within a specific channel**: the
/// server-wide mask with the channel's overrides applied. Administrators bypass
/// overrides entirely. Server-wide only for non-channel checks; this is the
/// entry point for every per-channel gate.
pub async fn channel_permissions(
    state: &Arc<AppState>,
    user: &str,
    kind: ChannelKind,
    channel_id: i32,
) -> Permissions {
    let base = effective_permissions(state, user).await;
    if base & permissions::ADMINISTRATOR != 0 {
        return permissions::ALL;
    }

    let overrides = {
        let map = state.channel_overrides.lock().await;
        match map.get(&(kind, channel_id)) {
            Some(set) => set.clone(),
            None => return base,
        }
    };

    let role_ids: Vec<i64> = {
        let assignments = state.user_roles.lock().await;
        assignments.get(user).cloned().unwrap_or_default()
    };
    let user_key = lookup_user_key(state, user).await;
    overrides.apply(base, &role_ids, user_key.as_deref())
}

/// Whether `user` may see a channel. Managers (server-wide `MANAGE_CHANNELS`,
/// which also covers the no-`ADMIN_TOKEN` fallback) and administrators always
/// can, so they can administer private channels; everyone else needs
/// `VIEW_CHANNELS` after overrides.
pub async fn can_view_channel(
    state: &Arc<AppState>,
    user: &str,
    kind: ChannelKind,
    channel_id: i32,
) -> bool {
    if has_permission(state, user, permissions::MANAGE_CHANNELS).await {
        return true;
    }
    channel_permissions(state, user, kind, channel_id).await & permissions::VIEW_CHANNELS != 0
}

/// Whether `user` holds `required` within a channel (e.g. `SEND_MESSAGES`).
pub async fn has_channel_permission(
    state: &Arc<AppState>,
    user: &str,
    kind: ChannelKind,
    channel_id: i32,
    required: Permissions,
) -> bool {
    permissions::mask_allows(
        channel_permissions(state, user, kind, channel_id).await,
        required,
    )
}

/// Broadcast a signal telling every connection to rebuild its own filtered
/// channel and voice lists. Sent when a channel's permission overrides change,
/// so private channels appear/disappear per viewer without leaking structure.
pub async fn broadcast_channels_refresh(state: &Arc<AppState>) {
    let _ = state.tx.send(r#"{"type":"channels-refresh"}"#.to_string());
}

/// Send the override list for one channel to a single (manager) client.
pub async fn send_channel_overrides(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    kind: ChannelKind,
    channel_id: i32,
) {
    let rows = db::get_channel_overrides(&state.db, kind, channel_id)
        .await
        .unwrap_or_default();
    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "targetType": r.target_type,
                "targetId": r.target_id,
                "targetLabel": r.target_label,
                "allow": r.allow,
                "deny": r.deny,
            })
        })
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "channel-overrides",
        "channelId": channel_id,
        "voice": kind == ChannelKind::Voice,
        "overrides": entries,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Whether a channel is private (its `@everyone` override denies View), read
/// from the in-memory cache. Used to mark channels with a lock and to hide
/// them from anonymous viewers.
pub async fn channel_is_private(state: &Arc<AppState>, kind: ChannelKind, channel_id: i32) -> bool {
    let map = state.channel_overrides.lock().await;
    map.get(&(kind, channel_id))
        .map(|set| set.restricts_view())
        .unwrap_or(false)
}

/// Whether a (possibly anonymous) connection may see a channel. Named users go
/// through the full override resolution; a keyless anonymous connection only
/// sees non-private channels.
pub async fn user_can_see_channel(
    state: &Arc<AppState>,
    user: Option<&str>,
    kind: ChannelKind,
    channel_id: i32,
) -> bool {
    match user {
        Some(u) => can_view_channel(state, u, kind, channel_id).await,
        None => !channel_is_private(state, kind, channel_id).await,
    }
}

/// Serialize the current custom emoji list as an `emoji-list` frame.
async fn emoji_list_frame(state: &Arc<AppState>) -> Option<String> {
    let emojis = match db::get_emojis(&state.db).await {
        Ok(list) => list,
        Err(e) => {
            error!("Failed to load custom emojis: {e}");
            return None;
        }
    };
    let entries: Vec<Value> = emojis
        .iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "url": e.url,
                "uploadedBy": e.uploaded_by,
                "createdAt": e.created_at,
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "type": "emoji-list",
        "emojis": entries,
    }))
    .ok()
}

/// Send the current custom emoji list to a single client.
pub async fn send_emojis(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    if let Some(msg) = emoji_list_frame(state).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Broadcast the current custom emoji list to all connected clients.
pub async fn broadcast_emojis(state: &Arc<AppState>) {
    if let Some(msg) = emoji_list_frame(state).await {
        let _ = state.tx.send(msg);
    }
}

/// Resolve a user's public key: currently connected users first (in-memory
/// map), then the persisted name binding, so moderation and role changes
/// also reach users who are offline.
pub async fn lookup_user_key(state: &Arc<AppState>, user: &str) -> Option<String> {
    if let Some(key) = state.user_keys.lock().await.get(user).cloned() {
        return Some(key);
    }
    match db::get_user_key(&state.db, user).await {
        Ok(key) => key,
        Err(e) => {
            error!("Failed to look up key binding for {user}: {e}");
            None
        }
    }
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
        match db::delete_message(&state.db, message_id).await {
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

/// Why a direct message's encryption fields were rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum DmPayloadError {
    /// Nonce or ciphertext is not valid base64, has the wrong nonce length,
    /// or the ciphertext is too short to contain the authenticator.
    Malformed,
    /// Decoded ciphertext exceeds the plaintext limit plus box overhead.
    TooLong,
}

/// Validate the encryption fields of a direct-message frame. Direct messages
/// are end-to-end encrypted, so the server never sees their plaintext; this
/// shape check (base64, exact nonce length, bounded ciphertext size) is the
/// only content validation possible before storing the frame verbatim.
pub fn validate_dm_payload(nonce_b64: &str, ciphertext_b64: &str) -> Result<(), DmPayloadError> {
    use base64::{Engine as _, engine::general_purpose};

    let nonce = general_purpose::STANDARD
        .decode(nonce_b64)
        .map_err(|_| DmPayloadError::Malformed)?;
    if nonce.len() != super::constants::DM_NONCE_BYTES {
        return Err(DmPayloadError::Malformed);
    }

    let ciphertext = general_purpose::STANDARD
        .decode(ciphertext_b64)
        .map_err(|_| DmPayloadError::Malformed)?;
    // The authenticator alone (empty plaintext) is also rejected: clients
    // never send empty messages.
    if ciphertext.len() <= super::constants::DM_CIPHERTEXT_OVERHEAD_BYTES {
        return Err(DmPayloadError::Malformed);
    }
    if ciphertext.len()
        > super::constants::MAX_MESSAGE_LENGTH + super::constants::DM_CIPHERTEXT_OVERHEAD_BYTES
    {
        return Err(DmPayloadError::TooLong);
    }
    Ok(())
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
