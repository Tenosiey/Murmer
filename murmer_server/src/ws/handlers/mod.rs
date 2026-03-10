//! WebSocket message handlers.
//!
//! The main socket loop lives here; domain-specific handlers are split into
//! submodules to keep each file focused:
//! - [`auth`] – user and bot authentication
//! - [`channels`] – text/voice channel and category management
//! - [`messages`] – chat, history, search and reactions

mod auth;
mod channels;
mod messages;

use super::{errors, helpers::*, validation::*};
use crate::{db, roles::RoleInfo, AppState};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    response::IntoResponse,
};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use serde_json::Value;
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
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, peer_addr: std::net::SocketAddr) {
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
                                if auth::handle_presence(&mut sender, &state, &mut v, &mut authenticated, &mut user_name, &client_ip, default_channel_id).await.is_err() {
                                    break;
                                }
                            }
                            "bot-presence" => {
                                if auth::handle_bot_presence(&mut sender, &state, &v, &mut authenticated, &mut user_name, default_channel_id).await.is_err() {
                                    break;
                                }
                            }
                            "join" => {
                                messages::handle_join(&state, &mut sender, &v, &mut channel_id, &mut chan_tx, &mut chan_rx).await;
                            }
                            "load-history" => {
                                messages::handle_load_history(&state, &mut sender, &v, channel_id).await;
                            }
                            "search-history" => {
                                messages::handle_search_history(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "create-channel" => {
                                channels::handle_create_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-channel" => {
                                if channels::handle_delete_channel(&state, &mut sender, &v, &user_name, &mut channel_id, &mut chan_tx, &mut chan_rx, default_channel_id).await.is_err() {
                                    continue;
                                }
                            }
                            "move-channel" => {
                                channels::handle_move_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "create-category" => {
                                channels::handle_create_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "rename-category" => {
                                channels::handle_rename_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-category" => {
                                channels::handle_delete_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "create-voice-channel" => {
                                channels::handle_create_voice_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "update-voice-channel" => {
                                channels::handle_update_voice_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-voice-channel" => {
                                channels::handle_delete_voice_channel(&state, &mut sender, &v, &user_name, &mut voice_channel).await;
                            }
                            "chat" => {
                                messages::handle_chat(&state, &mut sender, &mut v, channel_id, &user_name).await;
                            }
                            "delete-message" => {
                                messages::handle_delete_message(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "react" => {
                                messages::handle_react(&state, &mut sender, &v, &user_name).await;
                            }
                            "status-update" => {
                                handle_status_update(&state, &mut sender, &v, &user_name).await;
                            }
                            "ping" => {
                                handle_ping(&mut sender, &v).await;
                            }
                            "voice-join" => {
                                handle_voice_join(&state, &mut sender, &v, &mut voice_channel, &user_name).await;
                            }
                            "voice-leave" => {
                                handle_voice_leave(&state, &v, &mut voice_channel, &user_name).await;
                            }
                            "voice-offer" | "voice-answer" | "voice-candidate" => {
                                let _ = state.tx.send(text.to_string());
                            }
                            "screenshare-start" => {
                                handle_screenshare_start(&state, &v).await;
                                let _ = state.tx.send(text.to_string());
                            }
                            "screenshare-stop" => {
                                handle_screenshare_stop(&state, &v).await;
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

// ── Small handlers kept here to avoid creating very tiny files ──────────────

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
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    voice_channel: &mut Option<i32>,
    user_name: &Option<String>,
) {
    let Some(u) = user_name.as_deref() else {
        return;
    };
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

        send_active_screen_shares(state, sender, ch_id).await;
    }
}

/// Handle voice leave request.
async fn handle_voice_leave(
    state: &Arc<AppState>,
    v: &Value,
    voice_channel: &mut Option<i32>,
    user_name: &Option<String>,
) {
    let Some(u) = user_name.as_deref() else {
        return;
    };
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

/// Track a new screen share in application state.
async fn handle_screenshare_start(state: &Arc<AppState>, v: &Value) {
    let Some(user) = v.get("user").and_then(|u| u.as_str()) else {
        return;
    };
    let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()) else {
        return;
    };
    state
        .active_screen_shares
        .lock()
        .await
        .entry(ch_id as i32)
        .or_default()
        .insert(user.to_string());
}

/// Remove a screen share from application state.
async fn handle_screenshare_stop(state: &Arc<AppState>, v: &Value) {
    let Some(user) = v.get("user").and_then(|u| u.as_str()) else {
        return;
    };
    let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()) else {
        return;
    };
    let ch_id = ch_id as i32;
    let mut shares = state.active_screen_shares.lock().await;
    if let Some(set) = shares.get_mut(&ch_id) {
        set.remove(user);
        if set.is_empty() {
            shares.remove(&ch_id);
        }
    }
}

/// Send active screen shares for a voice channel to a single client.
async fn send_active_screen_shares(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    channel_id: i32,
) {
    let users: Vec<String> = {
        let shares = state.active_screen_shares.lock().await;
        shares
            .get(&channel_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    };
    if users.is_empty() {
        return;
    }
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "screenshare-active",
        "channelId": channel_id,
        "users": users,
    })) {
        let _ = sender.send(Message::Text(msg)).await;
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

        // Clean up any active screen shares owned by the disconnecting user.
        {
            let mut shares = state.active_screen_shares.lock().await;
            let channels_with_share: Vec<i32> = shares
                .iter()
                .filter(|(_, users)| users.contains(&name))
                .map(|(ch_id, _)| *ch_id)
                .collect();
            for ch_id in &channels_with_share {
                if let Some(set) = shares.get_mut(ch_id) {
                    set.remove(&name);
                    if set.is_empty() {
                        shares.remove(ch_id);
                    }
                }
            }
            drop(shares);
            for ch_id in channels_with_share {
                if let Ok(msg) = serde_json::to_string(&serde_json::json!({
                    "type": "screenshare-stop",
                    "user": name,
                    "channelId": ch_id,
                })) {
                    let _ = state.tx.send(msg);
                }
            }
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
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}
