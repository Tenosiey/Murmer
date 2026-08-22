//! Handlers for the server-wide chat policy: slow mode, the per-message
//! length cap and the profanity filter.
//!
//! The three public values are announced to every client after
//! authentication and broadcast on change, because the composer needs them to
//! show the right character counter and slow mode hint. The filtered *word
//! list* is not: it only travels in the direct answer to `get-chat-settings`
//! from a manager, the same way channel overrides are only sent to the people
//! who can edit them.
//!
//! All three are enforced here, in `messages.rs`, never by the client: the
//! composer's counter is cosmetic, and masking happens before a message is
//! stored or broadcast so a filtered word never reaches a reader at all.

use crate::ws::{errors, helpers::*};
use crate::{AppState, db, profanity};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

/// Serialize the cached chat policy as a `chat-settings` frame. The filtered
/// word list is only included for managers (`words`); its absence means "not
/// disclosed", not "empty".
async fn chat_settings_frame(state: &Arc<AppState>, include_words: bool) -> Option<String> {
    let settings = state.chat_settings.lock().await.clone();
    let mut payload = serde_json::json!({
        "type": "chat-settings",
        "slowModeSeconds": settings.slow_mode_seconds,
        "maxMessageLength": settings.max_message_length,
        "profanityFilter": settings.profanity_filter,
    });
    if include_words && let Some(map) = payload.as_object_mut() {
        map.insert("words".into(), serde_json::json!(settings.profanity_words));
    }
    serde_json::to_string(&payload).ok()
}

/// Send the public chat policy to a single client (used right after
/// authentication).
pub(super) async fn send_chat_settings(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    if let Some(msg) = chat_settings_frame(state, false).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Handle `get-chat-settings`: answer a manager with the policy including the
/// filtered word list. Unauthorised requests are dropped without an error
/// frame, mirroring `get-server-info`.
pub(super) async fn handle_get_chat_settings(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        return;
    };
    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        info!(requester, "Denied chat settings request");
        return;
    }
    if let Some(msg) = chat_settings_frame(state, true).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Handle `set-chat-settings`: store the policy, refresh the cache and
/// broadcast the public part. Values outside this build's bounds are clamped
/// rather than rejected; only structurally invalid frames are refused.
pub(super) async fn handle_set_chat_settings(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::CHAT_SETTINGS_PERMISSION_DENIED).await;
        return;
    };

    // Server-side permission check; clients cannot spoof this.
    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        warn!("User {requester} attempted to change the chat policy without permission");
        send_error(sender, errors::CHAT_SETTINGS_PERMISSION_DENIED).await;
        return;
    }

    let Some(slow_mode_seconds) = v.get("slowModeSeconds").and_then(|s| s.as_u64()) else {
        send_error(sender, errors::INVALID_CHAT_SETTINGS).await;
        return;
    };
    let Some(max_message_length) = v
        .get("maxMessageLength")
        .and_then(|l| l.as_u64())
        .map(|l| l as usize)
    else {
        send_error(sender, errors::INVALID_CHAT_SETTINGS).await;
        return;
    };
    let Some(profanity_filter) = v.get("profanityFilter").and_then(|p| p.as_bool()) else {
        send_error(sender, errors::INVALID_CHAT_SETTINGS).await;
        return;
    };
    let Some(raw_words) = v.get("words").and_then(|w| w.as_array()) else {
        send_error(sender, errors::INVALID_CHAT_SETTINGS).await;
        return;
    };
    let mut words: Vec<String> = Vec::with_capacity(raw_words.len());
    for entry in raw_words {
        match entry.as_str() {
            Some(word) => words.push(word.to_string()),
            None => {
                send_error(sender, errors::INVALID_CHAT_SETTINGS).await;
                return;
            }
        }
    }

    let requested = db::ChatSettings {
        slow_mode_seconds,
        max_message_length,
        profanity_filter,
        profanity_words: words,
    };
    let stored = match db::set_chat_settings(&state.db, &requested).await {
        Ok(stored) => stored,
        Err(e) => {
            error!("Failed to store chat settings: {e}");
            send_error(sender, errors::CHAT_SETTINGS_UPDATE_FAILED).await;
            return;
        }
    };

    info!(
        requester,
        slow_mode_seconds = stored.slow_mode_seconds,
        max_message_length = stored.max_message_length,
        profanity_filter = stored.profanity_filter,
        filtered_words = stored.profanity_words.len(),
        "Chat policy updated"
    );
    *state.chat_settings.lock().await = stored;

    if let Some(msg) = chat_settings_frame(state, false).await {
        let _ = state.tx.send(msg.into());
    }
    // The editor keeps its word list in sync from this answer; the broadcast
    // above deliberately carries no words.
    if let Some(msg) = chat_settings_frame(state, true).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// The configured per-message length cap.
pub(super) async fn max_message_length(state: &Arc<AppState>) -> usize {
    state.chat_settings.lock().await.max_message_length
}

/// Whether `user` may post right now under slow mode. Members who can manage
/// messages are exempt — slow mode exists to pace a busy room, and the people
/// moderating it need to answer without waiting their turn.
pub(super) async fn slow_mode_allows(state: &Arc<AppState>, user: &str) -> bool {
    let interval = state.chat_settings.lock().await.slow_mode_seconds;
    if interval == 0 {
        return true;
    }
    if has_permission(state, user, crate::permissions::MANAGE_MESSAGES).await {
        return true;
    }
    let sends = state.slow_mode_sends.lock().await;
    match sends.get(user) {
        Some(last) => last.elapsed().as_secs() >= interval,
        None => true,
    }
}

/// Record that `user` had a message accepted, starting their slow mode
/// interval. Only called once the message is on its way, so a rejected
/// message never costs the sender their turn.
pub(super) async fn note_message_sent(state: &Arc<AppState>, user: &str) {
    if state.chat_settings.lock().await.slow_mode_seconds == 0 {
        return;
    }
    state
        .slow_mode_sends
        .lock()
        .await
        .insert(user.to_string(), Instant::now());
}

/// The filtered word list, or `None` while the filter is off or empty.
async fn active_filter_words(state: &Arc<AppState>) -> Option<Vec<String>> {
    let settings = state.chat_settings.lock().await;
    (settings.profanity_filter && !settings.profanity_words.is_empty())
        .then(|| settings.profanity_words.clone())
}

/// Mask filtered words in `text`. Runs before a message is stored or
/// broadcast, so the masked form is the only one that ever exists outside the
/// sender's own composer.
pub(super) async fn mask_text(state: &Arc<AppState>, text: &str) -> String {
    match active_filter_words(state).await {
        Some(words) => profanity::mask(text, &words).unwrap_or_else(|| text.to_string()),
        None => text.to_string(),
    }
}

/// Mask filtered words in a message's `text` field, in place.
pub(super) async fn apply_profanity_filter(state: &Arc<AppState>, v: &mut Value) {
    let Some(words) = active_filter_words(state).await else {
        return;
    };
    let Some(text) = v.get("text").and_then(|t| t.as_str()) else {
        return;
    };
    if let Some(masked) = profanity::mask(text, &words) {
        v["text"] = Value::String(masked);
    }
}
