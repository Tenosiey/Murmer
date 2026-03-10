//! Handlers for text channel, voice channel and category management.

use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{db, security, AppState, VoiceChannelState};
use axum::extract::ws::{Message, WebSocket};
use futures::{stream::SplitSink, SinkExt};
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
        let _ = sender
            .send(Message::Text(errors::INVALID_CHANNEL_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("create-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to create channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let category_id = v
        .get("categoryId")
        .and_then(|c| c.as_i64())
        .map(|id| id as i32);

    match db::add_channel(&state.db, ch, category_id).await {
        Ok(Some(record)) => {
            get_or_create_channel(state, record.id).await;
            broadcast_new_channel(state, record.id, &record.name, record.category_id).await;
        }
        Ok(None) => {}
        Err(e) => {
            error!("db add channel error: {e}");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_CREATION_FAILED.to_string()))
                .await;
        }
    }
}

/// Handle delete channel request.
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
        let _ = sender
            .send(Message::Text(errors::CANNOT_DELETE_GENERAL.to_string()))
            .await;
        return Err(());
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("delete-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return Err(());
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to delete channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return Err(());
    }

    if let Err(e) = db::remove_channel(&state.db, ch_id).await {
        error!("db remove channel error: {e}");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_DELETION_FAILED.to_string()))
            .await;
    } else {
        state.channels.lock().await.remove(&ch_id);
        broadcast_remove_channel(state, ch_id).await;
        if *channel_id == ch_id {
            *channel_id = default_channel_id;
            *chan_tx = get_or_create_channel(state, *channel_id).await;
            *chan_rx = chan_tx.subscribe();
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
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let category_id = if v.get("categoryId").map_or(false, |c| c.is_null()) {
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
            let _ = sender
                .send(Message::Text(errors::CHANNEL_MOVE_FAILED.to_string()))
                .await;
        }
        Err(e) => {
            error!("db move channel error: {e}");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_MOVE_FAILED.to_string()))
                .await;
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
        let _ = sender
            .send(Message::Text(errors::INVALID_CHANNEL_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("create-voice-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to create voice channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
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
        let _ = sender
            .send(Message::Text(errors::INVALID_VOICE_QUALITY.to_string()))
            .await;
        return;
    }

    let bitrate_value = match v.get("bitrate") {
        Some(val) if val.is_null() => None,
        Some(val) => match val.as_i64().and_then(validate_bitrate) {
            Some(valid) => Some(valid),
            None => {
                let _ = sender
                    .send(Message::Text(errors::INVALID_VOICE_BITRATE.to_string()))
                    .await;
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
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to update voice channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let quality_override = if let Some(raw) = v.get("quality").and_then(|q| q.as_str()) {
        let trimmed = raw.trim();
        if !validate_voice_quality(trimmed) {
            let _ = sender
                .send(Message::Text(errors::INVALID_VOICE_QUALITY.to_string()))
                .await;
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
                    let _ = sender
                        .send(Message::Text(errors::INVALID_VOICE_BITRATE.to_string()))
                        .await;
                    return;
                }
            }
        }
    } else {
        None
    };

    let current = state.voice_channels.lock().await;
    let Some(existing) = current.get(&ch_id).cloned() else {
        let _ = sender
            .send(Message::Text(errors::UNKNOWN_VOICE_CHANNEL.to_string()))
            .await;
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
            let _ = sender
                .send(Message::Text(errors::UNKNOWN_VOICE_CHANNEL.to_string()))
                .await;
        }
        Err(e) => {
            error!("Failed to update voice channel {ch_id}: {e}");
            let _ = sender
                .send(Message::Text(
                    errors::VOICE_CHANNEL_UPDATE_FAILED.to_string(),
                ))
                .await;
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
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to delete voice channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
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
        let _ = sender
            .send(Message::Text(errors::INVALID_CATEGORY_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
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
            let _ = sender
                .send(Message::Text(errors::CATEGORY_CREATION_FAILED.to_string()))
                .await;
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
        let _ = sender
            .send(Message::Text(errors::INVALID_CATEGORY_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    match db::rename_category(&state.db, id, name).await {
        Ok(true) => {
            broadcast_rename_category(state, id, name).await;
        }
        Ok(false) => {
            let _ = sender
                .send(Message::Text(errors::UNKNOWN_CATEGORY.to_string()))
                .await;
        }
        Err(e) => {
            error!("db rename category error: {e}");
            let _ = sender
                .send(Message::Text(errors::CATEGORY_RENAME_FAILED.to_string()))
                .await;
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
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    match db::remove_category(&state.db, id).await {
        Ok(true) => {
            broadcast_remove_category(state, id).await;
        }
        Ok(false) => {
            let _ = sender
                .send(Message::Text(errors::UNKNOWN_CATEGORY.to_string()))
                .await;
        }
        Err(e) => {
            error!("db delete category error: {e}");
            let _ = sender
                .send(Message::Text(errors::CATEGORY_DELETION_FAILED.to_string()))
                .await;
        }
    }
}
