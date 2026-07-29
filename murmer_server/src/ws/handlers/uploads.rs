//! Handlers for the server-wide upload policy (size cap + file categories).
//!
//! The policy is enforced by the `/upload` HTTP endpoint, which reads it from
//! the database on every request. This module keeps clients in sync: the
//! current values are sent after authentication and broadcast whenever they
//! change, so the client can reject a file before uploading it and label the
//! attach button accordingly. Changing the policy requires `MANAGE_SERVER`,
//! checked server-side against the role map.

use crate::upload::{MAX_CONFIGURABLE_FILE_SIZE, MIN_CONFIGURABLE_FILE_SIZE, is_known_category};
use crate::ws::{errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Serialize the current policy as an `upload-config` frame.
async fn upload_config_frame(state: &Arc<AppState>) -> Option<String> {
    let config = match db::upload_config(&state.db).await {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load upload config: {e}");
            return None;
        }
    };
    serde_json::to_string(&serde_json::json!({
        "type": "upload-config",
        "maxBytes": config.max_bytes,
        "categories": config.categories,
    }))
    .ok()
}

/// Send the current upload policy to a single client (used right after
/// authentication).
pub(super) async fn send_upload_config(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    if let Some(msg) = upload_config_frame(state).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Handle `set-upload-config`: store the policy and broadcast it. `maxBytes`
/// must be within the configurable bounds and `categories` an array of known
/// category ids (an empty array disables uploads entirely).
pub(super) async fn handle_set_upload_config(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::UPLOAD_PERMISSION_DENIED).await;
        return;
    };

    // Server-side permission check; clients cannot spoof this.
    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        warn!("User {requester} attempted to change the upload policy without permission");
        send_error(sender, errors::UPLOAD_PERMISSION_DENIED).await;
        return;
    }

    let Some(max_bytes) = v.get("maxBytes").and_then(|b| b.as_u64()).filter(|&b| {
        (MIN_CONFIGURABLE_FILE_SIZE as u64..=MAX_CONFIGURABLE_FILE_SIZE as u64).contains(&b)
    }) else {
        send_error(sender, errors::INVALID_UPLOAD_CONFIG).await;
        return;
    };

    let Some(raw_categories) = v.get("categories").and_then(|c| c.as_array()) else {
        send_error(sender, errors::INVALID_UPLOAD_CONFIG).await;
        return;
    };

    // Every entry must name a category this build knows, so a client can never
    // widen the extension safe-list by inventing an id.
    let mut categories: Vec<String> = Vec::with_capacity(raw_categories.len());
    for entry in raw_categories {
        match entry.as_str() {
            Some(id) if is_known_category(id) => {
                if !categories.iter().any(|existing| existing == id) {
                    categories.push(id.to_string());
                }
            }
            _ => {
                send_error(sender, errors::INVALID_UPLOAD_CONFIG).await;
                return;
            }
        }
    }

    let config = db::UploadConfig {
        max_bytes,
        categories,
    };
    if let Err(e) = db::set_upload_config(&state.db, &config).await {
        error!("Failed to store upload config: {e}");
        send_error(sender, errors::UPLOAD_CONFIG_UPDATE_FAILED).await;
        return;
    }

    info!(
        requester,
        max_bytes,
        categories = ?config.categories,
        "Upload policy updated"
    );
    if let Some(msg) = upload_config_frame(state).await {
        let _ = state.tx.send(msg.into());
    }
}
