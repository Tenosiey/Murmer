//! Read markers: where each account stopped reading, per channel and per
//! direct conversation, shared by every client it is signed in from.
//!
//! `mark-read` advances one marker. A marker that moved is echoed as
//! `read-marker` to every connection of the same account, so a second client
//! clears its badge as soon as the first one reads; it goes direct, since it
//! concerns nobody else. `read-markers` is the whole set, sent at sign-in
//! ahead of the channel list so the client has it before it opens a channel.
//!
//! Anyone authenticated may mark, and only for themselves: the frame names a
//! channel or a peer, never a user. The channel must be one they can see and
//! the peer one the server knows, which also bounds how many rows one
//! account can make the server keep.

use crate::channel_overrides::ChannelKind;
use crate::ws::{helpers::*, validation::i32_field};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use tracing::error;

/// Handle `mark-read`: `{channelId, messageId}` or `{with, messageId}`.
pub(super) async fn handle_mark_read(state: &Arc<AppState>, v: &Value, user_name: &Option<String>) {
    let Some(user) = user_name.as_deref() else {
        return;
    };
    let Some(message_id) = v
        .get("messageId")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
    else {
        return;
    };

    let (moved, echo) = if let Some(channel_id) = i32_field(v, "channelId") {
        if !can_view_channel(state, user, ChannelKind::Text, channel_id).await {
            return;
        }
        (
            db::mark_channel_read(&state.db, user, channel_id, message_id).await,
            json!({ "type": "read-marker", "channelId": channel_id, "messageId": message_id }),
        )
    } else if let Some(peer) = v.get("with").and_then(Value::as_str) {
        if peer == user || !state.known_users.lock().await.contains(peer) {
            return;
        }
        (
            db::mark_dm_read(&state.db, user, peer, message_id).await,
            json!({ "type": "read-marker", "with": peer, "messageId": message_id }),
        )
    } else {
        return;
    };

    match moved {
        Ok(true) => {
            send_to_user(state, user, echo.to_string().into()).await;
        }
        Ok(false) => {}
        Err(e) => error!("Failed to store read marker for {user}: {e}"),
    }
}

/// Send `user` every marker they hold, plus how many direct messages wait
/// past each DM marker. The channel side carries no counts: the client works
/// those out from what arrives, and only it can spot a mention in an
/// encrypted channel.
pub(super) async fn send_read_markers(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
) {
    let channels = match db::channel_read_markers(&state.db, user).await {
        Ok(markers) => markers,
        Err(e) => {
            error!("Failed to load channel read markers for {user}: {e}");
            return;
        }
    };
    let dms = match db::dm_read_state(&state.db, user).await {
        Ok(markers) => markers,
        Err(e) => {
            error!("Failed to load DM read markers for {user}: {e}");
            return;
        }
    };
    let channels: Map<String, Value> = channels
        .into_iter()
        .map(|(id, message_id)| (id.to_string(), Value::from(message_id)))
        .collect();
    let dms: Map<String, Value> = dms
        .into_iter()
        .map(|(peer, (last_read, unread))| {
            (peer, json!({ "lastRead": last_read, "unread": unread }))
        })
        .collect();
    send_json(
        sender,
        &json!({ "type": "read-markers", "channels": channels, "dms": dms }),
    )
    .await;
}
