//! Handlers for custom server emoji management.
//!
//! Emoji images travel through the regular `/upload` endpoint (which already
//! enforces the image safe-list and magic-byte validation); registration and
//! deletion happen here over the authenticated WebSocket, where the
//! requester's role is known. The server re-validates the uploaded file
//! before registering it, so the open upload endpoint grants no extra power.

use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Handle a request to register a custom emoji from a previously uploaded image.
pub(super) async fn handle_add_emoji(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            send_error(sender, errors::EMOJI_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_emojis(state, requester).await {
        warn!("User {requester} attempted to add an emoji without permission");
        send_error(sender, errors::EMOJI_PERMISSION_DENIED).await;
        return;
    }

    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if !validate_emoji_name(&name) {
        send_error(sender, errors::INVALID_EMOJI_NAME).await;
        return;
    }

    let Some(url) = v.get("url").and_then(|u| u.as_str()) else {
        send_error(sender, errors::INVALID_EMOJI_URL).await;
        return;
    };
    let Some(key) = upload_key_from_url(url) else {
        send_error(sender, errors::INVALID_EMOJI_URL).await;
        return;
    };

    // The upload endpoint is open, so confirm the referenced file really is a
    // stored upload within the emoji size cap before registering it.
    match tokio::fs::metadata(state.upload_dir.join(key)).await {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_EMOJI_FILE_BYTES => {}
        Ok(_) | Err(_) => {
            send_error(sender, errors::INVALID_EMOJI_URL).await;
            return;
        }
    }

    match db::count_emojis(&state.db).await {
        Ok(count) if count >= MAX_CUSTOM_EMOJIS => {
            send_error(sender, errors::EMOJI_LIMIT_REACHED).await;
            return;
        }
        Ok(_) => {}
        Err(e) => {
            error!("db emoji count error: {e}");
            send_error(sender, errors::EMOJI_UPDATE_FAILED).await;
            return;
        }
    }

    match db::add_emoji(&state.db, &name, url, requester).await {
        Ok(true) => {
            info!(requester, name, "Custom emoji added");
            broadcast_emojis(state).await;
        }
        Ok(false) => {
            send_error(sender, errors::EMOJI_NAME_TAKEN).await;
        }
        Err(e) => {
            error!("db add emoji error: {e}");
            send_error(sender, errors::EMOJI_UPDATE_FAILED).await;
        }
    }
}

/// Handle a request to delete a custom emoji.
pub(super) async fn handle_remove_emoji(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            send_error(sender, errors::EMOJI_PERMISSION_DENIED).await;
            return;
        }
    };

    if !can_manage_emojis(state, requester).await {
        warn!("User {requester} attempted to remove an emoji without permission");
        send_error(sender, errors::EMOJI_PERMISSION_DENIED).await;
        return;
    }

    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if !validate_emoji_name(&name) {
        send_error(sender, errors::INVALID_EMOJI_NAME).await;
        return;
    }

    match db::remove_emoji(&state.db, &name).await {
        Ok(Some(url)) => {
            // Best-effort file cleanup; re-validate the stored URL before
            // touching the filesystem in case the DB row was tampered with.
            if let Some(key) = upload_key_from_url(&url) {
                let _ = tokio::fs::remove_file(state.upload_dir.join(key)).await;
            }
            info!(requester, name, "Custom emoji removed");
            broadcast_emojis(state).await;
        }
        Ok(None) => {
            send_error(sender, errors::EMOJI_NOT_FOUND).await;
        }
        Err(e) => {
            error!("db remove emoji error: {e}");
            send_error(sender, errors::EMOJI_UPDATE_FAILED).await;
        }
    }
}
