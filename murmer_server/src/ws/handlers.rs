//! WebSocket message handlers.

use super::{constants::*, errors, helpers::*, validation::*};
use crate::{bot, db, roles::RoleInfo, security, AppState, VoiceChannelState};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    response::IntoResponse,
};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument};

/// Resolve the "general" channel ID from the database.
async fn general_channel_id(state: &Arc<AppState>) -> i32 {
    db::get_channel_id_by_name(&state.db, "general")
        .await
        .unwrap_or(1)
}

/// Main WebSocket loop handling incoming messages and broadcasting events.
#[tracing::instrument(skip(socket, state), fields(client_ip = %peer_addr.ip()))]
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, peer_addr: SocketAddr) {
    let client_ip = peer_addr.ip().to_string();
    info!("Client connected");

    let (mut sender, mut receiver) = socket.split();
    let mut global_rx = state.tx.subscribe();
    let default_channel_id = general_channel_id(&state).await;
    let mut channel_id: i32 = default_channel_id;
    let mut chan_tx = get_or_create_channel(&state, channel_id).await;
    let mut chan_rx = chan_tx.subscribe();
    let mut user_name: Option<String> = None;
    let mut voice_channel: Option<i32> = None;
    let mut authenticated = state.password.is_none();

    loop {
        tokio::select! {
            Some(result) = receiver.next() => {
                let text = match result {
                    Ok(Message::Text(t)) => t,
                    _ => break,
                };

                if let Ok(mut v) = serde_json::from_str::<Value>(&text) {
                    if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
                        if t.starts_with("voice-") {
                            debug!("Received voice message: {t}");
                        } else {
                            info!("Received message type: {t}");
                        }

                        if !authenticated && t != "presence" && t != "bot-presence" {
                            let _ = sender.send(Message::Text(errors::UNAUTHENTICATED.to_string())).await;
                            break;
                        }

                        match t {
                            "presence" => {
                                if handle_presence(&mut sender, &state, &mut v, &mut authenticated, &mut user_name, &client_ip, default_channel_id).await.is_err() {
                                    break;
                                }
                            }
                            "bot-presence" => {
                                if handle_bot_presence(&mut sender, &state, &v, &mut authenticated, &mut user_name, default_channel_id).await.is_err() {
                                    break;
                                }
                            }
                            "join" => {
                                handle_join(&state, &mut sender, &v, &mut channel_id, &mut chan_tx, &mut chan_rx).await;
                            }
                            "load-history" => {
                                handle_load_history(&state, &mut sender, &v, channel_id).await;
                            }
                            "search-history" => {
                                handle_search_history(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "create-channel" => {
                                handle_create_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-channel" => {
                                if handle_delete_channel(&state, &mut sender, &v, &user_name, &mut channel_id, &mut chan_tx, &mut chan_rx, default_channel_id).await.is_err() {
                                    continue;
                                }
                            }
                            "move-channel" => {
                                handle_move_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "create-category" => {
                                handle_create_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "rename-category" => {
                                handle_rename_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-category" => {
                                handle_delete_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "create-voice-channel" => {
                                handle_create_voice_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "update-voice-channel" => {
                                handle_update_voice_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-voice-channel" => {
                                handle_delete_voice_channel(&state, &mut sender, &v, &user_name, &mut voice_channel).await;
                            }
                            "chat" => {
                                handle_chat(&state, &mut sender, &mut v, channel_id, &user_name).await;
                            }
                            "delete-message" => {
                                handle_delete_message(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "react" => {
                                handle_react(&state, &mut sender, &v, &user_name).await;
                            }
                            "status-update" => {
                                handle_status_update(&state, &mut sender, &v, &user_name).await;
                            }
                            "ping" => {
                                handle_ping(&mut sender, &v).await;
                            }
                            "voice-join" => {
                                handle_voice_join(&state, &v, &mut voice_channel, &user_name).await;
                            }
                            "voice-leave" => {
                                handle_voice_leave(&state, &v, &mut voice_channel, &user_name).await;
                            }
                            "voice-offer" | "voice-answer" | "voice-candidate" => {
                                let _ = state.tx.send(text.to_string());
                            }
                            "screenshare-start" => {
                                let _ = state.tx.send(text.to_string());
                            }
                            "screenshare-stop" => {
                                let _ = state.tx.send(text.to_string());
                            }
                            "screenshare-offer" | "screenshare-answer" | "screenshare-candidate" => {
                                let _ = state.tx.send(text.to_string());
                            }
                            "set-role" => {
                                handle_set_role(&state, &mut sender, &v, &user_name).await;
                            }
                            "remove-role" => {
                                handle_remove_role(&state, &mut sender, &v, &user_name).await;
                            }
                            _ => {
                                error!("unknown message type: {t}");
                            }
                        }
                    }
                } else {
                    error!("invalid json message: {text}");
                }
            }
            result = chan_rx.recv() => {
                match result {
                    Ok(msg) => {
                        if sender.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
            result = global_rx.recv() => {
                match result {
                    Ok(msg) => {
                        if sender.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }

    handle_disconnect(&state, user_name).await;
    info!(%client_ip, "Client disconnected");
}

/// Handle user presence (authentication) message.
async fn handle_presence(
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
            db::send_history(&state.db, sender, default_channel_id, None, DEFAULT_HISTORY_LIMIT)
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
async fn handle_bot_presence(
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

/// Handle channel join message.
async fn handle_join(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: &mut i32,
    chan_tx: &mut broadcast::Sender<String>,
    chan_rx: &mut broadcast::Receiver<String>,
) {
    if let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()) {
        *channel_id = ch_id as i32;
        *chan_tx = get_or_create_channel(state, *channel_id).await;
        *chan_rx = chan_tx.subscribe();
        db::send_history(&state.db, sender, *channel_id, None, DEFAULT_HISTORY_LIMIT).await;
    }
}

/// Handle history loading request.
async fn handle_load_history(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: i32,
) {
    let before = v.get("before").and_then(|b| b.as_i64());
    let mut limit = v
        .get("limit")
        .and_then(|l| l.as_i64())
        .unwrap_or(DEFAULT_HISTORY_LIMIT);

    if limit > MAX_HISTORY_LIMIT {
        limit = MAX_HISTORY_LIMIT;
        tracing::warn!(
            "History request limit capped at {} for request",
            MAX_HISTORY_LIMIT
        );
    }

    db::send_history(&state.db, sender, channel_id, before, limit).await;
}

/// Handle search history request.
async fn handle_search_history(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: i32,
    user_name: &Option<String>,
) {
    let request_id = v.get("requestId").cloned().unwrap_or(Value::Null);
    let request_id_for_error = request_id.clone();

    let Some(raw_query) = v.get("query").and_then(|q| q.as_str()) else {
        let payload = serde_json::json!({
            "type": "search-error",
            "message": "missing-query",
            "requestId": request_id_for_error,
        });
        let _ = sender.send(Message::Text(payload.to_string())).await;
        return;
    };

    let trimmed_query = raw_query.trim();
    if trimmed_query.is_empty() {
        let payload = serde_json::json!({
            "type": "search-results",
            "requestId": request_id,
            "channelId": channel_id,
            "messages": [],
        });
        let _ = sender.send(Message::Text(payload.to_string())).await;
        return;
    }

    let channel_to_search = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
        .unwrap_or(channel_id);

    let mut limit = v
        .get("limit")
        .and_then(|l| l.as_i64())
        .unwrap_or(DEFAULT_HISTORY_LIMIT);
    limit = limit.clamp(1, MAX_SEARCH_RESULTS);

    match db::search_messages(&state.db, channel_to_search, trimmed_query, limit).await {
        Ok(rows) => {
            let mut ids = Vec::new();
            for (id, _) in &rows {
                if let Ok(value) = i32::try_from(*id) {
                    ids.push(value);
                }
            }

            let reaction_map = if ids.is_empty() {
                std::collections::HashMap::new()
            } else {
                match db::get_reactions_for_messages(&state.db, &ids).await {
                    Ok(map) => map,
                    Err(error) => {
                        error!(
                            "Failed to load reactions for search results in channel {channel_to_search}: {error}"
                        );
                        std::collections::HashMap::new()
                    }
                }
            };

            let mut messages = Vec::new();
            for (id, content) in rows {
                if let Ok(mut value) = serde_json::from_str::<Value>(&content) {
                    value["id"] = Value::from(id);
                    if value.get("channelId").is_none() {
                        value["channelId"] = Value::from(channel_to_search);
                    }
                    #[allow(clippy::collapsible_if)]
                    if let Ok(id32) = i32::try_from(id) {
                        if let Some(reactions) = reaction_map.get(&id32) {
                            if let Ok(reaction_value) = serde_json::to_value(reactions) {
                                value["reactions"] = reaction_value;
                            }
                        }
                    }
                    if value.get("reactions").is_none() {
                        value["reactions"] = Value::Object(Map::new());
                    }
                    messages.push(value);
                }
            }

            let payload = serde_json::json!({
                "type": "search-results",
                "requestId": request_id,
                "channelId": channel_to_search,
                "messages": messages,
            });
            let _ = sender.send(Message::Text(payload.to_string())).await;
        }
        Err(error) => {
            error!(
                "Search query failed for channel {channel_to_search} and user {:?}: {error}",
                user_name
            );
            let payload = serde_json::json!({
                "type": "search-error",
                "message": "Search failed",
                "requestId": request_id_for_error,
            });
            let _ = sender.send(Message::Text(payload.to_string())).await;
        }
    }
}

/// Handle create channel request.
async fn handle_create_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch) = v.get("name").and_then(|c| c.as_str()) else {
        return;
    };

    if !security::validate_channel_name(ch) {
        error!("Invalid channel name: {}", ch);
        let _ = sender
            .send(Message::Text(errors::INVALID_CHANNEL_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("create-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to create channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let category_id = v
        .get("categoryId")
        .and_then(|c| c.as_i64())
        .map(|id| id as i32);

    match db::add_channel(&state.db, ch, category_id).await {
        Ok(Some(record)) => {
            get_or_create_channel(state, record.id).await;
            broadcast_new_channel(state, record.id, &record.name, record.category_id).await;
        }
        Ok(None) => {
            // Channel name already exists
        }
        Err(e) => {
            error!("db add channel error: {e}");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_CREATION_FAILED.to_string()))
                .await;
        }
    }
}

/// Handle delete channel request.
async fn handle_delete_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
    channel_id: &mut i32,
    chan_tx: &mut broadcast::Sender<String>,
    chan_rx: &mut broadcast::Receiver<String>,
    default_channel_id: i32,
) -> Result<(), ()> {
    let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()).map(|c| c as i32) else {
        return Ok(());
    };

    // Look up the channel to check its name
    let record = match db::get_channel_by_id(&state.db, ch_id).await {
        Some(r) => r,
        None => return Ok(()),
    };

    if record.name == "general" {
        let _ = sender
            .send(Message::Text(errors::CANNOT_DELETE_GENERAL.to_string()))
            .await;
        return Err(());
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("delete-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return Err(());
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to delete channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return Err(());
    }

    if let Err(e) = db::remove_channel(&state.db, ch_id).await {
        error!("db remove channel error: {e}");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_DELETION_FAILED.to_string()))
            .await;
    } else {
        state.channels.lock().await.remove(&ch_id);
        broadcast_remove_channel(state, ch_id).await;
        if *channel_id == ch_id {
            *channel_id = default_channel_id;
            *chan_tx = get_or_create_channel(state, *channel_id).await;
            *chan_rx = chan_tx.subscribe();
        }
    }

    Ok(())
}

/// Handle create voice channel request.
async fn handle_create_voice_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch) = v.get("name").and_then(|c| c.as_str()) else {
        return;
    };

    if !security::validate_channel_name(ch) {
        error!("Invalid voice channel name: {}", ch);
        let _ = sender
            .send(Message::Text(errors::INVALID_CHANNEL_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("create-voice-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to create voice channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let quality_value = v
        .get("quality")
        .and_then(|q| q.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_VOICE_QUALITY.to_string());
    if !validate_voice_quality(&quality_value) {
        let _ = sender
            .send(Message::Text(errors::INVALID_VOICE_QUALITY.to_string()))
            .await;
        return;
    }

    let bitrate_value = match v.get("bitrate") {
        Some(val) if val.is_null() => None,
        Some(val) => match val.as_i64().and_then(validate_bitrate) {
            Some(valid) => Some(valid),
            None => {
                let _ = sender
                    .send(Message::Text(errors::INVALID_VOICE_BITRATE.to_string()))
                    .await;
                return;
            }
        },
        None => Some(DEFAULT_VOICE_BITRATE),
    };

    let category_id = v
        .get("categoryId")
        .and_then(|c| c.as_i64())
        .map(|id| id as i32);

    match db::add_voice_channel(&state.db, ch, &quality_value, bitrate_value, category_id).await {
        Ok(Some(record)) => {
            let info = VoiceChannelState {
                name: record.name.clone(),
                users: HashSet::new(),
                quality: record.quality.clone(),
                bitrate: record.bitrate,
                category_id: record.category_id,
            };
            state
                .voice_channels
                .lock()
                .await
                .insert(record.id, info.clone());
            broadcast_new_voice_channel(state, record.id, &info).await;
        }
        Ok(None) => {
            // Name already exists
        }
        Err(e) => {
            error!("db add voice channel error: {e}");
        }
    }
}

/// Handle update voice channel request.
async fn handle_update_voice_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()).map(|c| c as i32) else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("update-voice-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to update voice channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let quality_override = if let Some(raw) = v.get("quality").and_then(|q| q.as_str()) {
        let trimmed = raw.trim();
        if !validate_voice_quality(trimmed) {
            let _ = sender
                .send(Message::Text(errors::INVALID_VOICE_QUALITY.to_string()))
                .await;
            return;
        }
        Some(trimmed.to_string())
    } else {
        None
    };

    let bitrate_override = if let Some(val) = v.get("bitrate") {
        if val.is_null() {
            Some(None)
        } else {
            match val.as_i64().and_then(validate_bitrate) {
                Some(valid) => Some(Some(valid)),
                None => {
                    let _ = sender
                        .send(Message::Text(errors::INVALID_VOICE_BITRATE.to_string()))
                        .await;
                    return;
                }
            }
        }
    } else {
        None
    };

    let current = state.voice_channels.lock().await;
    let Some(existing) = current.get(&ch_id).cloned() else {
        let _ = sender
            .send(Message::Text(errors::UNKNOWN_VOICE_CHANNEL.to_string()))
            .await;
        return;
    };
    drop(current);

    let next_quality = quality_override.unwrap_or_else(|| existing.quality.clone());
    let next_bitrate = match bitrate_override {
        Some(value) => value,
        None => existing.bitrate,
    };

    match db::update_voice_channel(&state.db, ch_id, &next_quality, next_bitrate).await {
        Ok(true) => {
            let mut map = state.voice_channels.lock().await;
            if let Some(entry) = map.get_mut(&ch_id) {
                entry.quality = next_quality.clone();
                entry.bitrate = next_bitrate;
                let snapshot = entry.clone();
                drop(map);
                broadcast_voice_channel_update(state, ch_id, &snapshot).await;
            }
        }
        Ok(false) => {
            let _ = sender
                .send(Message::Text(errors::UNKNOWN_VOICE_CHANNEL.to_string()))
                .await;
        }
        Err(e) => {
            error!("Failed to update voice channel {ch_id}: {e}");
            let _ = sender
                .send(Message::Text(
                    errors::VOICE_CHANNEL_UPDATE_FAILED.to_string(),
                ))
                .await;
        }
    }
}

/// Handle delete voice channel request.
async fn handle_delete_voice_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
    voice_channel: &mut Option<i32>,
) {
    let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()).map(|c| c as i32) else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            error!("delete-voice-channel requested before presence was fully processed");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        error!("User {requester} attempted to delete voice channel without permission");
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    state.voice_channels.lock().await.remove(&ch_id);
    let _ = db::remove_voice_channel(&state.db, ch_id).await;
    broadcast_remove_voice_channel(state, ch_id).await;
    if *voice_channel == Some(ch_id) {
        *voice_channel = None;
    }
}

/// Handle create category request.
async fn handle_create_category(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(name) = v.get("name").and_then(|n| n.as_str()) else {
        return;
    };

    if !security::validate_channel_name(name) {
        let _ = sender
            .send(Message::Text(errors::INVALID_CATEGORY_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let position = v
        .get("position")
        .and_then(|p| p.as_i64())
        .map(|p| p as i32)
        .unwrap_or(0);

    match db::add_category(&state.db, name, position).await {
        Ok(id) => {
            broadcast_new_category(state, id, name, position).await;
        }
        Err(e) => {
            error!("db add category error: {e}");
            let _ = sender
                .send(Message::Text(errors::CATEGORY_CREATION_FAILED.to_string()))
                .await;
        }
    }
}

/// Handle rename category request.
async fn handle_rename_category(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(id) = v.get("id").and_then(|i| i.as_i64()).map(|i| i as i32) else {
        return;
    };
    let Some(name) = v.get("name").and_then(|n| n.as_str()) else {
        return;
    };

    if !security::validate_channel_name(name) {
        let _ = sender
            .send(Message::Text(errors::INVALID_CATEGORY_NAME.to_string()))
            .await;
        return;
    }

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    match db::rename_category(&state.db, id, name).await {
        Ok(true) => {
            broadcast_rename_category(state, id, name).await;
        }
        Ok(false) => {
            let _ = sender
                .send(Message::Text(errors::UNKNOWN_CATEGORY.to_string()))
                .await;
        }
        Err(e) => {
            error!("db rename category error: {e}");
            let _ = sender
                .send(Message::Text(errors::CATEGORY_RENAME_FAILED.to_string()))
                .await;
        }
    }
}

/// Handle delete category request.
async fn handle_delete_category(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(id) = v.get("id").and_then(|i| i.as_i64()).map(|i| i as i32) else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    match db::remove_category(&state.db, id).await {
        Ok(true) => {
            broadcast_remove_category(state, id).await;
        }
        Ok(false) => {
            let _ = sender
                .send(Message::Text(errors::UNKNOWN_CATEGORY.to_string()))
                .await;
        }
        Err(e) => {
            error!("db delete category error: {e}");
            let _ = sender
                .send(Message::Text(errors::CATEGORY_DELETION_FAILED.to_string()))
                .await;
        }
    }
}

/// Handle move channel to a category (or remove from category).
async fn handle_move_channel(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()).map(|c| c as i32) else {
        return;
    };

    let requester = match user_name.as_deref() {
        Some(n) => n,
        None => {
            let _ = sender
                .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_channels(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::CHANNEL_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let category_id = if v.get("categoryId").map_or(false, |c| c.is_null()) {
        None
    } else {
        v.get("categoryId")
            .and_then(|c| c.as_i64())
            .map(|i| i as i32)
    };

    let is_voice = v
        .get("voice")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let result = if is_voice {
        match db::move_voice_channel(&state.db, ch_id, category_id).await {
            Ok(updated) => {
                if updated {
                    let mut map = state.voice_channels.lock().await;
                    if let Some(entry) = map.get_mut(&ch_id) {
                        entry.category_id = category_id;
                    }
                }
                Ok(updated)
            }
            Err(e) => Err(e),
        }
    } else {
        db::move_channel(&state.db, ch_id, category_id).await
    };

    match result {
        Ok(true) => {
            broadcast_channel_move(state, ch_id, category_id, is_voice).await;
        }
        Ok(false) => {
            let _ = sender
                .send(Message::Text(errors::CHANNEL_MOVE_FAILED.to_string()))
                .await;
        }
        Err(e) => {
            error!("db move channel error: {e}");
            let _ = sender
                .send(Message::Text(errors::CHANNEL_MOVE_FAILED.to_string()))
                .await;
        }
    }
}

/// Handle chat message.
#[tracing::instrument(skip(state, sender, v), fields(channel_id = %channel_id, user = ?user_name))]
async fn handle_chat(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &mut Value,
    channel_id: i32,
    user_name: &Option<String>,
) {
    let user = match user_name {
        Some(u) => u,
        None => return,
    };

    if !security::check_message_rate_limit(&state.rate_limiter, user).await {
        let _ = sender
            .send(Message::Text(errors::MESSAGE_RATE_LIMIT.to_string()))
            .await;
        return;
    }

    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        if text.len() > MAX_MESSAGE_LENGTH {
            let _ = sender
                .send(Message::Text(errors::MESSAGE_TOO_LONG.to_string()))
                .await;
            return;
        }
    }

    v["user"] = Value::String(user.clone());
    v["channelId"] = Value::from(channel_id);
    if let Some(map) = v.as_object_mut() {
        map.remove("channel");
    }
    let timestamp = sanitize_message_timestamp(v);

    let mut ephemeral_expiry: Option<DateTime<Utc>> = None;
    if let Some(raw_expiry) = v.get("expiresAt").and_then(|value| value.as_str()) {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(raw_expiry) {
            let mut expiry = parsed.with_timezone(&Utc);
            let min_allowed = timestamp + ChronoDuration::seconds(MIN_EPHEMERAL_SECONDS);
            let max_allowed = timestamp + ChronoDuration::seconds(MAX_EPHEMERAL_SECONDS);
            if expiry < min_allowed {
                expiry = min_allowed;
            }
            if expiry > max_allowed {
                expiry = max_allowed;
            }
            v["expiresAt"] = Value::String(expiry.to_rfc3339());
            v["ephemeral"] = Value::Bool(true);
            ephemeral_expiry = Some(expiry);
        } else if let Some(map) = v.as_object_mut() {
            map.remove("expiresAt");
            map.remove("ephemeral");
        }
    } else if let Some(map) = v.as_object_mut() {
        map.remove("ephemeral");
    }

    ensure_reactions(v);
    ensure_time(v, &timestamp);

    let out = serde_json::to_string(&v).unwrap_or_else(|_| v.to_string());
    match state
        .db
        .query_one(
            "INSERT INTO messages (channel_id, content) VALUES ($1, $2) RETURNING id::bigint",
            &[&channel_id, &out],
        )
        .await
    {
        Ok(row) => {
            let id: i64 = row.get(0);
            v["id"] = Value::from(id);
            let out_with_id = serde_json::to_string(&v).unwrap_or_else(|_| out.clone());
            let chan_tx = get_or_create_channel(state, channel_id).await;
            let _ = chan_tx.send(out_with_id);

            if let Some(expiry) = ephemeral_expiry {
                let state_clone = Arc::clone(state);
                let ch_id = channel_id;
                tokio::spawn(async move {
                    let mut delay = expiry.signed_duration_since(Utc::now());
                    if delay < ChronoDuration::zero() {
                        delay = ChronoDuration::zero();
                    }
                    if delay > ChronoDuration::seconds(MAX_EPHEMERAL_SECONDS) {
                        delay = ChronoDuration::seconds(MAX_EPHEMERAL_SECONDS);
                    }
                    if let Ok(duration) = delay.to_std() {
                        tokio::time::sleep(duration).await;
                    }
                    if let Ok(id32) = i32::try_from(id) {
                        match db::delete_message(&state_clone.db, id32).await {
                            Ok(true) => {
                                let payload = serde_json::json!({
                                    "type": "message-deleted",
                                    "id": id,
                                    "channelId": ch_id,
                                });
                                let chan_sender =
                                    get_or_create_channel(&state_clone, ch_id).await;
                                let _ = chan_sender.send(payload.to_string());
                            }
                            Ok(false) => {}
                            Err(error) => {
                                error!("failed to delete ephemeral message {id}: {error}");
                            }
                        }
                    }
                });
            }
        }
        Err(e) => error!("db insert error: {e}"),
    }
}

/// Handle delete message request.
async fn handle_delete_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: i32,
    user_name: &Option<String>,
) {
    let requester = match user_name.clone() {
        Some(name) => name,
        None => {
            let msg = serde_json::json!({
                "type": "error",
                "message": "not-authenticated",
            })
            .to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    let Some(raw_id) = v.get("messageId").and_then(|m| m.as_i64()) else {
        let msg = serde_json::json!({
            "type": "error",
            "message": "invalid-message-id",
        })
        .to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    };

    let message_id32 = match i32::try_from(raw_id) {
        Ok(id) => id,
        Err(_) => {
            let msg = serde_json::json!({
                "type": "error",
                "message": "invalid-message-id",
            })
            .to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    let record = match db::get_message_record(&state.db, message_id32).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            let msg = serde_json::json!({
                "type": "error",
                "message": "message-not-found",
            })
            .to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
        Err(error) => {
            error!("failed to load message {raw_id} for deletion: {error}");
            let msg = serde_json::json!({
                "type": "error",
                "message": "message-delete-failed",
            })
            .to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    if record.channel_id != channel_id {
        let msg = serde_json::json!({
            "type": "error",
            "message": "message-wrong-channel",
        })
        .to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    }

    let owner = record
        .content
        .get("user")
        .and_then(|user| user.as_str())
        .map(|value| value.to_string());

    let mut allowed = owner.as_deref() == Some(requester.as_str());
    if !allowed && can_manage_channels(state, &requester).await {
        allowed = true;
    }

    if !allowed {
        let msg = serde_json::json!({
            "type": "error",
            "message": "message-permission-denied",
        })
        .to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    }

    match db::delete_message(&state.db, message_id32).await {
        Ok(true) => {
            let payload = serde_json::json!({
                "type": "message-deleted",
                "id": raw_id,
                "channelId": record.channel_id,
            });
            let chan_sender = get_or_create_channel(state, record.channel_id).await;
            let _ = chan_sender.send(payload.to_string());
        }
        Ok(false) => {
            let msg = serde_json::json!({
                "type": "error",
                "message": "message-not-found",
            })
            .to_string();
            let _ = sender.send(Message::Text(msg)).await;
        }
        Err(error) => {
            error!("failed to delete message {raw_id}: {error}");
            let msg = serde_json::json!({
                "type": "error",
                "message": "message-delete-failed",
            })
            .to_string();
            let _ = sender.send(Message::Text(msg)).await;
        }
    }
}

/// Handle reaction (add/remove emoji) request.
async fn handle_react(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let user = match user_name.clone() {
        Some(name) => name,
        None => {
            let msg =
                serde_json::json!({"type": "error", "message": "not-authenticated"}).to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    let Some(message_id) = v.get("messageId").and_then(|m| m.as_i64()) else {
        let msg = serde_json::json!({"type": "error", "message": "invalid-message-id"}).to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    };

    let Some(action) = v.get("action").and_then(|a| a.as_str()) else {
        let msg =
            serde_json::json!({"type": "error", "message": "invalid-reaction-action"}).to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    };

    let Some(raw_emoji) = v.get("emoji").and_then(|e| e.as_str()) else {
        let msg = serde_json::json!({"type": "error", "message": "invalid-emoji"}).to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    };

    let emoji = raw_emoji.trim();
    if emoji.is_empty()
        || emoji.len() > 16
        || emoji.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        let msg = serde_json::json!({"type": "error", "message": "invalid-emoji"}).to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    }

    let message_id32 = match i32::try_from(message_id) {
        Ok(val) => val,
        Err(_) => {
            let msg =
                serde_json::json!({"type": "error", "message": "invalid-message-id"}).to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    let target_channel_id = match db::get_message_channel_id(&state.db, message_id32).await {
        Ok(Some(ch)) => ch,
        Ok(None) => {
            let msg =
                serde_json::json!({"type": "error", "message": "message-not-found"}).to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
        Err(e) => {
            error!("failed to lookup message channel for reaction: {e}");
            let msg =
                serde_json::json!({"type": "error", "message": "reaction-failed"}).to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    let result = match action {
        "add" => db::add_reaction(&state.db, message_id32, &user, emoji).await,
        "remove" => db::remove_reaction(&state.db, message_id32, &user, emoji).await,
        _ => {
            let msg = serde_json::json!({"type": "error", "message": "invalid-reaction-action"})
                .to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    if let Err(e) = result {
        error!("db reaction error: {e}");
        let msg = serde_json::json!({"type": "error", "message": "reaction-failed"}).to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    }

    let reactions = match db::get_reaction_summary(&state.db, message_id32).await {
        Ok(map) => map,
        Err(e) => {
            error!("db reaction summary error: {e}");
            return;
        }
    };

    let payload = serde_json::json!({
        "type": "reaction-update",
        "channelId": target_channel_id,
        "messageId": message_id,
        "reactions": reactions,
    });
    let chan_sender = get_or_create_channel(state, target_channel_id).await;
    let _ = chan_sender.send(payload.to_string());
}

/// Handle status update request.
async fn handle_status_update(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let user = match user_name.clone() {
        Some(name) => name,
        None => {
            let msg = serde_json::json!({
                "type": "error",
                "message": "not-authenticated",
            })
            .to_string();
            let _ = sender.send(Message::Text(msg)).await;
            return;
        }
    };

    let Some(raw_status) = v.get("status").and_then(|s| s.as_str()) else {
        let msg = serde_json::json!({
            "type": "error",
            "message": "invalid-status",
        })
        .to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    };

    let Some(status) = normalize_status(raw_status) else {
        let msg = serde_json::json!({
            "type": "error",
            "message": "invalid-status",
        })
        .to_string();
        let _ = sender.send(Message::Text(msg)).await;
        return;
    };

    state
        .statuses
        .lock()
        .await
        .insert(user.clone(), status.to_string());
    broadcast_status(state, &user, status).await;
}

/// Handle ping request.
async fn handle_ping(sender: &mut SplitSink<WebSocket, Message>, v: &Value) {
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let msg = serde_json::json!({ "type": "pong", "id": id });
    let _ = sender.send(Message::Text(msg.to_string())).await;
}

/// Handle voice join request.
async fn handle_voice_join(
    state: &Arc<AppState>,
    v: &Value,
    voice_channel: &mut Option<i32>,
    user_name: &Option<String>,
) {
    let Some(u) = user_name.as_deref() else { return };
    if let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()) {
        let ch_id = ch_id as i32;
        let mut map = state.voice_channels.lock().await;
        for info in map.values_mut() {
            info.users.remove(u);
        }
        if let Some(entry) = map.get_mut(&ch_id) {
            entry.users.insert(u.to_string());
            *voice_channel = Some(ch_id);
        }
        drop(map);
        broadcast_voice(state, ch_id).await;
        let msg = serde_json::json!({
            "type": "voice-join",
            "user": u,
            "channelId": ch_id,
        });
        let _ = state.tx.send(msg.to_string());
    }
}

/// Handle voice leave request.
async fn handle_voice_leave(
    state: &Arc<AppState>,
    v: &Value,
    voice_channel: &mut Option<i32>,
    user_name: &Option<String>,
) {
    let Some(u) = user_name.as_deref() else { return };
    if let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()) {
        let ch_id = ch_id as i32;
        let mut map = state.voice_channels.lock().await;
        if let Some(info) = map.get_mut(&ch_id) {
            info.users.remove(u);
        }
        drop(map);
        broadcast_voice(state, ch_id).await;
        if *voice_channel == Some(ch_id) {
            *voice_channel = None;
        }
        let msg = serde_json::json!({
            "type": "voice-leave",
            "user": u,
            "channelId": ch_id,
        });
        let _ = state.tx.send(msg.to_string());
    }
}

/// Handle set-role request from an Owner.
async fn handle_set_role(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            let _ = sender
                .send(Message::Text(errors::ROLE_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_roles(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::ROLE_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let Some(target_user) = v.get("user").and_then(|u| u.as_str()) else {
        let _ = sender
            .send(Message::Text(errors::ROLE_TARGET_NOT_FOUND.to_string()))
            .await;
        return;
    };

    let Some(role) = v.get("role").and_then(|r| r.as_str()) else {
        let _ = sender
            .send(Message::Text(errors::ROLE_UPDATE_FAILED.to_string()))
            .await;
        return;
    };

    let target_key = {
        let keys = state.user_keys.lock().await;
        keys.get(target_user).cloned()
    };

    let Some(key) = target_key else {
        let _ = sender
            .send(Message::Text(errors::ROLE_TARGET_NOT_FOUND.to_string()))
            .await;
        return;
    };

    let color = v
        .get("color")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .or_else(|| crate::roles::default_color(role));

    if let Err(e) = db::set_role(&state.db, &key, role, color.as_deref()).await {
        error!("Failed to set role for {target_user} (key {key}): {e}");
        let _ = sender
            .send(Message::Text(errors::ROLE_UPDATE_FAILED.to_string()))
            .await;
        return;
    }

    let info = RoleInfo {
        role: role.to_string(),
        color,
    };
    state
        .roles
        .lock()
        .await
        .insert(target_user.to_string(), info.clone());
    broadcast_role(state, target_user, &info.role, info.color.as_deref()).await;
    info!(requester, target_user, role, "Role assigned via WebSocket");
}

/// Handle remove-role request from an Owner.
async fn handle_remove_role(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            let _ = sender
                .send(Message::Text(errors::ROLE_PERMISSION_DENIED.to_string()))
                .await;
            return;
        }
    };

    if !can_manage_roles(state, requester).await {
        let _ = sender
            .send(Message::Text(errors::ROLE_PERMISSION_DENIED.to_string()))
            .await;
        return;
    }

    let Some(target_user) = v.get("user").and_then(|u| u.as_str()) else {
        let _ = sender
            .send(Message::Text(errors::ROLE_TARGET_NOT_FOUND.to_string()))
            .await;
        return;
    };

    let target_key = {
        let keys = state.user_keys.lock().await;
        keys.get(target_user).cloned()
    };

    let Some(key) = target_key else {
        let _ = sender
            .send(Message::Text(errors::ROLE_TARGET_NOT_FOUND.to_string()))
            .await;
        return;
    };

    if let Err(e) = db::remove_role(&state.db, &key).await {
        error!("Failed to remove role for {target_user} (key {key}): {e}");
        let _ = sender
            .send(Message::Text(errors::ROLE_UPDATE_FAILED.to_string()))
            .await;
        return;
    }

    state.roles.lock().await.remove(target_user);

    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "role-remove",
        "user": target_user,
    })) {
        let _ = state.tx.send(msg);
    }
    info!(requester, target_user, "Role removed via WebSocket");
}

/// Handle client disconnect cleanup.
async fn handle_disconnect(state: &Arc<AppState>, user_name: Option<String>) {
    if let Some(name) = user_name {
        state.users.lock().await.remove(&name);
        broadcast_users(state).await;

        let mut map = state.voice_channels.lock().await;
        let mut ch_to_broadcast = None;
        for (id, info) in map.iter_mut() {
            if info.users.remove(&name) {
                ch_to_broadcast = Some(*id);
                break;
            }
        }
        drop(map);

        if let Some(ch_id) = ch_to_broadcast {
            broadcast_voice(state, ch_id).await;
        }

        state
            .statuses
            .lock()
            .await
            .insert(name.clone(), "offline".to_string());
        broadcast_status(state, &name, "offline").await;
    }
}

/// Axum handler that upgrades the HTTP connection to a WebSocket and spawns message processing.
#[instrument(skip(ws, state), fields(client_addr = %addr))]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}
