//! Handlers for text channel, voice channel and category management.

use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, VoiceChannelState, db, security};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::error;

/// Handle create channel request.
pub(super) async fn handle_create_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch) = v.get("name").and_then(|c| c.as_str()) else {
        return;
    };

    if !security::validate_channel_name(ch) {
        error!("Invalid channel name: {}", ch);
        send_error(sender, errors::INVALID_CHANNEL_NAME).await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("create-channel requested before presence was fully processed");
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to create channel without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    let category_id = v
        .get("categoryId")
        .and_then(|c| c.as_i64())
        .map(|id| id as i32);

    match db::add_channel(&state.db, ch, category_id).await {
        Ok(Some(record)) => {
            get_or_create_channel(state, record.id).await;
            broadcast_new_channel(state, &record).await;
        }
        Ok(None) => {}
        Err(e) => {
            error!("db add channel error: {e}");
            send_error(sender, errors::CHANNEL_CREATION_FAILED).await;
        }
    }
}

/// Handle delete channel request.
// The per-connection channel state (id, tx, rx) is deliberately passed as
// individual &mut bindings from the ws dispatch loop.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_delete_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
    channel_id: &mut i32,
    chan_tx: &mut tokio::sync::broadcast::Sender<String>,
    chan_rx: &mut tokio::sync::broadcast::Receiver<String>,
    default_channel_id: i32,
) -> Result<(), ()> {
    let Some(ch_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return Ok(());
    };

    let record = match db::get_channel_by_id(&state.db, ch_id).await {
        Some(r) => r,
        None => return Ok(()),
    };

    if record.name == "general" {
        send_error(sender, errors::CANNOT_DELETE_GENERAL).await;
        return Err(());
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("delete-channel requested before presence was fully processed");
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return Err(());
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to delete channel without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return Err(());
    }

    match db::remove_channel(&state.db, ch_id).await {
        Err(e) => {
            error!("db remove channel error: {e}");
            send_error(sender, errors::CHANNEL_DELETION_FAILED).await;
        }
        Ok(_) => {
            state.channels.lock().await.remove(&ch_id);
            broadcast_remove_channel(state, ch_id).await;
            if *channel_id == ch_id {
                *channel_id = default_channel_id;
                *chan_tx = get_or_create_channel(state, *channel_id).await;
                *chan_rx = chan_tx.subscribe();
            }
        }
    }

    Ok(())
}

/// Handle move channel to a category (or remove from category).
pub(super) async fn handle_move_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    let category_id = if v.get("categoryId").is_some_and(|c| c.is_null()) {
        None
    } else {
        v.get("categoryId")
            .and_then(|c| c.as_i64())
            .map(|i| i as i32)
    };

    let is_voice = v.get("voice").and_then(|v| v.as_bool()).unwrap_or(false);

    let result = if is_voice {
        match db::move_voice_channel(&state.db, ch_id, category_id).await {
            Ok(updated) => {
                if updated {
                    let mut map = state.voice_channels.lock().await;
                    if let Some(entry) = map.get_mut(&ch_id) {
                        entry.category_id = category_id;
                    }
                }
                Ok(updated)
            }
            Err(e) => Err(e),
        }
    } else {
        db::move_channel(&state.db, ch_id, category_id).await
    };

    match result {
        Ok(true) => {
            broadcast_channel_move(state, ch_id, category_id, is_voice).await;
        }
        Ok(false) => {
            send_error(sender, errors::CHANNEL_MOVE_FAILED).await;
        }
        Err(e) => {
            error!("db move channel error: {e}");
            send_error(sender, errors::CHANNEL_MOVE_FAILED).await;
        }
    }
}

/// Handle set channel topic request.
pub(super) async fn handle_set_channel_topic(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return;
    };
    let Some(raw_topic) = v.get("topic").and_then(|t| t.as_str()) else {
        return;
    };

    let topic = raw_topic.trim();
    if !validate_channel_topic(topic) {
        send_error(sender, errors::INVALID_CHANNEL_TOPIC).await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to set channel topic without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    match db::set_channel_description(&state.db, ch_id, topic).await {
        Ok(true) => {
            broadcast_channel_topic(state, ch_id, topic).await;
        }
        Ok(false) => {
            send_error(sender, errors::UNKNOWN_CHANNEL).await;
        }
        Err(e) => {
            error!("db set channel topic error: {e}");
            send_error(sender, errors::TOPIC_UPDATE_FAILED).await;
        }
    }
}

/// Handle create voice channel request.
pub(super) async fn handle_create_voice_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch) = v.get("name").and_then(|c| c.as_str()) else {
        return;
    };

    if !security::validate_channel_name(ch) {
        error!("Invalid voice channel name: {}", ch);
        send_error(sender, errors::INVALID_CHANNEL_NAME).await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("create-voice-channel requested before presence was fully processed");
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to create voice channel without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    let quality_value = v
        .get("quality")
        .and_then(|q| q.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_VOICE_QUALITY.to_string());
    if !validate_voice_quality(&quality_value) {
        send_error(sender, errors::INVALID_VOICE_QUALITY).await;
        return;
    }

    let bitrate_value = match v.get("bitrate") {
        Some(val) if val.is_null() => None,
        Some(val) => match val.as_i64().and_then(validate_bitrate) {
            Some(valid) => Some(valid),
            None => {
                send_error(sender, errors::INVALID_VOICE_BITRATE).await;
                return;
            }
        },
        None => Some(DEFAULT_VOICE_BITRATE),
    };

    let category_id = v
        .get("categoryId")
        .and_then(|c| c.as_i64())
        .map(|id| id as i32);

    match db::add_voice_channel(&state.db, ch, &quality_value, bitrate_value, category_id).await {
        Ok(Some(record)) => {
            let info = VoiceChannelState {
                name: record.name.clone(),
                users: HashSet::new(),
                quality: record.quality.clone(),
                bitrate: record.bitrate,
                category_id: record.category_id,
            };
            state
                .voice_channels
                .lock()
                .await
                .insert(record.id, info.clone());
            broadcast_new_voice_channel(state, record.id, &info).await;
        }
        Ok(None) => {}
        Err(e) => {
            error!("db add voice channel error: {e}");
        }
    }
}

/// Handle update voice channel request.
pub(super) async fn handle_update_voice_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("update-voice-channel requested before presence was fully processed");
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to update voice channel without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    let quality_override = if let Some(raw) = v.get("quality").and_then(|q| q.as_str()) {
        let trimmed = raw.trim();
        if !validate_voice_quality(trimmed) {
            send_error(sender, errors::INVALID_VOICE_QUALITY).await;
            return;
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
                    send_error(sender, errors::INVALID_VOICE_BITRATE).await;
                    return;
                }
            }
        }
    } else {
        None
    };

    let current = state.voice_channels.lock().await;
    let Some(existing) = current.get(&ch_id).cloned() else {
        send_error(sender, errors::UNKNOWN_VOICE_CHANNEL).await;
        return;
    };
    drop(current);

    let next_quality = quality_override.unwrap_or_else(|| existing.quality.clone());
    let next_bitrate = match bitrate_override {
        Some(value) => value,
        None => existing.bitrate,
    };

    match db::update_voice_channel(&state.db, ch_id, &next_quality, next_bitrate).await {
        Ok(true) => {
            let mut map = state.voice_channels.lock().await;
            if let Some(entry) = map.get_mut(&ch_id) {
                entry.quality = next_quality.clone();
                entry.bitrate = next_bitrate;
                let snapshot = entry.clone();
                drop(map);
                broadcast_voice_channel_update(state, ch_id, &snapshot).await;
            }
        }
        Ok(false) => {
            send_error(sender, errors::UNKNOWN_VOICE_CHANNEL).await;
        }
        Err(e) => {
            error!("Failed to update voice channel {ch_id}: {e}");
            send_error(sender, errors::VOICE_CHANNEL_UPDATE_FAILED).await;
        }
    }
}

/// Handle delete voice channel request.
pub(super) async fn handle_delete_voice_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
    voice_channel: &mut Option<i32>,
) {
    let Some(ch_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("delete-voice-channel requested before presence was fully processed");
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to delete voice channel without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    state.voice_channels.lock().await.remove(&ch_id);
    let _ = db::remove_voice_channel(&state.db, ch_id).await;
    broadcast_remove_voice_channel(state, ch_id).await;
    if *voice_channel == Some(ch_id) {
        *voice_channel = None;
    }
}

/// Handle create category request.
pub(super) async fn handle_create_category(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(name) = v.get("name").and_then(|n| n.as_str()) else {
        return;
    };

    if !security::validate_channel_name(name) {
        send_error(sender, errors::INVALID_CATEGORY_NAME).await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    let position = v
        .get("position")
        .and_then(|p| p.as_i64())
        .map(|p| p as i32)
        .unwrap_or(0);

    match db::add_category(&state.db, name, position).await {
        Ok(id) => {
            broadcast_new_category(state, id, name, position).await;
        }
        Err(e) => {
            error!("db add category error: {e}");
            send_error(sender, errors::CATEGORY_CREATION_FAILED).await;
        }
    }
}

/// Handle rename category request.
pub(super) async fn handle_rename_category(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(id) = v.get("id").and_then(|i| i.as_i64()).map(|i| i as i32) else {
        return;
    };
    let Some(name) = v.get("name").and_then(|n| n.as_str()) else {
        return;
    };

    if !security::validate_channel_name(name) {
        send_error(sender, errors::INVALID_CATEGORY_NAME).await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    match db::rename_category(&state.db, id, name).await {
        Ok(true) => {
            broadcast_rename_category(state, id, name).await;
        }
        Ok(false) => {
            send_error(sender, errors::UNKNOWN_CATEGORY).await;
        }
        Err(e) => {
            error!("db rename category error: {e}");
            send_error(sender, errors::CATEGORY_RENAME_FAILED).await;
        }
    }
}

/// Handle delete category request.
pub(super) async fn handle_delete_category(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(id) = v.get("id").and_then(|i| i.as_i64()).map(|i| i as i32) else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }

    match db::remove_category(&state.db, id).await {
        Ok(true) => {
            broadcast_remove_category(state, id).await;
        }
        Ok(false) => {
            send_error(sender, errors::UNKNOWN_CATEGORY).await;
        }
        Err(e) => {
            error!("db delete category error: {e}");
            send_error(sender, errors::CATEGORY_DELETION_FAILED).await;
        }
    }
}
