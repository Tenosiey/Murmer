//! Handlers for the configurable server identity (dashboard Overview tab).
//!
//! The identity consists of the server name, a short description, a welcome
//! message and an icon. All values persist in the `server_settings` table;
//! the icon travels through the regular `/upload` endpoint (which enforces
//! the image safe-list and magic-byte validation) and is registered here by
//! URL, mirroring custom emojis. The full identity is sent to every client
//! after authentication and broadcast whenever it changes. Editing requires
//! an Admin or Owner role, checked server-side against the role map.

use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Serialize the current server identity as a `server-identity` frame.
async fn server_identity_frame(state: &Arc<AppState>) -> Option<String> {
    let identity = match db::get_server_identity(&state.db).await {
        Ok(identity) => identity,
        Err(e) => {
            error!("Failed to load server identity: {e}");
            return None;
        }
    };
    serde_json::to_string(&serde_json::json!({
        "type": "server-identity",
        "name": identity.name,
        "description": identity.description,
        "welcomeMessage": identity.welcome_message,
        "icon": identity.icon,
    }))
    .ok()
}

/// Send the current server identity to a single client (used right after
/// authentication so every member knows the server's name and icon).
pub(super) async fn send_server_identity(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    if let Some(msg) = server_identity_frame(state).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Broadcast the current server identity to all connected clients.
async fn broadcast_server_identity(state: &Arc<AppState>) {
    if let Some(msg) = server_identity_frame(state).await {
        let _ = state.tx.send(msg);
    }
}

/// Send the configured welcome message to a client whose user connected for
/// the first time (their name/key binding was just created). No frame is
/// sent when no welcome message is configured.
pub(super) async fn send_welcome(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let identity = match db::get_server_identity(&state.db).await {
        Ok(identity) => identity,
        Err(e) => {
            error!("Failed to load server identity for welcome message: {e}");
            return;
        }
    };
    if identity.welcome_message.is_empty() {
        return;
    }
    let msg = serde_json::json!({
        "type": "welcome",
        "serverName": identity.name,
        "message": identity.welcome_message,
    });
    let _ = sender.send(Message::Text(msg.to_string().into())).await;
}

/// Validate a `set-server-identity` icon reference: it must be a stored
/// upload within the icon size cap. Returns the validated URL.
async fn validate_icon_url(state: &Arc<AppState>, url: &str) -> Option<String> {
    let key = upload_key_from_url(url)?;
    match tokio::fs::metadata(state.upload_dir.join(key)).await {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_SERVER_ICON_BYTES => Some(url.to_string()),
        _ => None,
    }
}

/// Handle `set-server-identity`: update any subset of the identity fields.
/// Absent fields are left untouched; `"icon": null` clears the icon. The
/// change is persisted and the full identity is broadcast to all clients.
pub(super) async fn handle_set_server_identity(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            send_error(sender, errors::IDENTITY_PERMISSION_DENIED).await;
            return;
        }
    };

    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        warn!("User {requester} attempted to edit the server identity without permission");
        send_error(sender, errors::IDENTITY_PERMISSION_DENIED).await;
        return;
    }

    let mut fields: Vec<(&'static str, String)> = Vec::new();

    if let Some(raw) = v.get("name") {
        let Some(name) = raw
            .as_str()
            .map(str::trim)
            .filter(|n| validate_server_name(n))
        else {
            send_error(sender, errors::INVALID_SERVER_NAME).await;
            return;
        };
        fields.push((db::IDENTITY_NAME_KEY, name.to_string()));
    }

    if let Some(raw) = v.get("description") {
        let Some(description) = raw
            .as_str()
            .map(str::trim)
            .filter(|d| validate_server_description(d))
        else {
            send_error(sender, errors::INVALID_SERVER_DESCRIPTION).await;
            return;
        };
        fields.push((db::IDENTITY_DESCRIPTION_KEY, description.to_string()));
    }

    if let Some(raw) = v.get("welcomeMessage") {
        let Some(welcome) = raw
            .as_str()
            .map(str::trim)
            .filter(|w| validate_welcome_message(w))
        else {
            send_error(sender, errors::INVALID_WELCOME_MESSAGE).await;
            return;
        };
        fields.push((db::IDENTITY_WELCOME_KEY, welcome.to_string()));
    }

    // The icon needs extra care: a string must reference a stored upload
    // (the open upload endpoint grants no extra power that way), `null`
    // clears it, and the previously registered icon file is cleaned up.
    let mut replaced_icon: Option<String> = None;
    if let Some(raw) = v.get("icon") {
        let new_icon = if raw.is_null() {
            String::new()
        } else {
            let Some(url) = raw.as_str() else {
                send_error(sender, errors::INVALID_SERVER_ICON).await;
                return;
            };
            let Some(validated) = validate_icon_url(state, url).await else {
                send_error(sender, errors::INVALID_SERVER_ICON).await;
                return;
            };
            validated
        };
        match db::get_server_identity(&state.db).await {
            Ok(identity) => {
                replaced_icon = identity.icon.filter(|old| *old != new_icon);
            }
            Err(e) => {
                error!("Failed to load current server icon: {e}");
                send_error(sender, errors::IDENTITY_UPDATE_FAILED).await;
                return;
            }
        }
        fields.push((db::IDENTITY_ICON_KEY, new_icon));
    }

    if fields.is_empty() {
        return;
    }

    if let Err(e) = db::set_server_identity_fields(&state.db, fields).await {
        error!("Failed to store server identity: {e}");
        send_error(sender, errors::IDENTITY_UPDATE_FAILED).await;
        return;
    }

    // Best-effort cleanup of the replaced icon file; re-validate the stored
    // URL before touching the filesystem in case the DB row was tampered with.
    if let Some(old_url) = replaced_icon
        && let Some(key) = upload_key_from_url(&old_url)
    {
        let _ = tokio::fs::remove_file(state.upload_dir.join(key)).await;
    }

    info!(requester, "Server identity updated");
    broadcast_server_identity(state).await;
}
