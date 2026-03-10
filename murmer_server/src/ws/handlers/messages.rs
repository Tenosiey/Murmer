//! Handlers for chat messages, message deletion, reactions, history and search.

use crate::ws::{constants::*, errors, helpers::*};
#[allow(unused_imports)]
use crate::{db, security, AppState};
use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::{stream::SplitSink, SinkExt};
use serde_json::{Map, Value};
use std::sync::Arc;
use tracing::error;

/// Handle channel join and load initial history.
pub(super) async fn handle_join(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: &mut i32,
    chan_tx: &mut tokio::sync::broadcast::Sender<String>,
    chan_rx: &mut tokio::sync::broadcast::Receiver<String>,
) {
    if let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()) {
        *channel_id = ch_id as i32;
        *chan_tx = get_or_create_channel(state, *channel_id).await;
        *chan_rx = chan_tx.subscribe();
        db::send_history(&state.db, sender, *channel_id, None, DEFAULT_HISTORY_LIMIT).await;
    }
}

/// Handle history loading request.
pub(super) async fn handle_load_history(
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
pub(super) async fn handle_search_history(
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

/// Handle chat message: persist, broadcast, and schedule ephemeral deletion.
#[tracing::instrument(skip(state, sender, v), fields(channel_id = %channel_id, user = ?user_name))]
pub(super) async fn handle_chat(
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
                                let chan_sender = get_or_create_channel(&state_clone, ch_id).await;
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
pub(super) async fn handle_delete_message(
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
pub(super) async fn handle_react(
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
