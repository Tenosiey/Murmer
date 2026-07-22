//! Handlers for per-channel permission overrides — the mechanism behind
//! private channels and listen-only members.
//!
//! All actions require the server-wide `MANAGE_CHANNELS` permission. Override
//! masks are clamped to [`CHANNEL_OVERRIDABLE`](crate::permissions::CHANNEL_OVERRIDABLE)
//! (View + Write/Talk) so channel settings can never grant management or
//! moderation powers. Every change persists, updates the in-memory cache and
//! signals clients to re-derive their visible channel lists.

use crate::channel_overrides::{ChannelKind, OverridePair};
use crate::permissions::{self, Permissions};
use crate::ws::{errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

/// Resolve the requester and require `MANAGE_CHANNELS`.
async fn require_channel_manager(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) -> Option<String> {
    let Some(requester) = user_name.clone() else {
        send_error(sender, errors::CHANNEL_OVERRIDE_PERMISSION_DENIED).await;
        return None;
    };
    if !has_permission(state, &requester, permissions::MANAGE_CHANNELS).await {
        send_error(sender, errors::CHANNEL_OVERRIDE_PERMISSION_DENIED).await;
        return None;
    }
    Some(requester)
}

/// Read `channelId` and the `voice` flag into an id + kind.
fn channel_ref(v: &Value) -> Option<(ChannelKind, i32)> {
    let id = v.get("channelId").and_then(|c| c.as_i64())? as i32;
    let kind = ChannelKind::from_voice(v.get("voice").and_then(|b| b.as_bool()).unwrap_or(false));
    Some((kind, id))
}

/// Write one override to the database and the in-memory cache. `everyone`
/// targets ignore `target_id`.
#[allow(clippy::too_many_arguments)]
async fn store_override(
    state: &Arc<AppState>,
    kind: ChannelKind,
    channel_id: i32,
    target_type: &str,
    target_id: &str,
    target_label: &str,
    allow: Permissions,
    deny: Permissions,
) -> bool {
    if let Err(e) = db::upsert_channel_override(
        &state.db,
        kind,
        channel_id,
        target_type,
        target_id,
        target_label,
        allow,
        deny,
    )
    .await
    {
        error!("failed to persist channel override: {e}");
        return false;
    }
    let mut map = state.channel_overrides.lock().await;
    let set = map.entry((kind, channel_id)).or_default();
    let pair = OverridePair { allow, deny };
    match target_type {
        "everyone" => set.everyone = pair,
        "role" => {
            if let Ok(id) = target_id.parse::<i64>() {
                set.roles.insert(id, pair);
            }
        }
        "user" => {
            set.users.insert(target_id.to_string(), pair);
        }
        _ => {}
    }
    true
}

/// Seed a freshly created channel as private: deny `@everyone` View and grant
/// the creator View + Write/Talk so they keep access. Called from the channel
/// create handlers when `private: true`.
pub(super) async fn make_channel_private(
    state: &Arc<AppState>,
    kind: ChannelKind,
    channel_id: i32,
    creator: &str,
) {
    store_override(
        state,
        kind,
        channel_id,
        "everyone",
        "",
        "",
        0,
        permissions::VIEW_CHANNELS,
    )
    .await;
    if let Some(key) = lookup_user_key(state, creator).await {
        store_override(
            state,
            kind,
            channel_id,
            "user",
            &key,
            creator,
            permissions::CHANNEL_OVERRIDABLE,
            0,
        )
        .await;
    }
}

/// Handle `set-channel-override`: create or update one target's override.
pub(super) async fn handle_set_channel_override(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(_requester) = require_channel_manager(state, sender, user_name).await else {
        return;
    };
    let Some((kind, channel_id)) = channel_ref(v) else {
        send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
        return;
    };

    let allow = v.get("allow").and_then(|a| a.as_u64()).unwrap_or(0) as Permissions
        & permissions::CHANNEL_OVERRIDABLE;
    let deny = v.get("deny").and_then(|d| d.as_u64()).unwrap_or(0) as Permissions
        & permissions::CHANNEL_OVERRIDABLE;
    // A permission cannot be both allowed and denied.
    if allow & deny != 0 {
        send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
        return;
    }

    let target = v.get("target");
    let target_type = target
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let (target_id, label) = match target_type {
        "everyone" => (String::new(), String::new()),
        "role" => {
            let Some(role_id) = target.and_then(|t| t.get("id")).and_then(|i| i.as_i64()) else {
                send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
                return;
            };
            match db::get_role_def(&state.db, role_id).await {
                Ok(Some(def)) => (role_id.to_string(), def.name),
                _ => {
                    send_error(sender, errors::OVERRIDE_TARGET_NOT_FOUND).await;
                    return;
                }
            }
        }
        "user" => {
            let Some(user) = target.and_then(|t| t.get("user")).and_then(|u| u.as_str()) else {
                send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
                return;
            };
            let Some(key) = lookup_user_key(state, user).await else {
                send_error(sender, errors::OVERRIDE_TARGET_NOT_FOUND).await;
                return;
            };
            (key, user.to_string())
        }
        _ => {
            send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
            return;
        }
    };

    // An empty override is a removal.
    if allow == 0 && deny == 0 {
        remove_and_notify(state, sender, kind, channel_id, target_type, &target_id).await;
        return;
    }

    if !store_override(
        state,
        kind,
        channel_id,
        target_type,
        &target_id,
        &label,
        allow,
        deny,
    )
    .await
    {
        send_error(sender, errors::CHANNEL_OVERRIDE_FAILED).await;
        return;
    }

    broadcast_channels_refresh(state).await;
    send_channel_overrides(state, sender, kind, channel_id).await;
}

/// Handle `remove-channel-override`.
pub(super) async fn handle_remove_channel_override(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(_requester) = require_channel_manager(state, sender, user_name).await else {
        return;
    };
    let Some((kind, channel_id)) = channel_ref(v) else {
        send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
        return;
    };
    let target = v.get("target");
    let target_type = target
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let target_id = match target_type {
        "everyone" => String::new(),
        "role" => match target.and_then(|t| t.get("id")).and_then(|i| i.as_i64()) {
            Some(id) => id.to_string(),
            None => {
                send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
                return;
            }
        },
        "user" => {
            let Some(user) = target.and_then(|t| t.get("user")).and_then(|u| u.as_str()) else {
                send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
                return;
            };
            match lookup_user_key(state, user).await {
                Some(key) => key,
                None => {
                    send_error(sender, errors::OVERRIDE_TARGET_NOT_FOUND).await;
                    return;
                }
            }
        }
        _ => {
            send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
            return;
        }
    };
    remove_and_notify(state, sender, kind, channel_id, target_type, &target_id).await;
}

/// Delete an override from the database and cache, then refresh clients and
/// reply with the updated list.
async fn remove_and_notify(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    kind: ChannelKind,
    channel_id: i32,
    target_type: &str,
    target_id: &str,
) {
    if let Err(e) =
        db::delete_channel_override(&state.db, kind, channel_id, target_type, target_id).await
    {
        error!("failed to delete channel override: {e}");
        send_error(sender, errors::CHANNEL_OVERRIDE_FAILED).await;
        return;
    }
    {
        let mut map = state.channel_overrides.lock().await;
        if let Some(set) = map.get_mut(&(kind, channel_id)) {
            match target_type {
                "everyone" => set.everyone = OverridePair::default(),
                "role" => {
                    if let Ok(id) = target_id.parse::<i64>() {
                        set.roles.remove(&id);
                    }
                }
                "user" => {
                    set.users.remove(target_id);
                }
                _ => {}
            }
            if set.is_empty() {
                map.remove(&(kind, channel_id));
            }
        }
    }
    broadcast_channels_refresh(state).await;
    send_channel_overrides(state, sender, kind, channel_id).await;
}

/// Handle `get-channel-overrides`: send the current overrides for one channel
/// to the requesting manager.
pub(super) async fn handle_get_channel_overrides(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(_requester) = require_channel_manager(state, sender, user_name).await else {
        return;
    };
    let Some((kind, channel_id)) = channel_ref(v) else {
        send_error(sender, errors::INVALID_CHANNEL_OVERRIDE).await;
        return;
    };
    send_channel_overrides(state, sender, kind, channel_id).await;
}

/// Remove all overrides for a deleted channel (DB + cache).
pub(super) async fn cleanup_channel(state: &Arc<AppState>, kind: ChannelKind, channel_id: i32) {
    if let Err(e) = db::delete_overrides_for_channel(&state.db, kind, channel_id).await {
        error!("failed to clean up overrides for deleted channel: {e}");
    }
    state
        .channel_overrides
        .lock()
        .await
        .remove(&(kind, channel_id));
}
