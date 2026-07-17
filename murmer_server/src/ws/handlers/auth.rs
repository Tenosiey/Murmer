//! Authentication handlers for user and bot presence.

use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, bot, db, roles::RoleInfo, security};
use axum::extract::ws::{Message, WebSocket};
use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::error;

/// Verify that the presence frame proves ownership of its claimed public key:
/// the timestamp must be fresh and unused (replay protection) and the Ed25519
/// signature over it must verify against the key. Sends the matching error to
/// the client and returns `Err` on failure.
async fn verify_key_proof(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    v: &Value,
    client_ip: &str,
) -> Result<(), ()> {
    let (Some(pk), Some(sig), Some(ts)) = (
        v.get("publicKey").and_then(|p| p.as_str()),
        v.get("signature").and_then(|s| s.as_str()),
        v.get("timestamp").and_then(|t| t.as_str()),
    ) else {
        send_error(sender, errors::INVALID_SIGNATURE).await;
        return Err(());
    };

    if !security::check_auth_rate_limit(&state.rate_limiter, client_ip).await {
        send_error(sender, errors::AUTH_RATE_LIMIT).await;
        return Err(());
    }

    let timestamp = match security::validate_timestamp(ts) {
        Ok(ts) => ts,
        Err(err) => {
            error!("Authentication failed - {}: {}", err, ts);
            send_error(sender, errors::INVALID_TIMESTAMP).await;
            return Err(());
        }
    };

    let nonce = format!("{}:{}", pk, timestamp);
    if !security::check_and_store_nonce(&state.rate_limiter, &nonce).await {
        send_error(sender, errors::REPLAY_ATTACK).await;
        return Err(());
    }

    let (Ok(pk_bytes), Ok(sig_bytes)) = (
        general_purpose::STANDARD.decode(pk),
        general_purpose::STANDARD.decode(sig),
    ) else {
        error!("Authentication failed - invalid base64 encoding");
        send_error(sender, errors::INVALID_ENCODING).await;
        return Err(());
    };

    let Ok(pk_array) = pk_bytes.as_slice().try_into() else {
        error!(
            "Authentication failed - public key wrong length: {}",
            pk_bytes.len()
        );
        send_error(sender, errors::INVALID_KEY_LENGTH).await;
        return Err(());
    };

    let key = match VerifyingKey::from_bytes(&pk_array) {
        Ok(key) => key,
        Err(e) => {
            error!("Authentication failed - invalid public key: {}", e);
            send_error(sender, errors::INVALID_PUBLIC_KEY).await;
            return Err(());
        }
    };

    let signature = match Signature::from_slice(&sig_bytes) {
        Ok(signature) => signature,
        Err(e) => {
            error!("Authentication failed - invalid signature format: {}", e);
            send_error(sender, errors::INVALID_SIGNATURE_FORMAT).await;
            return Err(());
        }
    };

    if key.verify(ts.as_bytes(), &signature).is_err() {
        error!(
            "Authentication failed - signature verification failed for key: {}",
            pk
        );
        send_error(sender, errors::INVALID_SIGNATURE).await;
        return Err(());
    }

    Ok(())
}

/// Handle user presence (authentication) message.
pub(super) async fn handle_presence(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    v: &mut Value,
    authenticated: &mut bool,
    user_name: &mut Option<String>,
    client_ip: &str,
    default_channel_id: i32,
) -> Result<(), ()> {
    if !*authenticated && let Some(required) = &state.password {
        let provided = v.get("password").and_then(|p| p.as_str()).unwrap_or("");
        if !bool::from(provided.as_bytes().ct_eq(required.as_bytes())) {
            send_error(sender, errors::INVALID_PASSWORD).await;
            return Err(());
        }
    }

    // A claimed public key must always be proven, even on an open server or a
    // repeated presence frame: roles, bans and moderation identity attach to
    // the key, so accepting it unverified would allow impersonation.
    let verified_key = if v.get("publicKey").is_some() {
        verify_key_proof(sender, state, v, client_ip).await?;
        *authenticated = true;
        v.get("publicKey")
            .and_then(|p| p.as_str())
            .map(str::to_string)
    } else {
        // Without a key there is nothing to verify; servers without a
        // password accept the connection as an anonymous (role-less) user.
        if state.password.is_none() {
            *authenticated = true;
        }
        None
    };

    if *authenticated {
        if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
            if !security::validate_user_name(u) {
                error!("Invalid user name: {}", u);
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
            match db::get_user_key(&state.db, u).await {
                Ok(Some(bound)) if verified_key.as_deref() != Some(bound.as_str()) => {
                    error!("Rejected presence for {u}: name is bound to another key");
                    send_error(sender, errors::USERNAME_TAKEN).await;
                    return Err(());
                }
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to check name binding for {u}: {e}");
                }
            }

            // Reject banned users before they are registered as present.
            match db::is_banned(&state.db, verified_key.as_deref(), u).await {
                Ok(true) => {
                    error!("Rejected banned user: {}", u);
                    send_error(sender, errors::BANNED).await;
                    return Err(());
                }
                Ok(false) => {}
                Err(e) => {
                    error!("Failed to check ban state for {u}: {e}");
                }
            }

            // Claim the name for this key (no-op when already bound). A newly
            // created binding marks a first-time member, who receives the
            // configured welcome message below. Anonymous connections (no
            // key) have no persistent identity, so they never trigger it.
            let mut first_connection = false;
            if let Some(pk) = verified_key.as_deref() {
                match db::bind_user_key(&state.db, u, pk).await {
                    Ok(newly_bound) => first_connection = newly_bound,
                    Err(e) => error!("Failed to persist name binding for {u}: {e}"),
                }
            }

            state.users.lock().await.insert(u.to_string());
            state.known_users.lock().await.insert(u.to_string());
            state
                .statuses
                .lock()
                .await
                .insert(u.to_string(), "online".to_string());

            broadcast_status(state, u, "online").await;
            broadcast_users(state).await;
            *user_name = Some(u.to_string());

            if let Some(pk) = verified_key.as_deref() {
                state
                    .user_keys
                    .lock()
                    .await
                    .insert(u.to_string(), pk.to_string());

                if let Some((role, color)) = db::get_role(&state.db, pk).await {
                    let info = RoleInfo {
                        role: role.clone(),
                        color: color.or_else(|| crate::roles::default_color(&role)),
                    };
                    state.roles.lock().await.insert(u.to_string(), info.clone());
                    broadcast_role(state, u, &info.role, info.color.as_deref()).await;
                } else if state.roles.lock().await.remove(u).is_some() {
                    // The role was revoked (e.g. via the CLI) while the user
                    // was offline; drop the stale in-memory entry so it does
                    // not keep granting privileges.
                    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
                        "type": "role-remove",
                        "user": u,
                    })) {
                        let _ = state.tx.send(msg);
                    }
                }
            }

            send_all_roles(state, sender).await;
            send_all_statuses(state, sender).await;
            super::profile::send_all_avatars(state, sender).await;
            send_categories(state, sender).await;
            send_channels(state, sender).await;
            send_emojis(state, sender).await;
            send_voice_channels(state, sender).await;
            send_users(state, sender).await;
            send_all_voice(state, sender).await;
            super::identity::send_server_identity(state, sender).await;
            if first_connection {
                super::identity::send_welcome(state, sender).await;
            }
            super::stats::send_stats_config(state, sender, u).await;
            super::screenshare::send_screenshare_config(state, sender).await;
            db::send_history(
                &state.db,
                sender,
                default_channel_id,
                None,
                DEFAULT_HISTORY_LIMIT,
            )
            .await;
            // The connection starts in the default channel without an
            // explicit join, so its wiki snapshot has to be sent here.
            super::wiki::send_wiki_index(state, sender, default_channel_id).await;
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

    *authenticated = true;
    let bot_name = record.name.clone();

    state.users.lock().await.insert(bot_name.clone());
    state.known_users.lock().await.insert(bot_name.clone());
    state
        .statuses
        .lock()
        .await
        .insert(bot_name.clone(), "online".to_string());

    broadcast_status(state, &bot_name, "online").await;
    broadcast_users(state).await;
    *user_name = Some(bot_name);

    send_all_roles(state, sender).await;
    send_all_statuses(state, sender).await;
    super::profile::send_all_avatars(state, sender).await;
    send_channels(state, sender).await;
    send_emojis(state, sender).await;
    send_voice_channels(state, sender).await;
    send_users(state, sender).await;
    send_all_voice(state, sender).await;
    super::identity::send_server_identity(state, sender).await;
    db::send_history(
        &state.db,
        sender,
        default_channel_id,
        None,
        DEFAULT_HISTORY_LIMIT,
    )
    .await;
    super::wiki::send_wiki_index(state, sender, default_channel_id).await;

    Ok(())
}
