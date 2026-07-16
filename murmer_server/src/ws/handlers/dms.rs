//! Direct message handlers: private chat between two users.
//!
//! DM frames travel over the global broadcast channel like presence updates,
//! but the socket loop only forwards a frame to its two participants (see
//! [`crate::ws::helpers::dm_involves`]), so other clients never receive the
//! content over the wire.

use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db, security};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

/// Handle an outgoing direct message: validate, persist and broadcast it to
/// the sender and recipient.
pub(super) async fn handle_dm(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(from) = user_name.clone() else {
        return;
    };

    if !security::check_message_rate_limit(&state.rate_limiter, &from).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return;
    }

    if super::moderation::is_muted(state, &from).await {
        send_error(sender, errors::MUTED).await;
        return;
    }

    let to = match v.get("to").and_then(|t| t.as_str()).map(str::trim) {
        Some(to) if !to.is_empty() => to.to_string(),
        _ => {
            send_error(sender, errors::DM_TARGET_NOT_FOUND).await;
            return;
        }
    };
    if to == from {
        send_error(sender, errors::CANNOT_DM_SELF).await;
        return;
    }
    if !state.known_users.lock().await.contains(&to) {
        send_error(sender, errors::DM_TARGET_NOT_FOUND).await;
        return;
    }

    let Some(text) = v.get("text").and_then(|t| t.as_str()) else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    if text.len() > MAX_MESSAGE_LENGTH {
        send_error(sender, errors::MESSAGE_TOO_LONG).await;
        return;
    }

    let mut out = serde_json::json!({
        "type": "dm",
        "from": from,
        "to": to,
        "text": text,
    });
    if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
        out["timestamp"] = Value::String(ts.to_string());
    }
    if let Some(time) = v.get("time").and_then(|t| t.as_str()) {
        out["time"] = Value::String(time.to_string());
    }
    let timestamp = sanitize_message_timestamp(&mut out);
    ensure_time(&mut out, &timestamp);

    let content = out.to_string();
    match db::insert_direct_message(&state.db, &from, &to, &content).await {
        Ok(id) => {
            out["id"] = Value::from(id);
            // Delivered via the global broadcast; the socket loop filters the
            // frame so only the two participants receive it.
            let _ = state.tx.send(out.to_string());

            // Only the count is recorded — DM content and recipients stay
            // out of the stats tables entirely.
            super::stats::record(state, &from, vec![(crate::db::Stat::DmsSent, 1)]).await;
        }
        Err(e) => {
            error!("Failed to persist direct message from {from} to {to}: {e}");
            send_error(sender, errors::DM_SEND_FAILED).await;
        }
    }
}

/// Handle a request for the conversation history with another user.
pub(super) async fn handle_load_dm_history(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        return;
    };
    let peer = match v.get("with").and_then(|w| w.as_str()).map(str::trim) {
        Some(peer) if !peer.is_empty() => peer.to_string(),
        _ => {
            send_error(sender, errors::DM_TARGET_NOT_FOUND).await;
            return;
        }
    };

    let before = v.get("before").and_then(|b| b.as_i64());
    let limit = v
        .get("limit")
        .and_then(|l| l.as_i64())
        .unwrap_or(DEFAULT_HISTORY_LIMIT)
        .clamp(1, MAX_HISTORY_LIMIT);

    match db::fetch_dm_history(&state.db, &user, &peer, before, limit).await {
        Ok(rows) => {
            let mut msgs = Vec::new();
            for (id, content) in rows.into_iter().rev() {
                if let Ok(mut val) = serde_json::from_str::<Value>(&content) {
                    val["id"] = Value::from(id);
                    msgs.push(val);
                }
            }
            let payload = serde_json::json!({
                "type": "dm-history",
                "with": peer,
                "messages": msgs,
            });
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
        }
        Err(e) => {
            error!("Failed to load DM history between {user} and {peer}: {e}");
            send_error(sender, errors::DM_HISTORY_FAILED).await;
        }
    }
}
