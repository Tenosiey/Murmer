//! Handlers for chat messages, message deletion, editing, reactions, history and search.

use crate::ws::{constants::*, errors, helpers::*, validation::is_emoji_shortcode};
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
        super::pins::send_pins(state, sender, *channel_id).await;
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
        let _ = sender.send(Message::Text(payload.to_string().into())).await;
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
        let _ = sender.send(Message::Text(payload.to_string().into())).await;
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
            let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
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
                    if let Some(reactions) = reaction_map.get(&id) {
                        if let Ok(reaction_value) = serde_json::to_value(reactions) {
                            value["reactions"] = reaction_value;
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
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
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
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
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
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return;
    }

    if super::moderation::is_muted(state, user).await {
        send_error(sender, errors::MUTED).await;
        return;
    }

    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        if text.len() > MAX_MESSAGE_LENGTH {
            send_error(sender, errors::MESSAGE_TOO_LONG).await;
            return;
        }
    }

    v["user"] = Value::String(user.clone());
    v["channelId"] = Value::from(channel_id);
    if let Some(map) = v.as_object_mut() {
        map.remove("channel");
    }
    let timestamp = sanitize_message_timestamp(v);

    // Replies carry only the target message id from the client; the quoted
    // snippet and thread root are rebuilt from the stored message so a client
    // cannot forge quotes or attach messages to arbitrary threads.
    let reply_target = v.get("replyTo").and_then(|r| match r {
        Value::Number(n) => n.as_i64(),
        Value::Object(o) => o.get("id").and_then(|i| i.as_i64()),
        _ => None,
    });
    if let Some(map) = v.as_object_mut() {
        map.remove("replyTo");
        map.remove("threadId");
    }
    if let Some(target_id) = reply_target {
        match db::get_message_record(&state.db, target_id).await {
            Ok(Some(record)) if record.channel_id == channel_id => {
                let quoted_user = record
                    .content
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let quoted_text = reply_preview(
                    record
                        .content
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or(""),
                    MAX_REPLY_PREVIEW_CHARS,
                );
                v["replyTo"] = serde_json::json!({
                    "id": target_id,
                    "user": quoted_user,
                    "text": quoted_text,
                });
                // Replying to a reply joins the existing thread instead of
                // starting a nested one.
                let thread_root = record
                    .content
                    .get("threadId")
                    .and_then(|t| t.as_i64())
                    .unwrap_or(target_id);
                v["threadId"] = Value::from(thread_root);
            }
            Ok(_) => {
                send_error(sender, errors::REPLY_TARGET_NOT_FOUND).await;
                return;
            }
            Err(error) => {
                error!("failed to load reply target {target_id}: {error}");
            }
        }
    }

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
    match db::insert_message(&state.db, channel_id, &out).await {
        Ok(id) => {
            v["id"] = Value::from(id);
            let out_with_id = serde_json::to_string(&v).unwrap_or_else(|_| out.clone());
            let chan_tx = get_or_create_channel(state, channel_id).await;
            let _ = chan_tx.send(out_with_id);

            // Channel broadcasts only reach clients joined to this channel, so
            // additionally announce the message globally. Clients use this to
            // track unread counts and mentions for channels they are not
            // currently viewing.
            let notify = serde_json::json!({
                "type": "message-notify",
                "channelId": channel_id,
                "id": id,
                "user": user,
                "text": v.get("text").cloned().unwrap_or(Value::Null),
            });
            let _ = state.tx.send(notify.to_string());

            if let Some(expiry) = ephemeral_expiry {
                schedule_ephemeral_deletion(Arc::clone(state), id, channel_id, expiry);
            }

            // Lifetime stats (no-op unless server and user both opted in).
            super::stats::record(state, user, super::stats::chat_message_deltas(v)).await;
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
            send_error(sender, errors::NOT_AUTHENTICATED).await;
            return;
        }
    };

    let Some(message_id) = v.get("messageId").and_then(|m| m.as_i64()) else {
        send_error(sender, errors::INVALID_MESSAGE_ID).await;
        return;
    };

    let record = match db::get_message_record(&state.db, message_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            send_error(sender, errors::MESSAGE_NOT_FOUND).await;
            return;
        }
        Err(error) => {
            error!("failed to load message {message_id} for deletion: {error}");
            send_error(sender, errors::MESSAGE_DELETE_FAILED).await;
            return;
        }
    };

    if record.channel_id != channel_id {
        send_error(sender, errors::MESSAGE_WRONG_CHANNEL).await;
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
        send_error(sender, errors::MESSAGE_PERMISSION_DENIED).await;
        return;
    }

    match db::delete_message(&state.db, message_id).await {
        Ok(true) => {
            let payload = serde_json::json!({
                "type": "message-deleted",
                "id": message_id,
                "channelId": record.channel_id,
            });
            let chan_sender = get_or_create_channel(state, record.channel_id).await;
            let _ = chan_sender.send(payload.to_string());

            // Only deleting one's own message counts towards the stat;
            // moderator deletions say nothing about the requester's habits.
            if owner.as_deref() == Some(requester.as_str()) {
                super::stats::record(state, &requester, vec![(db::Stat::MessagesDeleted, 1)]).await;
            }
        }
        Ok(false) => {
            send_error(sender, errors::MESSAGE_NOT_FOUND).await;
        }
        Err(error) => {
            error!("failed to delete message {message_id}: {error}");
            send_error(sender, errors::MESSAGE_DELETE_FAILED).await;
        }
    }
}

/// Handle edit message request. Only the original author may edit a message.
pub(super) async fn handle_edit_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: i32,
    user_name: &Option<String>,
) {
    let requester = match user_name.clone() {
        Some(name) => name,
        None => {
            send_error(sender, errors::NOT_AUTHENTICATED).await;
            return;
        }
    };

    let Some(message_id) = v.get("messageId").and_then(|m| m.as_i64()) else {
        send_error(sender, errors::INVALID_MESSAGE_ID).await;
        return;
    };

    let Some(new_text) = v.get("text").and_then(|t| t.as_str()) else {
        send_error(sender, errors::INVALID_MESSAGE_TEXT).await;
        return;
    };

    if new_text.trim().is_empty() || new_text.len() > MAX_MESSAGE_LENGTH {
        send_error(sender, errors::MESSAGE_TOO_LONG).await;
        return;
    }

    let record = match db::get_message_record(&state.db, message_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            send_error(sender, errors::MESSAGE_NOT_FOUND).await;
            return;
        }
        Err(error) => {
            error!("failed to load message {message_id} for edit: {error}");
            send_error(sender, errors::MESSAGE_EDIT_FAILED).await;
            return;
        }
    };

    if record.channel_id != channel_id {
        send_error(sender, errors::MESSAGE_WRONG_CHANNEL).await;
        return;
    }

    let owner = record
        .content
        .get("user")
        .and_then(|user| user.as_str())
        .map(|value| value.to_string());

    // Editing rewrites someone's words, so unlike deletion it is never
    // extended to moderators - only the author may do it.
    if owner.as_deref() != Some(requester.as_str()) {
        send_error(sender, errors::MESSAGE_PERMISSION_DENIED).await;
        return;
    }

    let mut content = record.content.clone();
    let edited_at = Utc::now().to_rfc3339();
    content["text"] = Value::String(new_text.to_string());
    content["edited"] = Value::Bool(true);
    content["editedAt"] = Value::String(edited_at.clone());

    let serialized = match serde_json::to_string(&content) {
        Ok(out) => out,
        Err(error) => {
            error!("failed to serialize edited message {message_id}: {error}");
            send_error(sender, errors::MESSAGE_EDIT_FAILED).await;
            return;
        }
    };

    match db::update_message_content(&state.db, message_id, &serialized).await {
        Ok(true) => {
            let payload = serde_json::json!({
                "type": "message-edited",
                "id": message_id,
                "channelId": record.channel_id,
                "text": new_text,
                "editedAt": edited_at,
            });
            let chan_sender = get_or_create_channel(state, record.channel_id).await;
            let _ = chan_sender.send(payload.to_string());

            super::stats::record(state, &requester, vec![(db::Stat::MessagesEdited, 1)]).await;
        }
        Ok(false) => {
            send_error(sender, errors::MESSAGE_NOT_FOUND).await;
        }
        Err(error) => {
            error!("failed to edit message {message_id}: {error}");
            send_error(sender, errors::MESSAGE_EDIT_FAILED).await;
        }
    }
}

/// Handle a thread load request: return the root message and all replies that
/// belong to its thread.
pub(super) async fn handle_load_thread(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: i32,
) {
    let root_id = match v.get("rootId").and_then(|r| r.as_i64()) {
        Some(id) => id,
        None => {
            send_error(sender, errors::INVALID_MESSAGE_ID).await;
            return;
        }
    };

    match db::fetch_thread(&state.db, channel_id, root_id, MAX_THREAD_MESSAGES).await {
        Ok(rows) => {
            let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
            let reaction_map = if ids.is_empty() {
                std::collections::HashMap::new()
            } else {
                match db::get_reactions_for_messages(&state.db, &ids).await {
                    Ok(map) => map,
                    Err(error) => {
                        error!("Failed to load reactions for thread {root_id}: {error}");
                        std::collections::HashMap::new()
                    }
                }
            };

            let mut messages = Vec::new();
            for (id, content) in rows {
                if let Ok(mut value) = serde_json::from_str::<Value>(&content) {
                    value["id"] = Value::from(id);
                    if let Some(reactions) = reaction_map.get(&id) {
                        if let Ok(reaction_value) = serde_json::to_value(reactions) {
                            value["reactions"] = reaction_value;
                        }
                    }
                    messages.push(value);
                }
            }

            let payload = serde_json::json!({
                "type": "thread",
                "rootId": root_id,
                "channelId": channel_id,
                "messages": messages,
            });
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
        }
        Err(error) => {
            error!("failed to load thread {root_id}: {error}");
            send_error(sender, errors::THREAD_LOAD_FAILED).await;
        }
    }
}

/// Handle a typing notification: rebroadcast it to everyone in the channel.
/// Typing events are transient and never persisted; a per-connection throttle
/// keeps a misbehaving client from flooding the channel.
pub(super) async fn handle_typing(
    state: &Arc<AppState>,
    channel_id: i32,
    user_name: &Option<String>,
    last_typing_broadcast: &mut Option<std::time::Instant>,
) {
    let Some(user) = user_name.as_deref() else {
        return;
    };

    let throttle = std::time::Duration::from_millis(TYPING_BROADCAST_INTERVAL_MS);
    if let Some(prev) = last_typing_broadcast {
        if prev.elapsed() < throttle {
            return;
        }
    }
    *last_typing_broadcast = Some(std::time::Instant::now());

    let payload = serde_json::json!({
        "type": "typing",
        "user": user,
        "channelId": channel_id,
    });
    let chan_tx = get_or_create_channel(state, channel_id).await;
    let _ = chan_tx.send(payload.to_string());
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
            send_error(sender, errors::NOT_AUTHENTICATED).await;
            return;
        }
    };

    let Some(message_id) = v.get("messageId").and_then(|m| m.as_i64()) else {
        send_error(sender, errors::INVALID_MESSAGE_ID).await;
        return;
    };

    let Some(action) = v.get("action").and_then(|a| a.as_str()) else {
        send_error(sender, errors::INVALID_REACTION_ACTION).await;
        return;
    };

    let Some(raw_emoji) = v.get("emoji").and_then(|e| e.as_str()) else {
        send_error(sender, errors::INVALID_EMOJI).await;
        return;
    };

    let emoji = raw_emoji.trim();
    // Custom emoji shortcodes (`:name:`) may exceed the 16-byte cap that
    // bounds regular unicode reactions.
    let shortcode = is_emoji_shortcode(emoji);
    if !shortcode
        && (emoji.is_empty()
            || emoji.len() > 16
            || emoji.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        send_error(sender, errors::INVALID_EMOJI).await;
        return;
    }

    // Adding a shortcode reaction requires the emoji to actually exist so
    // junk shortcodes cannot be planted; removal stays permissive so
    // reactions of since-deleted emojis remain removable.
    if shortcode && action == "add" {
        match db::emoji_exists(&state.db, emoji.trim_matches(':')).await {
            Ok(true) => {}
            Ok(false) => {
                send_error(sender, errors::INVALID_EMOJI).await;
                return;
            }
            Err(e) => {
                error!("db emoji lookup error: {e}");
                send_error(sender, errors::REACTION_FAILED).await;
                return;
            }
        }
    }

    // The full record is loaded (not just the channel) so reaction stats can
    // credit the message author with a received reaction.
    let target_record = match db::get_message_record(&state.db, message_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            send_error(sender, errors::MESSAGE_NOT_FOUND).await;
            return;
        }
        Err(e) => {
            error!("failed to lookup message for reaction: {e}");
            send_error(sender, errors::REACTION_FAILED).await;
            return;
        }
    };
    let target_channel_id = target_record.channel_id;

    let result = match action {
        "add" => db::add_reaction(&state.db, message_id, &user, emoji).await,
        "remove" => db::remove_reaction(&state.db, message_id, &user, emoji).await,
        _ => {
            send_error(sender, errors::INVALID_REACTION_ACTION).await;
            return;
        }
    };

    if let Err(e) = result {
        error!("db reaction error: {e}");
        send_error(sender, errors::REACTION_FAILED).await;
        return;
    }

    if action == "add" {
        // Lifetime totals only count additions; taking a reaction back does
        // not subtract. Both sides are gated by their own opt-in.
        let author = target_record.content.get("user").and_then(|u| u.as_str());
        super::stats::record_reaction_added(state, &user, author, emoji).await;
    }

    let reactions = match db::get_reaction_summary(&state.db, message_id).await {
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
