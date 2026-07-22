//! Pinned message handlers: shared, persisted pins per channel.
//!
//! Any authenticated user can pin or unpin. Every change broadcasts a full
//! `pins` snapshot for the channel, and clients receive the same snapshot when
//! joining a channel, so all clients converge on the persisted state.

use crate::channel_overrides::ChannelKind;
use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// Build the `pins` snapshot payload for a channel.
async fn pins_payload(state: &Arc<AppState>, channel_id: i32) -> Option<String> {
    match db::get_pins_for_channel(&state.db, channel_id).await {
        Ok(pins) => Some(
            serde_json::json!({
                "type": "pins",
                "channelId": channel_id,
                "pins": pins,
            })
            .to_string(),
        ),
        Err(e) => {
            error!("Failed to load pins for channel {channel_id}: {e}");
            None
        }
    }
}

/// Send the current pin list for a channel to a single client.
pub(super) async fn send_pins(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    channel_id: i32,
) {
    if let Some(payload) = pins_payload(state, channel_id).await {
        let _ = sender.send(Message::Text(payload.into())).await;
    }
}

/// Broadcast the current pin list to every client joined to the channel.
async fn broadcast_pins(state: &Arc<AppState>, channel_id: i32) {
    if let Some(payload) = pins_payload(state, channel_id).await {
        let chan_tx = get_or_create_channel(state, channel_id).await;
        let _ = chan_tx.send(payload);
    }
}

/// Extract the target message id or reply with a pin error.
async fn require_message_id(sender: &mut SplitSink<WebSocket, Message>, v: &Value) -> Option<i64> {
    let id = v.get("messageId").and_then(|m| m.as_i64());
    if id.is_none() {
        send_error(sender, errors::PIN_TARGET_NOT_FOUND).await;
    }
    id
}

/// Handle pin-message request: persist the pin and broadcast the new list.
pub(super) async fn handle_pin_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        return;
    };
    let Some(message_id) = require_message_id(sender, v).await else {
        return;
    };

    match db::get_message_record(&state.db, message_id).await {
        Ok(Some(record)) => {
            if !can_view_channel(state, &user, ChannelKind::Text, record.channel_id).await {
                send_error(sender, errors::PIN_TARGET_NOT_FOUND).await;
                return;
            }
            match db::add_pin(
                &state.db,
                message_id,
                record.channel_id,
                &user,
                MAX_PINS_PER_CHANNEL,
            )
            .await
            {
                Ok(true) => {
                    broadcast_pins(state, record.channel_id).await;
                    info!(user, message_id, "Message pinned");
                    super::stats::record(state, &user, vec![(db::Stat::PinsAdded, 1)]).await;
                }
                Ok(false) => {
                    send_error(sender, errors::PIN_LIMIT_REACHED).await;
                }
                Err(e) => {
                    error!("Failed to pin message {message_id}: {e}");
                    send_error(sender, errors::PIN_FAILED).await;
                }
            }
        }
        Ok(None) => {
            send_error(sender, errors::PIN_TARGET_NOT_FOUND).await;
        }
        Err(e) => {
            error!("Failed to load message {message_id} for pinning: {e}");
            send_error(sender, errors::PIN_FAILED).await;
        }
    }
}

/// Handle unpin-message request: drop the pin and broadcast the new list.
pub(super) async fn handle_unpin_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        return;
    };
    let Some(message_id) = require_message_id(sender, v).await else {
        return;
    };

    // Only act on a pin in a channel the user can see.
    if let Ok(Some(record)) = db::get_message_record(&state.db, message_id).await
        && !can_view_channel(state, &user, ChannelKind::Text, record.channel_id).await
    {
        send_error(sender, errors::PIN_TARGET_NOT_FOUND).await;
        return;
    }

    match db::remove_pin(&state.db, message_id).await {
        Ok(Some(channel_id)) => {
            broadcast_pins(state, channel_id).await;
            info!(user, message_id, "Message unpinned");
        }
        // Already unpinned (e.g. a concurrent unpin); nothing to broadcast.
        Ok(None) => {}
        Err(e) => {
            error!("Failed to unpin message {message_id}: {e}");
            send_error(sender, errors::PIN_FAILED).await;
        }
    }
}
