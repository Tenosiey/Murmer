//! Handlers for the server-wide screen share configuration.
//!
//! Screen share video is exchanged peer-to-peer; the server only stores one
//! policy value: an optional bitrate cap (bits per second) that clients apply
//! to their outgoing stream. The cap persists in the `server_settings` table,
//! is sent to every client after authentication and broadcast whenever it
//! changes. Changing it requires an Admin or Owner role, checked server-side
//! against the role map.

use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Serialize the current configuration as a `screenshare-config` frame.
/// `maxBitrate` is a number in bits per second, or `null` when uncapped.
async fn screenshare_config_frame(state: &Arc<AppState>) -> Option<String> {
    let max_bitrate = match db::screenshare_max_bitrate(&state.db).await {
        Ok(value) => value,
        Err(e) => {
            error!("Failed to load screen share config: {e}");
            return None;
        }
    };
    serde_json::to_string(&serde_json::json!({
        "type": "screenshare-config",
        "maxBitrate": max_bitrate,
    }))
    .ok()
}

/// Send the current screen share configuration to a single client (used
/// right after authentication).
pub(super) async fn send_screenshare_config(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    if let Some(msg) = screenshare_config_frame(state).await {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Handle `set-screenshare-max-bitrate`: store the cap and broadcast the new
/// configuration. `maxBitrate` must be `null` (remove the cap) or a number of
/// bits per second within the allowed bounds.
pub(super) async fn handle_set_screenshare_max_bitrate(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::SCREENSHARE_PERMISSION_DENIED).await;
        return;
    };

    // Server-side role check; clients cannot spoof this.
    let allowed = {
        let roles = state.roles.lock().await;
        roles
            .get(requester)
            .map(|info| {
                SCREENSHARE_ADMIN_ROLES
                    .iter()
                    .any(|role| info.role.eq_ignore_ascii_case(role))
            })
            .unwrap_or(false)
    };
    if !allowed {
        warn!(
            "User {requester} attempted to change the screen share bitrate cap without permission"
        );
        send_error(sender, errors::SCREENSHARE_PERMISSION_DENIED).await;
        return;
    }

    let max_bitrate = match v.get("maxBitrate") {
        Some(raw) if raw.is_null() => None,
        Some(raw) => match raw.as_u64() {
            Some(value) if (MIN_SCREENSHARE_BITRATE..=MAX_SCREENSHARE_BITRATE).contains(&value) => {
                Some(value)
            }
            _ => {
                send_error(sender, errors::INVALID_SCREENSHARE_BITRATE).await;
                return;
            }
        },
        None => {
            send_error(sender, errors::INVALID_SCREENSHARE_BITRATE).await;
            return;
        }
    };

    if let Err(e) = db::set_screenshare_max_bitrate(&state.db, max_bitrate).await {
        error!("Failed to store screen share bitrate cap: {e}");
        send_error(sender, errors::SCREENSHARE_UPDATE_FAILED).await;
        return;
    }

    info!(requester, ?max_bitrate, "Screen share bitrate cap updated");
    if let Some(msg) = screenshare_config_frame(state).await {
        let _ = state.tx.send(msg);
    }
}
