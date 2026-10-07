//! Authentication handlers for user and bot presence.
//!
//! On a password-protected server a connection is admitted by one of three
//! credentials, tried in that order: the server password, an existing
//! invite-granted membership bound to the connecting public key, or a live
//! invite code. Membership is checked *before* the code so a member's
//! reconnect neither spends one of the invite's uses nor depends on the
//! invite still existing — which is what lets an invite be revoked without
//! evicting the people who joined through it. See `db::invites`.

use crate::channel_overrides::ChannelKind;
use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, bot, db, security};
use axum::extract::ws::{Message, WebSocket};
use chrono::Utc;
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::{error, info, warn};

/// Domain separator for the message a presence proof signs.
const PRESENCE_PROOF_PREFIX: &str = "presence:";

/// Verify that the presence frame proves ownership of its claimed public key:
/// an Ed25519 signature over `presence:<challenge>`, where the challenge is
/// the random value this connection was greeted with. Sends the matching
/// error to the client and returns `Err` on failure, or the proven key.
///
/// The challenge is what binds the proof to this server and this socket. One
/// identity key is the account on every server, so a proof over anything the
/// client picks itself — the old scheme signed a timestamp — could be lifted
/// by the operator of one server and replayed against another within its
/// freshness window.
async fn verify_key_proof(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    v: &Value,
    client_ip: &str,
    challenge: &str,
) -> Result<String, ()> {
    let (Some(pk), Some(sig)) = (
        v.get("publicKey").and_then(|p| p.as_str()),
        v.get("signature").and_then(|s| s.as_str()),
    ) else {
        send_error(sender, errors::INVALID_SIGNATURE).await;
        return Err(());
    };

    if !security::check_auth_rate_limit(&state.rate_limiter, client_ip).await {
        send_error(sender, errors::AUTH_RATE_LIMIT).await;
        return Err(());
    }

    let message = format!("{PRESENCE_PROOF_PREFIX}{challenge}");
    if let Err(err) = security::verify_key_signature(pk, sig, &message) {
        error!("Authentication failed - {err:?} for key: {pk}");
        let code = match err {
            security::ProofError::Encoding => errors::INVALID_ENCODING,
            security::ProofError::KeyLength => errors::INVALID_KEY_LENGTH,
            security::ProofError::PublicKey => errors::INVALID_PUBLIC_KEY,
            security::ProofError::SignatureFormat => errors::INVALID_SIGNATURE_FORMAT,
            security::ProofError::Signature => errors::INVALID_SIGNATURE,
        };
        send_error(sender, code).await;
        return Err(());
    }

    Ok(pk.to_string())
}

/// Which credential admitted a connection to a password-protected server.
enum Admission {
    /// The server password matched, or the key is already an invite member.
    /// Nothing further is owed.
    Granted,
    /// A currently redeemable invite code, to be spent once the account name
    /// and the ban list have also been checked — a rejected connection must
    /// not cost the invite one of its uses.
    PendingInvite(String),
}

/// Decide whether a presence frame may connect to a password-protected
/// server. Sends the matching error and returns `Err` when it may not.
async fn admit(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    v: &Value,
    required: &str,
    key: &str,
) -> Result<Admission, ()> {
    let provided = v.get("password").and_then(|p| p.as_str()).unwrap_or("");
    let password_ok = bool::from(provided.as_bytes().ct_eq(required.as_bytes()));

    if password_ok {
        return Ok(Admission::Granted);
    }

    match db::is_invite_member(&state.db, key).await {
        Ok(true) => return Ok(Admission::Granted),
        Ok(false) => {}
        Err(e) => {
            // Failing open here would hand out membership on a database
            // hiccup; a member can retry, an outsider cannot get in.
            error!("Failed to check invite membership: {e}");
            send_error(sender, errors::INVALID_PASSWORD).await;
            return Err(());
        }
    }

    let code = v
        .get("invite")
        .and_then(|i| i.as_str())
        .map(str::trim)
        .unwrap_or("");
    if code.is_empty() {
        send_error(sender, errors::INVALID_PASSWORD).await;
        return Err(());
    }

    match db::check_invite(&state.db, code, Utc::now()).await {
        Ok(db::Redemption::Granted) => Ok(Admission::PendingInvite(code.to_string())),
        Ok(reason) => {
            warn!("Rejected invite: {reason:?}");
            send_error(sender, errors::INVALID_INVITE).await;
            Err(())
        }
        Err(e) => {
            error!("Failed to check an invite code: {e}");
            send_error(sender, errors::INVALID_INVITE).await;
            Err(())
        }
    }
}

/// Send `user` the server-wide state that is otherwise kept current by
/// global broadcasts: members, roles, statuses, channels, voice occupancy and
/// the server settings. Every frame here is a whole snapshot the client
/// replaces its copy with, which is what lets the same set be re-sent to a
/// connection that fell behind the broadcast and lost some of those updates.
pub(super) async fn send_state_snapshot(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    u: &str,
) {
    send_role_definitions(state, sender).await;
    send_all_user_roles(state, sender).await;
    send_all_statuses(state, sender).await;
    super::profile::send_all_avatars(state, sender).await;
    super::profile::send_all_profiles(state, sender).await;
    send_categories(state, sender).await;
    send_channels(state, sender, Some(u)).await;
    send_emojis(state, sender).await;
    send_sounds(state, sender).await;
    send_voice_channels(state, sender, Some(u)).await;
    send_users(state, sender).await;
    send_all_voice(state, sender, u).await;
    super::screenshare::send_screenshare_config(state, sender).await;
    super::uploads::send_upload_config(state, sender).await;
    super::chat_settings::send_chat_settings(state, sender).await;
    super::voice_defaults::send_voice_defaults(state, sender).await;
}

/// Handle user presence (authentication) message.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_presence(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    v: &mut Value,
    authenticated: &mut bool,
    user_name: &mut Option<String>,
    client_ip: &str,
    challenge: &str,
    default_channel_id: i32,
) -> Result<(), ()> {
    // Every presence must prove a public key, even on an open server or a
    // repeated presence frame. A keyless one used to be admitted on an open
    // server as an unbound name: when the real owner later bound it, both
    // sockets ran as that name, and the keyless one acted with the roles the
    // owner was given while mute and ban, which resolve a key, could not
    // reach it. The proof also comes before the password is looked at, so a
    // protected server answers a right and a wrong password alike — any other
    // order is an oracle outside the authentication rate limit — and before
    // the credential check, since an invite membership and an invite code are
    // both bound to the key.
    let verified_key = verify_key_proof(sender, state, v, client_ip, challenge).await?;

    let mut pending_invite: Option<String> = None;
    if !*authenticated {
        match &state.password {
            None => *authenticated = true,
            Some(required) => {
                match admit(sender, state, v, required, &verified_key).await? {
                    Admission::Granted => {}
                    Admission::PendingInvite(code) => pending_invite = Some(code),
                }
                *authenticated = true;
            }
        }
    }

    if *authenticated {
        if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
            if !security::validate_user_name(u) {
                error!("Invalid user name: {}", u);
                send_error(sender, errors::INVALID_USERNAME).await;
                return Err(());
            }

            // A connection's identity is fixed once established. Switching
            // names mid-socket would leave the first name registered as
            // online, with its roles loaded, and nothing left to clean it up.
            if user_name.as_deref().is_some_and(|current| current != u) {
                error!("Rejected presence switching a connection to {u}");
                send_error(sender, errors::INVALID_USERNAME).await;
                return Err(());
            }

            // A user name stays permanently bound to the first verified
            // public key that used it (persisted in the database).
            // Reconnecting with the same key is fine, but any other key — or
            // no key at all — may not take the name over: roles attach to
            // names in memory, so a takeover would let the new connection
            // inherit the previous owner's privileges. The operator can
            // release a binding with the `unbind-name` CLI subcommand.
            // Bots are named in the same namespace and keyed by name in
            // memory just as accounts are, so a member must not take one's
            // name any more than another member's.
            //
            // Every check below fails closed: a database hiccup turns a
            // member away for one attempt, whereas failing open would admit a
            // banned user or hand a name to the wrong key.
            match bot::db::bot_name_taken(&state.db, u, None).await {
                Ok(false) => {}
                Ok(true) => {
                    error!("Rejected presence for {u}: name belongs to a bot");
                    send_error(sender, errors::USERNAME_TAKEN).await;
                    return Err(());
                }
                Err(e) => {
                    error!("Failed to check bot names for {u}: {e}");
                    send_error(sender, errors::LOGIN_FAILED).await;
                    return Err(());
                }
            }

            match db::get_user_key(&state.db, u).await {
                Ok(Some(bound)) if bound != verified_key => {
                    error!("Rejected presence for {u}: name is bound to another key");
                    send_error(sender, errors::USERNAME_TAKEN).await;
                    return Err(());
                }
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to check name binding for {u}: {e}");
                    send_error(sender, errors::LOGIN_FAILED).await;
                    return Err(());
                }
            }

            // Reject banned users before they are registered as present.
            match db::is_banned(&state.db, Some(&verified_key), u).await {
                Ok(true) => {
                    error!("Rejected banned user: {}", u);
                    send_error(sender, errors::BANNED).await;
                    return Err(());
                }
                Ok(false) => {}
                Err(e) => {
                    error!("Failed to check ban state for {u}: {e}");
                    send_error(sender, errors::LOGIN_FAILED).await;
                    return Err(());
                }
            }

            // The name and the ban list have now had their say, so an invite
            // that got this connection in can finally be spent. Doing it here
            // rather than at the credential check is what keeps a rejected
            // presence from costing the invite a use. It is re-validated
            // inside the same transaction that increments the counter, so two
            // clients racing for the last use cannot both be admitted.
            if let Some(code) = pending_invite.as_deref() {
                match db::redeem_invite(&state.db, code, &verified_key, Utc::now()).await {
                    Ok(db::Redemption::Granted) => info!(user = u, "Invite redeemed"),
                    Ok(reason) => {
                        warn!("Invite became unredeemable for {u}: {reason:?}");
                        send_error(sender, errors::INVALID_INVITE).await;
                        return Err(());
                    }
                    Err(e) => {
                        error!("Failed to redeem an invite for {u}: {e}");
                        send_error(sender, errors::INVALID_INVITE).await;
                        return Err(());
                    }
                }
            }

            // Claim the name for this key (no-op when already bound). A newly
            // created binding marks a first-time member, who receives the
            // configured welcome message below.
            let mut first_connection = false;
            let pk = verified_key.as_str();
            match db::bind_user_key(&state.db, u, pk).await {
                Ok(newly_bound) => first_connection = newly_bound,
                Err(e) => error!("Failed to persist name binding for {u}: {e}"),
            }
            // The binding check above ran before this insert, so two
            // keys racing for a fresh name both pass it and only one
            // insert wins. The loser must not go on as that name: it
            // would replace the owner's key in memory, which is what DMs
            // and channel keys are encrypted to.
            if !first_connection
                && !matches!(db::get_user_key(&state.db, u).await, Ok(Some(bound)) if bound == pk)
            {
                error!("Rejected presence for {u}: lost the race for the name");
                send_error(sender, errors::USERNAME_TAKEN).await;
                return Err(());
            }

            state.users.lock().await.insert(u.to_string());
            state.known_users.lock().await.insert(u.to_string());
            state
                .statuses
                .lock()
                .await
                .insert(u.to_string(), "online".to_string());

            broadcast_status(state, u, "online");
            broadcast_users(state).await;
            *user_name = Some(u.to_string());

            state
                .upload_sessions
                .lock()
                .await
                .insert(challenge.to_string(), (u.to_string(), pk.to_string()));
            state
                .user_keys
                .lock()
                .await
                .insert(u.to_string(), pk.to_string());

            // Load this key's role assignments from the database (the
            // source of truth) into memory and announce them. An empty set
            // also covers roles revoked while the user was offline.
            let role_ids = db::get_user_role_ids(&state.db, pk)
                .await
                .unwrap_or_default();
            state
                .user_roles
                .lock()
                .await
                .insert(u.to_string(), role_ids.clone());
            broadcast_user_roles(state, u, &role_ids);

            // Ahead of the channel list: the client reads a channel's
            // marker the moment it opens one, which the list triggers.
            super::read_markers::send_read_markers(state, sender, u).await;
            send_state_snapshot(state, sender, u).await;
            super::identity::send_server_identity(state, sender).await;
            if first_connection {
                super::identity::send_welcome(state, sender).await;
            }
            super::stats::send_stats_config(state, sender, u).await;
            super::scheduled::send_scheduled_messages(state, sender, u).await;
            super::scheduled::send_reminders(state, sender, u).await;
            super::send_ice_config(state, sender).await;
            send_default_channel(state, sender, u, default_channel_id).await;
        }
    } else {
        send_error(sender, errors::INVALID_SIGNATURE).await;
        return Err(());
    }

    Ok(())
}

/// Handle bot authentication via token.
pub(super) async fn handle_bot_presence(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    v: &Value,
    authenticated: &mut bool,
    user_name: &mut Option<String>,
    default_channel_id: i32,
) -> Result<(), ()> {
    let token = match v.get("token").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => {
            send_error(sender, errors::MISSING_BOT_TOKEN).await;
            return Err(());
        }
    };

    let hash = bot::models::hash_token(token);
    let record = match bot::db::get_bot_by_token_hash(&state.db, &hash).await {
        Ok(Some(b)) if b.active => b,
        _ => {
            send_error(sender, errors::INVALID_BOT_TOKEN).await;
            return Err(());
        }
    };

    if user_name
        .as_deref()
        .is_some_and(|current| current != record.name)
    {
        send_error(sender, errors::INVALID_BOT_TOKEN).await;
        return Err(());
    }

    // The bot API refuses a name an account already holds, but a bot created
    // before that check may still share one, and would connect as that
    // member's identity in memory — with their roles.
    if !matches!(db::get_user_key(&state.db, &record.name).await, Ok(None)) {
        error!(
            "Rejected bot {}: its name belongs to an account",
            record.name
        );
        send_error(sender, errors::USERNAME_TAKEN).await;
        return Err(());
    }

    *authenticated = true;
    let bot_name = record.name.clone();

    state.users.lock().await.insert(bot_name.clone());
    state.known_users.lock().await.insert(bot_name.clone());
    state
        .statuses
        .lock()
        .await
        .insert(bot_name.clone(), "online".to_string());

    broadcast_status(state, &bot_name, "online");
    broadcast_users(state).await;
    *user_name = Some(bot_name);

    send_role_definitions(state, sender).await;
    send_all_user_roles(state, sender).await;
    send_all_statuses(state, sender).await;
    super::profile::send_all_avatars(state, sender).await;
    super::profile::send_all_profiles(state, sender).await;
    send_channels(state, sender, user_name.as_deref()).await;
    send_emojis(state, sender).await;
    send_sounds(state, sender).await;
    send_voice_channels(state, sender, user_name.as_deref()).await;
    send_users(state, sender).await;
    send_all_voice(state, sender, &record.name).await;
    super::identity::send_server_identity(state, sender).await;
    send_default_channel(state, sender, &record.name, default_channel_id).await;

    Ok(())
}

/// Send the default channel's history and wiki index, which a connection gets
/// without an explicit join. Gated like a join: the default channel can be
/// made private like any other.
async fn send_default_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
    default_channel_id: i32,
) {
    if !can_view_channel(state, user, ChannelKind::Text, default_channel_id).await {
        return;
    }
    db::send_history(
        &state.db,
        sender,
        default_channel_id,
        None,
        DEFAULT_HISTORY_LIMIT,
    )
    .await;
    super::wiki::send_wiki_index(state, sender, default_channel_id).await;
}
