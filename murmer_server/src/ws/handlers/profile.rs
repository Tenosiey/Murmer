//! Handlers for the user profile: avatar, display name and about text.
//!
//! Avatars travel through the regular `/upload` endpoint (which enforces the
//! image safe-list and magic-byte validation) and are registered here by URL,
//! mirroring the server icon. Every user may only set their own avatar; the
//! value persists on the user's name/key binding, is broadcast on change and
//! snapshotted to every client after authentication.
//!
//! The display name and about text live on the same binding row and follow the
//! same shape (self-service, broadcast, snapshotted). The **display name is
//! cosmetic**: the account name remains the identity used for authentication,
//! role assignment, moderation, DM routing and message authorship, so nothing
//! server-side ever resolves a display name back to a user. That is what keeps
//! it safe to let it collide with another user's name — clients show the
//! account name alongside it on the profile.

use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// Send all configured avatars to a newly connected client.
pub(super) async fn send_all_avatars(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let avatars = match db::get_all_avatars(&state.db).await {
        Ok(list) => list,
        Err(e) => {
            error!("Failed to load avatars: {e}");
            return;
        }
    };
    if avatars.is_empty() {
        return;
    }
    let map: serde_json::Map<String, Value> = avatars
        .into_iter()
        .map(|(user, avatar)| (user, Value::String(avatar)))
        .collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "avatar-snapshot",
        "avatars": map,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Send every known user's profile to a newly connected client. Includes users
/// who are currently offline: the member list shows them too, and "member
/// since" must be available without a round trip when a profile is opened.
pub(super) async fn send_all_profiles(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let profiles = match db::get_all_profiles(&state.db).await {
        Ok(list) => list,
        Err(e) => {
            error!("Failed to load profiles: {e}");
            return;
        }
    };
    if profiles.is_empty() {
        return;
    }
    let list: Vec<Value> = profiles.into_iter().map(profile_json).collect();
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "profile-snapshot",
        "profiles": list,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Serialize one profile row for the snapshot and update frames.
fn profile_json(profile: db::UserProfile) -> Value {
    serde_json::json!({
        "user": profile.user_name,
        "displayName": profile.display_name,
        "about": profile.about,
        "createdAt": profile.created_at,
    })
}

/// Broadcast a user's avatar change to all clients. `None` clears the avatar.
async fn broadcast_avatar(state: &Arc<AppState>, user: &str, avatar: Option<&str>) {
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "avatar-update",
        "user": user,
        "avatar": avatar,
    })) {
        let _ = state.tx.send(msg.into());
    }
}

/// Validate a `set-avatar` reference: it must be a stored upload within the
/// avatar size cap. Returns the validated URL.
async fn validate_avatar_url(state: &Arc<AppState>, url: &str) -> Option<String> {
    let key = upload_key_from_url(url)?;
    match tokio::fs::metadata(state.upload_dir.join(key)).await {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_AVATAR_BYTES => Some(url.to_string()),
        _ => None,
    }
}

/// Handle `set-avatar`: register an uploaded image as the requester's own
/// avatar, or clear it with `"avatar": null`. The change is persisted and
/// broadcast; the replaced file is removed once nobody references it.
pub(super) async fn handle_set_avatar(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::AVATAR_UPDATE_FAILED).await;
        return;
    };

    let new_avatar = match v.get("avatar") {
        Some(raw) if raw.is_null() => String::new(),
        Some(raw) => {
            let Some(url) = raw.as_str() else {
                send_error(sender, errors::INVALID_AVATAR).await;
                return;
            };
            let Some(validated) = validate_avatar_url(state, url).await else {
                send_error(sender, errors::INVALID_AVATAR).await;
                return;
            };
            validated
        }
        None => return,
    };

    let old_avatar = match db::get_user_avatar(&state.db, requester).await {
        Ok(old) => old.filter(|old| *old != new_avatar),
        Err(e) => {
            error!("Failed to load current avatar for {requester}: {e}");
            send_error(sender, errors::AVATAR_UPDATE_FAILED).await;
            return;
        }
    };

    match db::set_user_avatar(&state.db, requester, &new_avatar).await {
        Ok(true) => {}
        // No binding row — bots and half-authenticated sessions have no
        // profile to attach an avatar to.
        Ok(false) => {
            send_error(sender, errors::AVATAR_UPDATE_FAILED).await;
            return;
        }
        Err(e) => {
            error!("Failed to store avatar for {requester}: {e}");
            send_error(sender, errors::AVATAR_UPDATE_FAILED).await;
            return;
        }
    }

    // Best-effort cleanup of the replaced file. Unlike the server icon an
    // upload URL can be referenced by several users, so it is only deleted
    // once the last reference is gone; the URL is re-validated before
    // touching the filesystem in case the DB row was tampered with.
    if let Some(old_url) = old_avatar
        && db::count_avatar_references(&state.db, &old_url)
            .await
            .is_ok_and(|count| count == 0)
        && let Some(key) = upload_key_from_url(&old_url)
    {
        let _ = tokio::fs::remove_file(state.upload_dir.join(key)).await;
    }

    info!(requester, "Avatar updated");
    let avatar = (!new_avatar.is_empty()).then_some(new_avatar);
    broadcast_avatar(state, requester, avatar.as_deref()).await;
}

/// Read an optional profile text field. `Ok(None)` means "not in the frame,
/// leave it alone"; `null` and `""` both clear it. `Err(())` after sending
/// `error` for a malformed or over-long value.
async fn parse_profile_field(
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    field: &str,
    valid: impl Fn(&str) -> bool,
    error: &'static str,
) -> Result<Option<String>, ()> {
    match v.get(field) {
        None => Ok(None),
        Some(raw) if raw.is_null() => Ok(Some(String::new())),
        Some(raw) => {
            let Some(text) = raw.as_str().map(str::trim) else {
                send_error(sender, error).await;
                return Err(());
            };
            if !valid(text) {
                send_error(sender, error).await;
                return Err(());
            }
            Ok(Some(text.to_string()))
        }
    }
}

/// Handle `set-profile`: update the requester's own display name and/or about
/// text. Absent fields are left untouched, `null` (or an empty string) clears
/// one. The change is persisted and broadcast to every client.
pub(super) async fn handle_set_profile(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
        return;
    };

    let Ok(display_name) = parse_profile_field(
        sender,
        v,
        "displayName",
        validate_display_name,
        errors::INVALID_DISPLAY_NAME,
    )
    .await
    else {
        return;
    };
    let Ok(about) =
        parse_profile_field(sender, v, "about", validate_about, errors::INVALID_ABOUT).await
    else {
        return;
    };
    if display_name.is_none() && about.is_none() {
        return;
    }

    match db::set_user_profile(
        &state.db,
        requester,
        display_name.as_deref(),
        about.as_deref(),
    )
    .await
    {
        Ok(true) => {}
        // No binding row — bots and half-authenticated sessions have no
        // profile to edit.
        Ok(false) => {
            send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
            return;
        }
        Err(e) => {
            error!("Failed to store profile for {requester}: {e}");
            send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
            return;
        }
    }

    // Re-read the row so the broadcast carries the full profile even when the
    // frame only touched one field.
    match db::get_user_profile(&state.db, requester).await {
        Ok(Some(profile)) => {
            info!(requester, "Profile updated");
            if let Ok(msg) = serde_json::to_string(&serde_json::json!({
                "type": "profile-update",
                "profile": profile_json(profile),
            })) {
                let _ = state.tx.send(msg.into());
            }
        }
        Ok(None) => send_error(sender, errors::PROFILE_UPDATE_FAILED).await,
        Err(e) => {
            error!("Failed to reload profile for {requester}: {e}");
            send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
        }
    }
}
