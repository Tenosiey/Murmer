//! Handlers for the voice defaults: the quality preset and bitrate a newly
//! created voice channel starts with.
//!
//! These are defaults, not a cap: an existing channel keeps what it was
//! created with, and a client that names its own quality/bitrate still wins.
//! They are announced to every client after authentication and broadcast on
//! change so the "create voice channel" dialog can preselect them.

use crate::ws::{errors, helpers::*, validation::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Serialize the current defaults as a `voice-defaults` frame. `bitrate` is
/// `null` for an uncompressed ("lossless") channel.
async fn voice_defaults_frame(state: &Arc<AppState>) -> Option<String> {
    let defaults = match db::voice_defaults(&state.db).await {
        Ok(defaults) => defaults,
        Err(e) => {
            error!("Failed to load voice defaults: {e}");
            return None;
        }
    };
    serde_json::to_string(&serde_json::json!({
        "type": "voice-defaults",
        "quality": defaults.quality,
        "bitrate": defaults.bitrate,
    }))
    .ok()
}

/// Send the current voice defaults to a single client (used right after
/// authentication).
pub(super) async fn send_voice_defaults(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    if let Some(msg) = voice_defaults_frame(state).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Handle `set-voice-defaults`: store the quality/bitrate pair and broadcast
/// it. `bitrate` must be `null` (uncompressed) or a positive value within the
/// allowed bound; `quality` is validated like a channel's own label.
pub(super) async fn handle_set_voice_defaults(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::VOICE_DEFAULTS_PERMISSION_DENIED).await;
        return;
    };

    // Server-side permission check; clients cannot spoof this.
    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        warn!("User {requester} attempted to change the voice defaults without permission");
        send_error(sender, errors::VOICE_DEFAULTS_PERMISSION_DENIED).await;
        return;
    }

    let quality = v
        .get("quality")
        .and_then(|q| q.as_str())
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if !validate_voice_quality(&quality) {
        send_error(sender, errors::INVALID_VOICE_QUALITY).await;
        return;
    }

    let bitrate = match v.get("bitrate") {
        Some(raw) if raw.is_null() => None,
        Some(raw) => match raw.as_i64().and_then(validate_bitrate) {
            Some(valid) => Some(valid),
            None => {
                send_error(sender, errors::INVALID_VOICE_BITRATE).await;
                return;
            }
        },
        None => {
            send_error(sender, errors::INVALID_VOICE_BITRATE).await;
            return;
        }
    };

    let defaults = db::VoiceDefaults { quality, bitrate };
    if let Err(e) = db::set_voice_defaults(&state.db, &defaults).await {
        error!("Failed to store voice defaults: {e}");
        send_error(sender, errors::VOICE_DEFAULTS_UPDATE_FAILED).await;
        return;
    }

    info!(
        requester,
        quality = defaults.quality,
        ?bitrate,
        "Voice defaults updated"
    );
    if let Some(msg) = voice_defaults_frame(state).await {
        let _ = state.tx.send(msg.into());
    }
}
