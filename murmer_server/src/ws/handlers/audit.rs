//! Reading the audit log.
//!
//! There is no write frame: entries are appended by the handlers that carry
//! out the actions, never by a client asking for one. What is here is the
//! read, and it answers the requester alone — a broadcast would put "who
//! banned whom" in front of everybody.
//!
//! Gated on `VIEW_AUDIT_LOG`, which is a flag of its own rather than a
//! by-product of `BAN_MEMBERS` or `MANAGE_SERVER`: the log is the record *of*
//! the moderators, so who may read it is a separate decision from who may
//! act. See [`crate::db::audit`] for what is recorded.

use crate::ws::{errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

/// Handle `get-audit-log`: answer one viewer with the newest entries.
pub(super) async fn handle_get_audit_log(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::AUDIT_LOG_PERMISSION_DENIED).await;
        return;
    };

    if !has_permission(state, requester, crate::permissions::VIEW_AUDIT_LOG).await {
        send_error(sender, errors::AUDIT_LOG_PERMISSION_DENIED).await;
        return;
    }

    let entries = match db::list_audit_entries(&state.db).await {
        Ok(entries) => entries,
        Err(e) => {
            error!("Failed to load the audit log: {e}");
            send_error(sender, errors::AUDIT_LOG_FAILED).await;
            return;
        }
    };

    let rows: Vec<Value> = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "action": entry.action,
                "actor": entry.actor,
                "target": entry.target,
                "detail": entry.detail,
                "at": entry.created_at.to_rfc3339(),
            })
        })
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "audit-log",
        "entries": rows,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}
