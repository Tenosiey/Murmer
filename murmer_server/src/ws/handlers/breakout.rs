//! Breakout rooms: splitting one voice channel into several temporary ones
//! and folding them back together again.
//!
//! A room is an ordinary voice channel row carrying `breakout_parent`, which
//! is what makes it temporary: `close-breakouts` deletes the rooms, and any
//! that survive a crash are swept at startup (see [`crate::db::run_schema`]).
//! Opening and closing a split needs `MANAGE_CHANNELS` — the same permission
//! that creates and deletes the channels it is made of.
//!
//! The move itself is a *request*, not an eviction. Audio is peer-to-peer, so
//! the server cannot relocate a call: it addresses a `breakout-move` frame to
//! each member and the client tears down its peer connections and joins the
//! room the same way it would from a click. A client that ignores the frame
//! simply stays where it is, which is the same outcome as a member who was
//! never in the channel to begin with.

use crate::channel_overrides::ChannelKind;
use crate::security::validate_channel_name;
use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, VoiceChannelState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{error, info};

/// Resolve the requester and require `MANAGE_CHANNELS`.
async fn require_channel_manager(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) -> Option<String> {
    let Some(requester) = user_name.clone() else {
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return None;
    };
    if !has_permission(state, &requester, crate::permissions::MANAGE_CHANNELS).await {
        error!("User {requester} attempted to manage breakout rooms without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return None;
    }
    Some(requester)
}

/// The ids of `parent`'s currently open breakout rooms, in creation order.
async fn open_rooms(state: &Arc<AppState>, parent_id: i32) -> Vec<i32> {
    let map = state.voice_channels.lock().await;
    let mut ids: Vec<i32> = map
        .iter()
        .filter(|(_, info)| info.breakout_parent == Some(parent_id))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids
}

/// Give a new room the parent's permission overrides, so a private channel
/// does not become public the moment it is split. Written before the room is
/// announced: `voice-channel-add` is filtered per recipient against exactly
/// these rows, and a room announced first would be announced to everyone.
async fn inherit_overrides(state: &Arc<AppState>, parent_id: i32, room_id: i32) {
    let rows = match db::get_channel_overrides(&state.db, ChannelKind::Voice, parent_id).await {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to read overrides of breakout parent {parent_id}: {e}");
            return;
        }
    };
    if rows.is_empty() {
        return;
    }
    for row in &rows {
        if let Err(e) = db::upsert_channel_override(
            &state.db,
            ChannelKind::Voice,
            room_id,
            &row.target_type,
            &row.target_id,
            &row.target_label,
            row.allow,
            row.deny,
        )
        .await
        {
            error!("failed to copy override onto breakout room {room_id}: {e}");
        }
    }
    let mut map = state.channel_overrides.lock().await;
    if let Some(set) = map.get(&(ChannelKind::Voice, parent_id)).cloned() {
        map.insert((ChannelKind::Voice, room_id), set);
    }
}

/// Handle `open-breakouts`: split a voice channel into `rooms` temporary
/// rooms and ask everyone currently in it to move into one of them.
pub(super) async fn handle_open_breakouts(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(parent_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return;
    };
    let Some(count) = v
        .get("rooms")
        .and_then(|r| r.as_i64())
        .and_then(breakout_room_count)
    else {
        send_error(sender, errors::INVALID_BREAKOUT_ROOMS).await;
        return;
    };
    let Some(requester) = require_channel_manager(state, sender, user_name).await else {
        return;
    };

    let Some(parent) = state.voice_channels.lock().await.get(&parent_id).cloned() else {
        send_error(sender, errors::UNKNOWN_VOICE_CHANNEL).await;
        return;
    };
    // A room of a room would have no way back: closing the outer split
    // deletes the channel the inner one wants to return everybody to.
    if parent.breakout_parent.is_some() || !open_rooms(state, parent_id).await.is_empty() {
        send_error(sender, errors::BREAKOUT_ALREADY_OPEN).await;
        return;
    }

    // Room names collide with an existing channel rather than replacing it —
    // voice channel names are unique — so a taken name is skipped instead of
    // renumbering the rest, which would make "Room 3" mean something else on
    // the second split of the same channel.
    let mut created: Vec<(i32, VoiceChannelState)> = Vec::new();
    for index in 1..=count {
        let name = breakout_room_name(&parent.name, index);
        if !validate_channel_name(&name) {
            continue;
        }
        match db::add_voice_channel(
            &state.db,
            &name,
            &parent.quality,
            parent.bitrate,
            parent.category_id,
            Some(parent_id),
        )
        .await
        {
            Ok(Some(record)) => created.push((
                record.id,
                VoiceChannelState {
                    name: record.name,
                    users: HashSet::new(),
                    quality: record.quality,
                    bitrate: record.bitrate,
                    category_id: record.category_id,
                    position: record.position,
                    breakout_parent: record.breakout_parent,
                },
            )),
            Ok(None) => {}
            Err(e) => error!("db add breakout room error: {e}"),
        }
    }

    // Fewer than two rooms is not a split. Undo the half-made one rather than
    // leaving the channel with a single stray room nobody asked for; nothing
    // has been announced yet, so this is invisible.
    if created.len() < MIN_BREAKOUT_ROOMS {
        for (id, _) in &created {
            if let Err(e) = db::remove_voice_channel(&state.db, *id).await {
                error!("failed to roll back breakout room {id}: {e}");
            }
        }
        send_error(sender, errors::BREAKOUT_CREATE_FAILED).await;
        return;
    }

    for (id, info) in &created {
        inherit_overrides(state, parent_id, *id).await;
        state.voice_channels.lock().await.insert(*id, info.clone());
        broadcast_new_voice_channel(state, *id, info).await;
    }
    if channel_is_private(state, ChannelKind::Voice, parent_id).await {
        broadcast_channels_refresh(state).await;
    }

    // Deal the members out round-robin over a sorted roster, so the split is
    // the same on every server and nobody's room depends on hash ordering.
    let mut members: Vec<String> = parent.users.iter().cloned().collect();
    members.sort();
    for (index, member) in members.iter().enumerate() {
        let room_id = created[index % created.len()].0;
        if let Ok(frame) = serde_json::to_string(&serde_json::json!({
            "type": "breakout-move",
            "channelId": room_id,
            "parentId": parent_id,
        })) {
            send_to_user(state, member, frame.into()).await;
        }
    }

    info!(
        requester,
        parent_id,
        rooms = created.len(),
        members = members.len(),
        "Opened breakout rooms"
    );
}

/// Handle `close-breakouts`: send everyone back to the parent channel and
/// delete the rooms. Closing a split that is not open is a no-op, so a second
/// click (or two managers clicking at once) is harmless.
pub(super) async fn handle_close_breakouts(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(parent_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
    else {
        return;
    };
    let Some(requester) = require_channel_manager(state, sender, user_name).await else {
        return;
    };

    let closed = close_breakouts(state, parent_id, true).await;
    if closed > 0 {
        info!(
            requester,
            parent_id,
            rooms = closed,
            "Closed breakout rooms"
        );
    }
}

/// Delete every breakout room of `parent_id`, optionally asking the members
/// to return to the parent first. Returns how many rooms were closed.
///
/// `return_to_parent` is false when the parent itself is being deleted:
/// there is nowhere to send anybody, and asking them to join a channel that
/// is about to disappear would only produce a join the next frame undoes.
pub(super) async fn close_breakouts(
    state: &Arc<AppState>,
    parent_id: i32,
    return_to_parent: bool,
) -> usize {
    let rooms = open_rooms(state, parent_id).await;
    if rooms.is_empty() {
        return 0;
    }

    if return_to_parent {
        let members: Vec<String> = {
            let map = state.voice_channels.lock().await;
            rooms
                .iter()
                .filter_map(|id| map.get(id))
                .flat_map(|info| info.users.iter().cloned())
                .collect()
        };
        // Ask first, delete second: a member told to move after their room is
        // already gone would be leaving a channel that no longer exists.
        for member in &members {
            if let Ok(frame) = serde_json::to_string(&serde_json::json!({
                "type": "breakout-move",
                "channelId": parent_id,
                "parentId": parent_id,
            })) {
                send_to_user(state, member, frame.into()).await;
            }
        }
    }

    for room_id in &rooms {
        super::channel_overrides::cleanup_channel(state, ChannelKind::Voice, *room_id).await;
        state.voice_channels.lock().await.remove(room_id);
        if let Err(e) = db::remove_voice_channel(&state.db, *room_id).await {
            // The room is already gone from memory and from every client; log
            // it because the row would come back as a channel after a restart.
            error!("db remove breakout room error: {e}");
        }
        broadcast_remove_voice_channel(state, *room_id).await;
    }
    rooms.len()
}
