//! Authentication handlers for user and bot presence.

use crate::ws::{constants::*, errors, helpers::*};
use crate::{bot, db, roles::RoleInfo, security, AppState};
use axum::extract::ws::{Message, WebSocket};
use base64::{engine::general_purpose, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures::{stream::SplitSink, SinkExt};
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

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
    if !*authenticated {
        if let Some(required) = &state.password {
            let provided = v.get("password").and_then(|p| p.as_str());
            if provided != Some(required) {
                let _ = sender
                    .send(Message::Text(errors::INVALID_PASSWORD.to_string()))
                    .await;
                return Err(());
            }
        }

        if let (Some(pk), Some(sig), Some(ts)) = (
            v.get("publicKey").and_then(|p| p.as_str()),
            v.get("signature").and_then(|s| s.as_str()),
            v.get("timestamp").and_then(|t| t.as_str()),
        ) {
            if !security::check_auth_rate_limit(&state.rate_limiter, client_ip).await {
                let _ = sender
                    .send(Message::Text(errors::AUTH_RATE_LIMIT.to_string()))
                    .await;
                return Err(());
            }

            let timestamp = match security::validate_timestamp(ts) {
                Ok(ts) => ts,
                Err(err) => {
                    error!("Authentication failed - {}: {}", err, ts);
                    let _ = sender
                        .send(Message::Text(errors::INVALID_TIMESTAMP.to_string()))
                        .await;
                    return Err(());
                }
            };

            let nonce = format!("{}:{}", pk, timestamp);
            if !security::check_and_store_nonce(&state.rate_limiter, &nonce).await {
                let _ = sender
                    .send(Message::Text(errors::REPLAY_ATTACK.to_string()))
                    .await;
                return Err(());
            }

            if let (Ok(pk_bytes), Ok(sig_bytes)) = (
                general_purpose::STANDARD.decode(pk),
                general_purpose::STANDARD.decode(sig),
            ) {
                if let Ok(pk_array) = pk_bytes.as_slice().try_into() {
                    match VerifyingKey::from_bytes(&pk_array) {
                        Ok(key) => match Signature::from_slice(&sig_bytes) {
                            Ok(signature) => {
                                if key.verify(ts.as_bytes(), &signature).is_ok() {
                                    *authenticated = true;
                                } else {
                                    error!(
                                        "Authentication failed - signature verification failed for key: {}",
                                        pk
                                    );
                                    let _ = sender
                                        .send(Message::Text(errors::INVALID_SIGNATURE.to_string()))
                                        .await;
                                    return Err(());
                                }
                            }
                            Err(e) => {
                                error!("Authentication failed - invalid signature format: {}", e);
                                let _ = sender
                                    .send(Message::Text(
                                        errors::INVALID_SIGNATURE_FORMAT.to_string(),
                                    ))
                                    .await;
                                return Err(());
                            }
                        },
                        Err(e) => {
                            error!("Authentication failed - invalid public key: {}", e);
                            let _ = sender
                                .send(Message::Text(errors::INVALID_PUBLIC_KEY.to_string()))
                                .await;
                            return Err(());
                        }
                    }
                } else {
                    error!(
                        "Authentication failed - public key wrong length: {}",
                        pk_bytes.len()
                    );
                    let _ = sender
                        .send(Message::Text(errors::INVALID_KEY_LENGTH.to_string()))
                        .await;
                    return Err(());
                }
            } else {
                error!("Authentication failed - invalid base64 encoding");
                let _ = sender
                    .send(Message::Text(errors::INVALID_ENCODING.to_string()))
                    .await;
                return Err(());
            }
        }
    }

    if *authenticated {
        if let Some(u) = v.get("user").and_then(|u| u.as_str()) {
            if !security::validate_user_name(u) {
                error!("Invalid user name: {}", u);
                let _ = sender
                    .send(Message::Text(errors::INVALID_USERNAME.to_string()))
                    .await;
                return Err(());
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

            if let Some(pk) = v.get("publicKey").and_then(|p| p.as_str()) {
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
                }
            }

            send_all_roles(state, sender).await;
            send_all_statuses(state, sender).await;
            send_categories(state, sender).await;
            send_channels(state, sender).await;
            send_voice_channels(state, sender).await;
            send_users(state, sender).await;
            send_all_voice(state, sender).await;
            db::send_history(
                &state.db,
                sender,
                default_channel_id,
                None,
                DEFAULT_HISTORY_LIMIT,
            )
            .await;
        }
    } else {
        let _ = sender
            .send(Message::Text(errors::INVALID_SIGNATURE.to_string()))
            .await;
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
            let _ = sender
                .send(Message::Text(
                    r#"{"type":"error","message":"missing-bot-token"}"#.to_string(),
                ))
                .await;
            return Err(());
        }
    };

    let hash = bot::models::hash_token(token);
    let record = match bot::db::get_bot_by_token_hash(&state.db, &hash).await {
        Ok(Some(b)) if b.active => b,
        _ => {
            let _ = sender
                .send(Message::Text(
                    r#"{"type":"error","message":"invalid-bot-token"}"#.to_string(),
                ))
                .await;
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
    send_channels(state, sender).await;
    send_voice_channels(state, sender).await;
    send_users(state, sender).await;
    send_all_voice(state, sender).await;
    db::send_history(
        &state.db,
        sender,
        default_channel_id,
        None,
        DEFAULT_HISTORY_LIMIT,
    )
    .await;

    Ok(())
}
