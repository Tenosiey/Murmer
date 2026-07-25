//! Handlers for the server's shared soundboard.
//!
//! Sound files travel through the regular `/upload` endpoint (the `audio`
//! category must be enabled in the server's upload policy); registration,
//! renaming and deletion happen here over the authenticated WebSocket where
//! the requester's permissions are known. Because `/upload` is open, the
//! server re-validates the referenced file — extension, size and magic bytes —
//! before registering it, so the open endpoint grants no extra power.
//!
//! Playback is *local on every listener*: `play-sound` is authorized here and
//! fanned out as a `soundboard-play` frame, and each client in the voice
//! channel fetches and plays the file itself. Nothing is mixed into anyone's
//! microphone stream, which is what makes per-listener volume and per-sound
//! mute possible at all.
//!
//! A soundboard is the most spammable surface in the app, so playback carries
//! a server-side per-user cooldown. The client shows the same cooldown, but
//! that display is cosmetic — this check is the one that counts.

use crate::channel_overrides::ChannelKind;
use crate::db::RenameOutcome;
use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tracing::{error, info, warn};

/// Resolve the requester, rejecting unauthenticated connections.
async fn requester<'a>(
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &'a Option<String>,
    error_json: &str,
) -> Option<&'a str> {
    match user_name.as_deref() {
        Some(name) => Some(name),
        None => {
            send_error(sender, error_json).await;
            None
        }
    }
}

/// Verify that `url` points at a real, small enough, genuinely-audio upload.
///
/// The extension safe-list alone is not enough here: these files are played
/// automatically on every listener's machine, so the head of the file must
/// also carry a known audio signature.
async fn validate_sound_upload(state: &Arc<AppState>, url: &str) -> bool {
    let Some(key) = sound_key_from_url(url) else {
        return false;
    };
    let path = state.upload_dir.join(key);

    match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_SOUND_FILE_BYTES && meta.len() > 0 => {}
        Ok(_) | Err(_) => return false,
    }

    let Ok(mut file) = tokio::fs::File::open(&path).await else {
        return false;
    };
    let mut head = [0u8; SOUND_MAGIC_BYTES];
    let Ok(read) = file.read(&mut head).await else {
        return false;
    };
    crate::upload::detect_audio_type(&head[..read]).is_some()
}

/// Handle `add-sound`: register a previously uploaded audio file.
pub(super) async fn handle_add_sound(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = requester(sender, user_name, errors::SOUND_PERMISSION_DENIED).await
    else {
        return;
    };

    if !has_permission(state, requester, crate::permissions::MANAGE_SOUNDS).await {
        warn!("User {requester} attempted to add a sound without permission");
        send_error(sender, errors::SOUND_PERMISSION_DENIED).await;
        return;
    }

    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(str::trim)
        .unwrap_or_default();
    if !validate_sound_name(name) {
        send_error(sender, errors::INVALID_SOUND_NAME).await;
        return;
    }

    let Some(url) = v.get("url").and_then(|u| u.as_str()) else {
        send_error(sender, errors::INVALID_SOUND_FILE).await;
        return;
    };
    if !validate_sound_upload(state, url).await {
        send_error(sender, errors::INVALID_SOUND_FILE).await;
        return;
    }

    match db::count_sounds(&state.db).await {
        Ok(count) if count >= MAX_SOUNDBOARD_SOUNDS => {
            send_error(sender, errors::SOUND_LIMIT_REACHED).await;
            return;
        }
        Ok(_) => {}
        Err(e) => {
            error!("db sound count error: {e}");
            send_error(sender, errors::SOUND_UPDATE_FAILED).await;
            return;
        }
    }

    match db::add_sound(&state.db, name, url, requester).await {
        Ok(Some(id)) => {
            info!(requester, name, id, "Soundboard sound added");
            broadcast_sounds(state).await;
        }
        Ok(None) => send_error(sender, errors::SOUND_NAME_TAKEN).await,
        Err(e) => {
            error!("db add sound error: {e}");
            send_error(sender, errors::SOUND_UPDATE_FAILED).await;
        }
    }
}

/// Handle `rename-sound`: change a sound's display name, keeping its id so
/// clients' per-sound volume and mute settings survive the rename.
pub(super) async fn handle_rename_sound(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = requester(sender, user_name, errors::SOUND_PERMISSION_DENIED).await
    else {
        return;
    };

    if !has_permission(state, requester, crate::permissions::MANAGE_SOUNDS).await {
        warn!("User {requester} attempted to rename a sound without permission");
        send_error(sender, errors::SOUND_PERMISSION_DENIED).await;
        return;
    }

    let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
        send_error(sender, errors::SOUND_NOT_FOUND).await;
        return;
    };

    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(str::trim)
        .unwrap_or_default();
    if !validate_sound_name(name) {
        send_error(sender, errors::INVALID_SOUND_NAME).await;
        return;
    }

    match db::rename_sound(&state.db, id, name).await {
        Ok(RenameOutcome::Renamed) => {
            info!(requester, id, name, "Soundboard sound renamed");
            broadcast_sounds(state).await;
        }
        Ok(RenameOutcome::NotFound) => send_error(sender, errors::SOUND_NOT_FOUND).await,
        Ok(RenameOutcome::NameTaken) => send_error(sender, errors::SOUND_NAME_TAKEN).await,
        Err(e) => {
            error!("db rename sound error: {e}");
            send_error(sender, errors::SOUND_UPDATE_FAILED).await;
        }
    }
}

/// Handle `remove-sound`: delete a sound and its file.
pub(super) async fn handle_remove_sound(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = requester(sender, user_name, errors::SOUND_PERMISSION_DENIED).await
    else {
        return;
    };

    if !has_permission(state, requester, crate::permissions::MANAGE_SOUNDS).await {
        warn!("User {requester} attempted to remove a sound without permission");
        send_error(sender, errors::SOUND_PERMISSION_DENIED).await;
        return;
    }

    let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
        send_error(sender, errors::SOUND_NOT_FOUND).await;
        return;
    };

    match db::remove_sound(&state.db, id).await {
        Ok(Some(url)) => {
            // Best-effort file cleanup; re-validate the stored URL before
            // touching the filesystem in case the DB row was tampered with.
            if let Some(key) = sound_key_from_url(&url) {
                let _ = tokio::fs::remove_file(state.upload_dir.join(key)).await;
            }
            info!(requester, id, "Soundboard sound removed");
            broadcast_sounds(state).await;
        }
        Ok(None) => send_error(sender, errors::SOUND_NOT_FOUND).await,
        Err(e) => {
            error!("db remove sound error: {e}");
            send_error(sender, errors::SOUND_UPDATE_FAILED).await;
        }
    }
}

/// Whether `user` may play a sound right now, recording the attempt when it
/// passes. Rejected attempts do *not* extend the window, so holding a key down
/// still lets a sound through once per cooldown instead of never.
async fn claim_cooldown(state: &Arc<AppState>, user: &str) -> bool {
    let cooldown = Duration::from_millis(SOUNDBOARD_COOLDOWN_MS);
    let now = Instant::now();
    let mut map = state.soundboard_cooldowns.lock().await;
    if let Some(last) = map.get(user)
        && now.duration_since(*last) < cooldown
    {
        return false;
    }
    map.insert(user.to_string(), now);
    true
}

/// Handle `play-sound`: authorize a playback request and fan it out to the
/// voice channel as a `soundboard-play` frame.
pub(super) async fn handle_play_sound(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    voice_channel: &Option<i32>,
    user_name: &Option<String>,
) {
    let Some(requester) = requester(sender, user_name, errors::SOUNDBOARD_PERMISSION_DENIED).await
    else {
        return;
    };

    let Some(channel_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return;
    };

    // Playing is scoped to the voice channel the connection is actually in;
    // the client cannot name someone else's channel.
    if *voice_channel != Some(channel_id) {
        send_error(sender, errors::SOUNDBOARD_PERMISSION_DENIED).await;
        return;
    }

    if !has_permission(state, requester, crate::permissions::USE_SOUNDBOARD).await {
        warn!("User {requester} attempted to play a sound without permission");
        send_error(sender, errors::SOUNDBOARD_PERMISSION_DENIED).await;
        return;
    }

    // A private voice channel must still gate playback even for a connection
    // that joined before its overrides changed.
    if !can_view_channel(state, requester, ChannelKind::Voice, channel_id).await {
        send_error(sender, errors::SOUNDBOARD_PERMISSION_DENIED).await;
        return;
    }

    // A server-muted member is silenced on every audio path, not just the mic.
    if super::moderation::is_muted(state, requester).await {
        send_error(sender, errors::SOUNDBOARD_PERMISSION_DENIED).await;
        return;
    }

    let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
        send_error(sender, errors::SOUND_NOT_FOUND).await;
        return;
    };
    let sound = match db::get_sound(&state.db, id).await {
        Ok(Some(sound)) => sound,
        Ok(None) => {
            send_error(sender, errors::SOUND_NOT_FOUND).await;
            return;
        }
        Err(e) => {
            error!("db get sound error: {e}");
            send_error(sender, errors::SOUND_UPDATE_FAILED).await;
            return;
        }
    };

    if !claim_cooldown(state, requester).await {
        send_error(sender, errors::SOUNDBOARD_COOLDOWN).await;
        return;
    }

    if let Err(e) = db::note_sound_played(&state.db, id).await {
        // An aggregate counter is not worth failing the playback over.
        error!("db sound play count error: {e}");
    }
    super::stats::note_sound_played(state, requester, &sound.name).await;

    let msg = serde_json::json!({
        "type": "soundboard-play",
        "id": sound.id,
        "name": sound.name,
        "url": sound.url,
        "user": requester,
        "channelId": channel_id,
    });
    let _ = state.tx.send(msg.to_string());
}

/// Drop a user's cooldown entry when they disconnect so the map stays bounded
/// by the number of connected users.
pub(super) async fn clear_cooldown(state: &Arc<AppState>, user: &str) {
    state.soundboard_cooldowns.lock().await.remove(user);
}
