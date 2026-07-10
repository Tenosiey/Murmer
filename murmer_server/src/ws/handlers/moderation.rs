//! Moderation handlers: kick, ban and mute.
//!
//! All actions require a privileged role (Owner, Admin or Mod) and the
//! requester must strictly outrank the target. Unlike channel management
//! there is no "no admin token" fallback: moderation is never open to
//! unprivileged users.

use crate::ws::{constants::*, errors, helpers::*};
use crate::{db, AppState};
use axum::extract::ws::{Message, WebSocket};
use chrono::{Duration as ChronoDuration, Utc};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// Validate that `requester` may run a moderation action against `target`.
/// Sends the appropriate error to the client and returns `false` on failure.
async fn check_allowed(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    requester: &str,
    target: &str,
) -> bool {
    if requester == target {
        send_error(sender, errors::CANNOT_MODERATE_SELF).await;
        return false;
    }

    let requester_rank = user_moderation_rank(state, requester).await;
    if requester_rank == 0 {
        send_error(sender, errors::MODERATION_PERMISSION_DENIED).await;
        return false;
    }

    let target_rank = user_moderation_rank(state, target).await;
    if target_rank >= requester_rank {
        send_error(sender, errors::MODERATION_TARGET_PROTECTED).await;
        return false;
    }

    true
}

/// Extract the requester name or reply with a permission error.
async fn require_requester(
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) -> Option<String> {
    match user_name.clone() {
        Some(name) => Some(name),
        None => {
            send_error(sender, errors::MODERATION_PERMISSION_DENIED).await;
            None
        }
    }
}

/// Extract the target user name or reply with a target error.
async fn require_target(sender: &mut SplitSink<WebSocket, Message>, v: &Value) -> Option<String> {
    match v.get("user").and_then(|u| u.as_str()) {
        Some(user) if !user.trim().is_empty() => Some(user.to_string()),
        _ => {
            send_error(sender, errors::MODERATION_TARGET_NOT_FOUND).await;
            None
        }
    }
}

/// Look up the public key of a target user, replying with an error if unknown.
async fn resolve_target_key(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    target: &str,
) -> Option<String> {
    let key = {
        let keys = state.user_keys.lock().await;
        keys.get(target).cloned()
    };
    if key.is_none() {
        send_error(sender, errors::MODERATION_TARGET_NOT_FOUND).await;
    }
    key
}

/// Broadcast a force-disconnect for `target`; the socket loop closes the
/// target's connection after forwarding the message.
fn broadcast_force_disconnect(state: &Arc<AppState>, target: &str, action: &str, by: &str) {
    let msg = serde_json::json!({
        "type": "force-disconnect",
        "user": target,
        "action": action,
        "by": by,
    });
    let _ = state.tx.send(msg.to_string());
}

/// Handle kick-user request: disconnect the target without persisting anything.
pub(super) async fn handle_kick_user(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = require_requester(sender, user_name).await else {
        return;
    };
    let Some(target) = require_target(sender, v).await else {
        return;
    };
    if !check_allowed(state, sender, &requester, &target).await {
        return;
    }

    let online = state.users.lock().await.contains(&target);
    if !online {
        send_error(sender, errors::MODERATION_TARGET_NOT_FOUND).await;
        return;
    }

    broadcast_force_disconnect(state, &target, "kicked", &requester);
    info!(requester, target, "User kicked");
}

/// Handle ban-user request: persist the ban and disconnect the target.
pub(super) async fn handle_ban_user(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = require_requester(sender, user_name).await else {
        return;
    };
    let Some(target) = require_target(sender, v).await else {
        return;
    };
    if !check_allowed(state, sender, &requester, &target).await {
        return;
    }
    let Some(key) = resolve_target_key(state, sender, &target).await else {
        return;
    };

    if let Err(e) = db::add_ban(&state.db, &key, &target, &requester).await {
        error!("Failed to persist ban for {target}: {e}");
        send_error(sender, errors::MODERATION_FAILED).await;
        return;
    }

    broadcast_force_disconnect(state, &target, "banned", &requester);
    info!(requester, target, "User banned");
}

/// Handle unban-user request: lift bans recorded for the target name.
pub(super) async fn handle_unban_user(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = require_requester(sender, user_name).await else {
        return;
    };
    let Some(target) = require_target(sender, v).await else {
        return;
    };
    if !check_allowed(state, sender, &requester, &target).await {
        return;
    }

    match db::remove_ban_by_name(&state.db, &target).await {
        Ok(true) => {
            let msg = serde_json::json!({
                "type": "user-unbanned",
                "user": target,
                "by": requester,
            });
            let _ = state.tx.send(msg.to_string());
            info!(requester, target, "User unbanned");
        }
        Ok(false) => {
            send_error(sender, errors::MODERATION_TARGET_NOT_FOUND).await;
        }
        Err(e) => {
            error!("Failed to lift ban for {target}: {e}");
            send_error(sender, errors::MODERATION_FAILED).await;
        }
    }
}

/// Handle mute-user request. An optional `durationSeconds` field limits the
/// mute; without it the mute lasts until explicitly lifted.
pub(super) async fn handle_mute_user(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = require_requester(sender, user_name).await else {
        return;
    };
    let Some(target) = require_target(sender, v).await else {
        return;
    };
    if !check_allowed(state, sender, &requester, &target).await {
        return;
    }
    let Some(key) = resolve_target_key(state, sender, &target).await else {
        return;
    };

    let until = v
        .get("durationSeconds")
        .and_then(|d| d.as_i64())
        .map(|seconds| {
            let clamped = seconds.clamp(MIN_MUTE_SECONDS, MAX_MUTE_SECONDS);
            Utc::now() + ChronoDuration::seconds(clamped)
        });

    if let Err(e) = db::add_mute(&state.db, &key, &target, &requester, until).await {
        error!("Failed to persist mute for {target}: {e}");
        send_error(sender, errors::MODERATION_FAILED).await;
        return;
    }

    state.mutes.lock().await.insert(key, until);

    let msg = serde_json::json!({
        "type": "user-muted",
        "user": target,
        "by": requester,
        "until": until.map(|value| value.to_rfc3339()),
    });
    let _ = state.tx.send(msg.to_string());
    info!(requester, target, ?until, "User muted");
}

/// Handle unmute-user request: lift mutes recorded for the target name.
pub(super) async fn handle_unmute_user(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = require_requester(sender, user_name).await else {
        return;
    };
    let Some(target) = require_target(sender, v).await else {
        return;
    };
    if !check_allowed(state, sender, &requester, &target).await {
        return;
    }

    match db::remove_mute_by_name(&state.db, &target).await {
        Ok(keys) if !keys.is_empty() => {
            {
                let mut mutes = state.mutes.lock().await;
                for key in &keys {
                    mutes.remove(key);
                }
            }
            let msg = serde_json::json!({
                "type": "user-unmuted",
                "user": target,
                "by": requester,
            });
            let _ = state.tx.send(msg.to_string());
            info!(requester, target, "User unmuted");
        }
        Ok(_) => {
            send_error(sender, errors::MODERATION_TARGET_NOT_FOUND).await;
        }
        Err(e) => {
            error!("Failed to lift mute for {target}: {e}");
            send_error(sender, errors::MODERATION_FAILED).await;
        }
    }
}

/// Check whether `user` is currently muted. Expired mutes are lazily removed
/// from both the in-memory map and the database.
pub(super) async fn is_muted(state: &Arc<AppState>, user: &str) -> bool {
    let key = {
        let keys = state.user_keys.lock().await;
        keys.get(user).cloned()
    };
    let Some(key) = key else {
        return false;
    };

    let entry = {
        let mutes = state.mutes.lock().await;
        match mutes.get(&key) {
            Some(entry) => *entry,
            None => return false,
        }
    };

    match entry {
        None => true,
        Some(until) if until > Utc::now() => true,
        Some(_) => {
            // The mute has expired: clean it up.
            state.mutes.lock().await.remove(&key);
            if let Err(e) = db::remove_mute_by_key(&state.db, &key).await {
                error!("Failed to remove expired mute for {user}: {e}");
            }
            false
        }
    }
}
