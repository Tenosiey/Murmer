//! Handlers for the key material of end-to-end encrypted channels.
//!
//! The server is a dumb store and a member directory here, nothing more. It
//! never sees a channel key: clients generate it, seal it per member with
//! `nacl.box` and hand the server opaque wraps, which it files under
//! `(channel, epoch, recipient)` and returns to whoever they are addressed to.
//!
//! What the server *does* decide is who counts as a member — the roster comes
//! from [`channel_members`], the same `can_view_channel` check that gates
//! reading the channel. That makes the server the key directory, exactly as it
//! already is for direct messages, and clients guard the same way: they pin
//! every member's identity key on first use and refuse to wrap the channel key
//! for a key that changed underneath them.
//!
//! Wraps are only ever accepted for accounts on that roster, so a member
//! cannot quietly hand the key to an outsider through this endpoint, and an
//! existing wrap is never overwritten (see [`db::insert_channel_keys`]).

use crate::channel_overrides::ChannelKind;
use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db, permissions};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::error;

/// Resolve the requester and the channel they name, requiring that they may
/// see it. Everything in this module is member-only: the roster and the epoch
/// structure of a private channel are themselves information.
async fn require_channel_view(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) -> Option<(String, i32)> {
    let Some(user) = user_name.clone() else {
        send_error(sender, errors::CHANNEL_KEY_PERMISSION_DENIED).await;
        return None;
    };
    let Some(channel_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|id| id as i32)
    else {
        send_error(sender, errors::INVALID_CHANNEL_KEY).await;
        return None;
    };
    if !can_view_channel(state, &user, ChannelKind::Text, channel_id).await {
        send_error(sender, errors::CHANNEL_KEY_PERMISSION_DENIED).await;
        return None;
    }
    Some((user, channel_id))
}

/// Send one member their view of a channel's key state: every wrap addressed
/// to them, the current epoch, who already holds it, and the roster to wrap
/// for. That is everything a client needs to decide between "read the
/// history", "hand the current key to a new member" and "rotate".
pub(super) async fn send_channel_keys(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
    channel_id: i32,
) {
    let Some(own_key) = lookup_user_key(state, user).await else {
        send_error(sender, errors::CHANNEL_KEY_PERMISSION_DENIED).await;
        return;
    };

    let keys = match db::get_channel_keys_for(&state.db, channel_id, &own_key).await {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load channel keys for {user} in {channel_id}: {e}");
            send_error(sender, errors::CHANNEL_KEY_FAILED).await;
            return;
        }
    };
    let epoch = match db::latest_channel_epoch(&state.db, channel_id).await {
        Ok(value) => value,
        Err(e) => {
            error!("failed to read key epoch of channel {channel_id}: {e}");
            send_error(sender, errors::CHANNEL_KEY_FAILED).await;
            return;
        }
    };
    let holders = match epoch {
        Some(epoch) => db::channel_epoch_recipients(&state.db, channel_id, epoch)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };

    let members: Vec<Value> = channel_members(state, channel_id)
        .await
        .into_iter()
        .map(|(name, key)| serde_json::json!({ "user": name, "publicKey": key }))
        .collect();
    let entries: Vec<Value> = keys
        .iter()
        .map(|k| {
            serde_json::json!({
                "epoch": k.epoch,
                "senderKey": k.sender_key,
                "nonce": k.nonce,
                "wrappedKey": k.wrapped_key,
            })
        })
        .collect();

    let payload = serde_json::json!({
        "type": "channel-keys",
        "channelId": channel_id,
        "epoch": epoch,
        "keys": entries,
        "holders": holders,
        "members": members,
    });
    let _ = sender.send(Message::Text(payload.to_string().into())).await;
}

/// Handle `get-channel-keys`.
pub(super) async fn handle_get_channel_keys(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some((user, channel_id)) = require_channel_view(state, sender, v, user_name).await else {
        return;
    };
    send_channel_keys(state, sender, &user, channel_id).await;
}

/// Handle `put-channel-keys`: store wraps of one epoch's channel key.
///
/// The frame carries only sealed blobs, so validation is structural: the
/// epoch is in range, every wrap is the right size, and every recipient is on
/// the channel's current member roster. The last of those is the one that
/// matters — it keeps this endpoint from being a way to hand a private
/// channel's key to somebody the channel's permissions exclude.
pub(super) async fn handle_put_channel_keys(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some((user, channel_id)) = require_channel_view(state, sender, v, user_name).await else {
        return;
    };
    let Some(author_key) = lookup_user_key(state, &user).await else {
        send_error(sender, errors::CHANNEL_KEY_PERMISSION_DENIED).await;
        return;
    };

    // Only encrypted channels hold key material, so a client that is out of
    // sync with the flag is told to re-read rather than allowed to seed keys
    // for a plaintext channel.
    if !channel_is_e2ee(state, channel_id).await {
        send_error(sender, errors::CHANNEL_NOT_ENCRYPTED).await;
        return;
    }

    let Some(epoch) = v.get("epoch").and_then(|e| e.as_i64()) else {
        send_error(sender, errors::INVALID_CHANNEL_KEY).await;
        return;
    };
    if !(1..=MAX_CHANNEL_KEY_EPOCH).contains(&epoch) {
        send_error(sender, errors::INVALID_CHANNEL_KEY).await;
        return;
    }

    let Some(raw_entries) = v.get("entries").and_then(|e| e.as_array()) else {
        send_error(sender, errors::INVALID_CHANNEL_KEY).await;
        return;
    };
    if raw_entries.is_empty() || raw_entries.len() > MAX_CHANNEL_KEY_ENTRIES {
        send_error(sender, errors::INVALID_CHANNEL_KEY).await;
        return;
    }

    let roster: HashSet<String> = channel_members(state, channel_id)
        .await
        .into_iter()
        .map(|(_, key)| key)
        .collect();

    let mut entries = Vec::with_capacity(raw_entries.len());
    for raw in raw_entries {
        let (Some(recipient), Some(nonce), Some(wrapped)) = (
            raw.get("recipientKey").and_then(|r| r.as_str()),
            raw.get("nonce").and_then(|n| n.as_str()),
            raw.get("wrappedKey").and_then(|w| w.as_str()),
        ) else {
            send_error(sender, errors::INVALID_CHANNEL_KEY).await;
            return;
        };
        // A wrapped key is a fixed-size blob (32-byte secret + authenticator);
        // anything else is not one, whatever the client believes it is sending.
        if validate_sealed_payload(
            nonce,
            wrapped,
            WRAPPED_CHANNEL_KEY_BYTES - BOX_OVERHEAD_BYTES,
        )
        .is_err()
        {
            send_error(sender, errors::INVALID_CHANNEL_KEY).await;
            return;
        }
        if !roster.contains(recipient) {
            send_error(sender, errors::CHANNEL_KEY_TARGET_NOT_MEMBER).await;
            return;
        }
        entries.push(db::ChannelKeyEntry {
            recipient_key: recipient.to_string(),
            nonce: nonce.to_string(),
            wrapped_key: wrapped.to_string(),
        });
    }

    match db::insert_channel_keys(&state.db, channel_id, epoch, &author_key, entries).await {
        Ok(db::ChannelKeyWrite::Stored(_)) => {}
        Ok(db::ChannelKeyWrite::EpochConflict) => {
            // Somebody else rotated first. Answering with the current state is
            // what lets the loser of the race redo its decision instead of
            // retrying a write that can never succeed.
            send_error(sender, errors::CHANNEL_KEY_EPOCH_CONFLICT).await;
            send_channel_keys(state, sender, &user, channel_id).await;
            return;
        }
        Ok(db::ChannelKeyWrite::NotAKeyHolder) => {
            send_error(sender, errors::CHANNEL_KEY_PERMISSION_DENIED).await;
            return;
        }
        Err(e) => {
            error!("failed to store channel keys for {channel_id}: {e}");
            send_error(sender, errors::CHANNEL_KEY_FAILED).await;
            return;
        }
    }

    send_channel_keys(state, sender, &user, channel_id).await;
    notify_key_holders(state, channel_id, &user).await;
}

/// Tell every member except the author that a channel's key material moved, so
/// they re-fetch it. Only the signal travels — the wraps themselves are
/// per-recipient and are fetched over each member's own connection.
async fn notify_key_holders(state: &Arc<AppState>, channel_id: i32, except: &str) {
    let frame: crate::Frame = serde_json::json!({
        "type": "channel-keys-changed",
        "channelId": channel_id,
    })
    .to_string()
    .into();
    for (member, _) in channel_members(state, channel_id).await {
        if member == except {
            continue;
        }
        send_to_user(state, &member, frame.clone()).await;
    }
}

/// Handle `set-channel-e2ee`: turn encryption on or off for a text channel.
///
/// Only private channels can be encrypted. That is not a cryptographic
/// requirement — it is the honest one: a channel `@everyone` can read has
/// every account on its key roster, so encrypting it would buy nothing but the
/// impression of privacy.
pub(super) async fn handle_set_channel_e2ee(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.clone() else {
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    };
    if !has_permission(state, &requester, permissions::MANAGE_CHANNELS).await {
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return;
    }
    let Some(channel_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|id| id as i32)
    else {
        send_error(sender, errors::INVALID_CHANNEL_KEY).await;
        return;
    };
    let enabled = v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false);

    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        send_error(sender, errors::UNKNOWN_CHANNEL).await;
        return;
    }
    if enabled && !channel_is_private(state, ChannelKind::Text, channel_id).await {
        send_error(sender, errors::CHANNEL_NOT_PRIVATE).await;
        return;
    }

    match db::set_channel_e2ee(&state.db, channel_id, enabled).await {
        Ok(true) => {}
        Ok(false) => {
            send_error(sender, errors::UNKNOWN_CHANNEL).await;
            return;
        }
        Err(e) => {
            error!("failed to set e2ee on channel {channel_id}: {e}");
            send_error(sender, errors::CHANNEL_KEY_FAILED).await;
            return;
        }
    }

    if !enabled && let Err(e) = db::delete_channel_keys(&state.db, channel_id).await {
        error!("failed to drop keys of channel {channel_id}: {e}");
    }

    // Everyone rebuilds their channel list from this, which is also how
    // members learn to start fetching (or stop expecting) key material.
    broadcast_channels_refresh(state).await;
}
