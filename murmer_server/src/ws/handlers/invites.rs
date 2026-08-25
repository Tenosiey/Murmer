//! Handlers for server-issued invite codes.
//!
//! Minting, listing and revoking all require `CREATE_INVITES`, and the answer
//! is only ever sent back to the requester: the codes are credentials on a
//! password-protected server, so the invite list is manager-only information
//! and never a broadcast. Redemption itself lives in [`super::auth`], where
//! the connecting key is known.
//!
//! A create or revoke replies with the whole refreshed list rather than the
//! one row that changed — it keeps the client from having to merge, and the
//! list is capped at [`MAX_INVITES`] rows.

use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use base64::Engine as _;
use chrono::{Duration, Utc};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// A fresh, unguessable invite code.
fn generate_code() -> String {
    let bytes: [u8; INVITE_CODE_BYTES] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Resolve the requester and confirm they may manage invites. Sends the
/// permission error and returns `None` otherwise.
async fn require_inviter(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) -> Option<String> {
    let Some(requester) = user_name.clone() else {
        send_error(sender, errors::INVITE_PERMISSION_DENIED).await;
        return None;
    };
    if !has_permission(state, &requester, crate::permissions::CREATE_INVITES).await {
        warn!("User {requester} attempted to manage invites without permission");
        send_error(sender, errors::INVITE_PERMISSION_DENIED).await;
        return None;
    }
    Some(requester)
}

/// Send the current invite list to this connection. The caller has already
/// checked the permission.
async fn send_invites(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let invites = match db::list_invites(&state.db).await {
        Ok(invites) => invites,
        Err(e) => {
            error!("Failed to load the invite list: {e}");
            send_error(sender, errors::INVITE_UPDATE_FAILED).await;
            return;
        }
    };

    let entries: Vec<Value> = invites
        .iter()
        .map(|invite| {
            serde_json::json!({
                "code": invite.code,
                "createdBy": invite.created_by,
                "createdAt": invite.created_at,
                "expiresAt": invite.expires_at,
                "maxUses": invite.max_uses,
                "uses": invite.uses,
            })
        })
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "invite-list",
        "invites": entries,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Handle a request for the server's invite list.
pub(super) async fn handle_get_invites(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    if require_inviter(state, sender, user_name).await.is_none() {
        return;
    }
    send_invites(state, sender).await;
}

/// Handle a request to mint a new invite.
///
/// `expiresIn` is a lifetime in seconds and `maxUses` a use limit; either may
/// be omitted or `0` for "no limit". They are taken as numbers rather than an
/// absolute date so a client with a wrong clock cannot mint an invite that is
/// already expired — or one that outlives [`MAX_INVITE_TTL_SECONDS`].
pub(super) async fn handle_create_invite(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = require_inviter(state, sender, user_name).await else {
        return;
    };

    let expires_in = v.get("expiresIn").and_then(|e| e.as_i64()).unwrap_or(0);
    let max_uses = v.get("maxUses").and_then(|m| m.as_i64()).unwrap_or(0);
    if !(0..=MAX_INVITE_TTL_SECONDS).contains(&expires_in)
        || !(0..=MAX_INVITE_USES).contains(&max_uses)
    {
        send_error(sender, errors::INVALID_INVITE_OPTIONS).await;
        return;
    }

    match db::count_invites(&state.db).await {
        Ok(count) if count >= MAX_INVITES => {
            send_error(sender, errors::INVITE_LIMIT_REACHED).await;
            return;
        }
        Ok(_) => {}
        Err(e) => {
            error!("db invite count error: {e}");
            send_error(sender, errors::INVITE_UPDATE_FAILED).await;
            return;
        }
    }

    let expires_at = (expires_in > 0).then(|| Utc::now() + Duration::seconds(expires_in));
    let code = generate_code();
    if let Err(e) = db::create_invite(&state.db, &code, &requester, expires_at, max_uses).await {
        error!("db create invite error: {e}");
        send_error(sender, errors::INVITE_UPDATE_FAILED).await;
        return;
    }

    // The code itself is deliberately absent from the log: it is a credential,
    // and operator logs are the wrong place to leak one.
    info!(requester, expires_in, max_uses, "Invite created");
    send_invites(state, sender).await;
}

/// Handle a request to withdraw an invite.
pub(super) async fn handle_revoke_invite(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = require_inviter(state, sender, user_name).await else {
        return;
    };

    let code = v.get("code").and_then(|c| c.as_str()).unwrap_or("").trim();
    if code.is_empty() {
        send_error(sender, errors::INVITE_NOT_FOUND).await;
        return;
    }

    match db::revoke_invite(&state.db, code).await {
        Ok(true) => {
            info!(requester, "Invite revoked");
            send_invites(state, sender).await;
        }
        Ok(false) => send_error(sender, errors::INVITE_NOT_FOUND).await,
        Err(e) => {
            error!("db revoke invite error: {e}");
            send_error(sender, errors::INVITE_UPDATE_FAILED).await;
        }
    }
}
