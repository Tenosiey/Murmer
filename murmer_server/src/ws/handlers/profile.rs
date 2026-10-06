//! Handlers for the user profile: avatar, display name, nickname and about
//! text.
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
//!
//! The **nickname** sits on top of the display name and is the one field here
//! somebody else may write: a member with `MANAGE_NICKNAMES` who outranks the
//! target can relabel them for this server. It is just as cosmetic, which is
//! why the extra reach is safe — see [`handle_set_nickname`].
//!
//! The **custom status line** ("back at 3") is self-service like the display
//! name, with an optional expiry. Nothing clears it when that passes: every
//! profile frame is built through [`db::UserProfile::active_status`], which
//! leaves a lapsed line out, and clients drop it on their own clock while
//! connected. A timer would be one more background task for a cosmetic line.
//!
//! Every frame here is broadcast to every connection, so each is published
//! text in all but name. [`may_publish`] holds them to the mute and message
//! rate limit a chat message passes, and [`screened`] to the same
//! auto-moderation and profanity filter: without that a muted member kept
//! talking to everyone through their status line.

use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, db, permissions};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// Whether `user` may broadcast a profile change: not muted, and inside the
/// message rate limit. Sends the refusal itself.
async fn may_publish(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
) -> bool {
    if super::moderation::is_muted(state, user).await {
        send_error(sender, errors::MUTED).await;
        return false;
    }
    if !crate::security::check_message_rate_limit(&state.rate_limiter, user).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return false;
    }
    true
}

/// `text` run through auto-moderation and the profanity mask, as a chat
/// message is, or `None` when a rule blocked it (the sender has been told).
async fn screened(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
    text: String,
) -> Option<String> {
    if text.is_empty() {
        return Some(text);
    }
    if super::automod::screen(state, sender, user, &text).await == super::automod::Screen::Blocked {
        return None;
    }
    Some(super::chat_settings::mask_text(state, &text).await)
}

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
    send_json(
        sender,
        &serde_json::json!({
            "type": "avatar-snapshot",
            "avatars": map,
        }),
    )
    .await;
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
    send_json(
        sender,
        &serde_json::json!({
            "type": "profile-snapshot",
            "profiles": list,
        }),
    )
    .await;
}

/// Serialize one profile row for the snapshot and update frames.
fn profile_json(profile: db::UserProfile) -> Value {
    let (status_text, status_expires_at) =
        profile.active_status(chrono::Utc::now().timestamp_millis());
    serde_json::json!({
        "statusText": status_text,
        "statusExpiresAt": (status_expires_at != 0).then_some(status_expires_at),
        "user": profile.user_name,
        "displayName": profile.display_name,
        "nickname": profile.nickname,
        "about": profile.about,
        "createdAt": profile.created_at,
    })
}

/// Re-read a user's profile row and broadcast it. Used after every change so
/// the frame carries the full profile even when only one field was touched.
/// Returns `false` when the row is gone, which the caller reports as a failure.
async fn broadcast_profile(state: &Arc<AppState>, user: &str) -> bool {
    match db::get_user_profile(&state.db, user).await {
        Ok(Some(profile)) => {
            broadcast(
                state,
                &serde_json::json!({
                    "type": "profile-update",
                    "profile": profile_json(profile),
                }),
            );
            true
        }
        Ok(None) => false,
        Err(e) => {
            error!("Failed to reload profile for {user}: {e}");
            false
        }
    }
}

/// Broadcast a user's avatar change to all clients. `None` clears the avatar.
fn broadcast_avatar(state: &Arc<AppState>, user: &str, avatar: Option<&str>) {
    broadcast(
        state,
        &serde_json::json!({
            "type": "avatar-update",
            "user": user,
            "avatar": avatar,
        }),
    );
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

    if !may_publish(state, sender, requester).await {
        return;
    }

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
        crate::upload::remove_upload(state, key).await;
    }

    info!(requester, "Avatar updated");
    let avatar = (!new_avatar.is_empty()).then_some(new_avatar);
    broadcast_avatar(state, requester, avatar.as_deref());
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
    if !may_publish(state, sender, requester).await {
        return;
    }
    let display_name = match display_name {
        Some(text) => match screened(state, sender, requester, text).await {
            Some(text) => Some(text),
            None => return,
        },
        None => None,
    };
    let about = match about {
        Some(text) => match screened(state, sender, requester, text).await {
            Some(text) => Some(text),
            None => return,
        },
        None => None,
    };

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

    if broadcast_profile(state, requester).await {
        info!(requester, "Profile updated");
    } else {
        send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
    }
}

/// Handle `set-nickname`: set or clear a member's per-server nickname.
///
/// Two paths, and only the second is a moderation action: setting your **own**
/// nickname is as self-service as the display name, while relabelling somebody
/// else needs [`MANAGE_NICKNAMES`](crate::permissions::MANAGE_NICKNAMES) *and*
/// strictly outranking them — otherwise a moderator could rename the owner.
/// The nickname stays cosmetic either way: it never becomes an identity the
/// server resolves anything by, so it may collide with a real account name
/// without that name meaning anything different to any check.
///
/// `"nickname": null` (or an empty string) clears it, falling back to the
/// user's own display name and then to their account name.
pub(super) async fn handle_set_nickname(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::NICKNAME_UPDATE_FAILED).await;
        return;
    };

    // An absent `user` means "my own", which is what the profile editor sends.
    let target = match v.get("user") {
        None => requester.to_string(),
        Some(raw) => {
            let Some(name) = raw.as_str().map(str::trim).filter(|n| !n.is_empty()) else {
                send_error(sender, errors::NICKNAME_UPDATE_FAILED).await;
                return;
            };
            name.to_string()
        }
    };

    let Ok(nickname) = parse_profile_field(
        sender,
        v,
        "nickname",
        validate_nickname,
        errors::INVALID_NICKNAME,
    )
    .await
    else {
        return;
    };
    // Nothing to do: the frame did not carry the field at all.
    let Some(nickname) = nickname else {
        return;
    };
    if !may_publish(state, sender, requester).await {
        return;
    }
    let Some(nickname) = screened(state, sender, requester, nickname).await else {
        return;
    };

    if target != requester {
        if !has_permission(state, requester, permissions::MANAGE_NICKNAMES).await {
            send_error(sender, errors::NICKNAME_PERMISSION_DENIED).await;
            return;
        }
        if top_position(state, requester).await <= top_position(state, &target).await {
            send_error(sender, errors::NICKNAME_PERMISSION_DENIED).await;
            return;
        }
    }

    match db::set_user_nickname(&state.db, &target, &nickname).await {
        Ok(true) => {}
        // No binding row — the name has never connected, so there is nothing
        // to label.
        Ok(false) => {
            send_error(sender, errors::NICKNAME_UPDATE_FAILED).await;
            return;
        }
        Err(e) => {
            error!("Failed to store nickname for {target}: {e}");
            send_error(sender, errors::NICKNAME_UPDATE_FAILED).await;
            return;
        }
    }

    if broadcast_profile(state, &target).await {
        info!(requester, target, "Nickname updated");
    } else {
        send_error(sender, errors::NICKNAME_UPDATE_FAILED).await;
    }
}

/// Handle `set-status-text`: set or clear the requester's own custom status
/// line. `text` empty (or `null`) clears it; `expiresAt` is optional Unix
/// milliseconds, in the future and at most [`MAX_STATUS_TEXT_TTL_MS`] ahead.
pub(super) async fn handle_set_status_text(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
        return;
    };

    let Ok(Some(text)) = parse_profile_field(
        sender,
        v,
        "text",
        validate_status_text,
        errors::INVALID_STATUS_TEXT,
    )
    .await
    else {
        return;
    };

    if !may_publish(state, sender, requester).await {
        return;
    }
    let Some(text) = screened(state, sender, requester, text).await else {
        return;
    };

    let now = chrono::Utc::now().timestamp_millis();
    let expires_at = match v.get("expiresAt") {
        None | Some(Value::Null) => 0,
        Some(raw) => match raw.as_i64() {
            Some(at) if at > now && at - now <= MAX_STATUS_TEXT_TTL_MS => at,
            _ => {
                send_error(sender, errors::INVALID_STATUS_TEXT).await;
                return;
            }
        },
    };
    // An empty line has nothing to expire.
    let expires_at = if text.is_empty() { 0 } else { expires_at };

    match db::set_user_status_text(&state.db, requester, &text, expires_at).await {
        Ok(true) => {}
        Ok(false) => {
            send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
            return;
        }
        Err(e) => {
            error!("Failed to store status text for {requester}: {e}");
            send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
            return;
        }
    }

    if !broadcast_profile(state, requester).await {
        send_error(sender, errors::PROFILE_UPDATE_FAILED).await;
    }
}
