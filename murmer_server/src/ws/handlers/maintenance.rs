//! Danger Zone handlers: purging every message and resetting the server.
//!
//! Both require `ADMINISTRATOR` (never the no-`ADMIN_TOKEN` fallback that
//! keeps channel management open) *and* a typed confirmation phrase echoed
//! back in the frame, so a stray click or a replayed frame cannot wipe a
//! server. The database work runs in one transaction in `db::maintenance`;
//! what is left here is refreshing the in-memory caches that mirror the rows
//! just deleted and telling every connected client to rebuild its view.

use crate::ws::{errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Phrase the client must echo in `confirm` to purge all messages.
const PURGE_CONFIRMATION: &str = "PURGE";

/// Phrase the client must echo in `confirm` to reset the server.
const RESET_CONFIRMATION: &str = "RESET";

/// Check that the requester is an administrator and typed the confirmation
/// phrase. Returns their name on success.
async fn require_confirmed_admin(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
    phrase: &str,
) -> Option<String> {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::MAINTENANCE_PERMISSION_DENIED).await;
        return None;
    };

    // Server-side permission check; clients cannot spoof this.
    if !has_permission(state, requester, crate::permissions::ADMINISTRATOR).await {
        warn!("User {requester} attempted a Danger Zone action without permission");
        send_error(sender, errors::MAINTENANCE_PERMISSION_DENIED).await;
        return None;
    }

    if v.get("confirm").and_then(|c| c.as_str()) != Some(phrase) {
        send_error(sender, errors::MAINTENANCE_NOT_CONFIRMED).await;
        return None;
    }

    Some(requester.to_string())
}

/// Handle `purge-all-messages`: delete every message, pin and reaction on the
/// server and tell clients to drop what they are holding.
pub(super) async fn handle_purge_all_messages(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) =
        require_confirmed_admin(state, sender, v, user_name, PURGE_CONFIRMATION).await
    else {
        return;
    };

    let purged = match db::purge_all_messages(&state.db).await {
        Ok(count) => count,
        Err(e) => {
            error!("Failed to purge messages: {e}");
            send_error(sender, errors::MAINTENANCE_FAILED).await;
            return;
        }
    };

    info!(requester, purged, "All messages purged");
    broadcast_messages_purged(state, &requester, purged);
}

/// Tell every client that the message history is gone. Clients clear their
/// channel view, pins and threads and reload (empty) history.
fn broadcast_messages_purged(state: &Arc<AppState>, requester: &str, purged: usize) {
    let msg = serde_json::json!({
        "type": "messages-purged",
        "by": requester,
        "count": purged,
    });
    let _ = state.tx.send(msg.to_string().into());
}

/// Handle `reset-server`: wipe the server's structure (channels, categories,
/// custom roles, overrides, wiki and messages) and rebuild the in-memory
/// state that mirrored it.
pub(super) async fn handle_reset_server(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) =
        require_confirmed_admin(state, sender, v, user_name, RESET_CONFIRMATION).await
    else {
        return;
    };

    let summary = match db::reset_server(&state.db).await {
        Ok(summary) => summary,
        Err(e) => {
            error!("Failed to reset server: {e}");
            send_error(sender, errors::MAINTENANCE_FAILED).await;
            return;
        }
    };

    // Every cache below mirrors rows the transaction just deleted; leaving
    // any of them populated would keep a deleted channel joinable or a
    // deleted role granting permissions until the next restart.
    state.voice_channels.lock().await.clear();
    state.channel_overrides.lock().await.clear();

    let role_defs = db::list_role_defs(&state.db).await.unwrap_or_default();
    let surviving: Vec<i64> = role_defs.iter().map(|def| def.id).collect();
    {
        let mut defs = state.role_defs.lock().await;
        *defs = role_defs.into_iter().map(|def| (def.id, def)).collect();
    }
    // Assignments to deleted roles went with them through the `user_roles`
    // cascade; drop them from the live map too and re-announce what is left.
    let assignments: Vec<(String, Vec<i64>)> = {
        let mut user_roles = state.user_roles.lock().await;
        for ids in user_roles.values_mut() {
            ids.retain(|id| surviving.contains(id));
        }
        user_roles
            .iter()
            .map(|(user, ids)| (user.clone(), ids.clone()))
            .collect()
    };

    info!(
        requester,
        messages = summary.messages,
        channels = summary.channels,
        voice_channels = summary.voice_channels,
        categories = summary.categories,
        roles = summary.roles,
        "Server reset"
    );

    broadcast_role_definitions(state).await;
    for (user, ids) in assignments {
        broadcast_user_roles(state, &user, &ids).await;
    }
    broadcast_messages_purged(state, &requester, summary.messages);
    let msg = serde_json::json!({
        "type": "server-reset",
        "by": requester,
    });
    let _ = state.tx.send(msg.to_string().into());
    // Rebuilds every connection's channel and voice lists from the database.
    // A member who was sitting in a voice channel now holds an id that is
    // gone from `voice_channels`, so their next join is refused and the
    // refreshed list drops the channel from their sidebar.
    broadcast_channels_refresh(state).await;
}
