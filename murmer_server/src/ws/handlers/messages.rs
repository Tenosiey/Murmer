//! Handlers for chat messages, message deletion, editing, forwarding,
//! reactions, history and search.
//!
//! Forwarding is the odd one out: the copy is made *here*, from the row the
//! server stored, so that the original author's name on it is not something
//! the sender could write. The rules it has to clear live in
//! [`crate::ws::helpers::prepare_forward`], and `docs/features.md` records
//! why each refusal exists.

use crate::channel_overrides::ChannelKind;
use crate::ws::{constants::*, errors, helpers::*, validation::is_emoji_shortcode};
use crate::{AppState, db, security};
use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::{SinkExt, stream::SplitSink};
use serde_json::{Map, Value};
use std::sync::Arc;
use tracing::error;

/// Whether `user` may see a specific text channel, applying per-channel
/// overrides. Users who cannot see a channel receive no history/pins and are
/// never subscribed to its broadcast.
async fn can_view_text(state: &Arc<AppState>, user_name: &Option<String>, channel_id: i32) -> bool {
    match user_name.as_deref() {
        Some(user) => can_view_channel(state, user, ChannelKind::Text, channel_id).await,
        None => false,
    }
}

/// Read and shape-check a message's `enc` object — the sealed envelope of an
/// encrypted channel. The plaintext inside is a JSON object holding the
/// message's text, attachment and reply preview, so the server can say nothing
/// about it beyond "this is base64 of a plausible size".
///
/// Returns the rebuilt `enc` value, so the stored frame carries known fields
/// only and a client cannot smuggle anything extra alongside the ciphertext.
/// `max_plaintext` is the server's configured message length limit — the
/// ciphertext is bounded by it just as plaintext is, which is the only length
/// rule that survives encryption.
fn sealed_message(v: &Value, max_plaintext: usize) -> Result<Value, &'static str> {
    let Some(enc) = v.get("enc") else {
        return Err(errors::CHANNEL_REQUIRES_ENCRYPTION);
    };
    let (Some(epoch), Some(nonce), Some(ciphertext)) = (
        enc.get("epoch").and_then(|e| e.as_i64()),
        enc.get("nonce").and_then(|n| n.as_str()),
        enc.get("ciphertext").and_then(|c| c.as_str()),
    ) else {
        return Err(errors::INVALID_ENCRYPTED_MESSAGE);
    };
    if !(1..=MAX_CHANNEL_KEY_EPOCH).contains(&epoch) {
        return Err(errors::INVALID_ENCRYPTED_MESSAGE);
    }
    match validate_sealed_payload(nonce, ciphertext, max_plaintext) {
        Ok(()) => {}
        Err(SealedPayloadError::TooLong) => return Err(errors::MESSAGE_TOO_LONG),
        Err(SealedPayloadError::Malformed) => return Err(errors::INVALID_ENCRYPTED_MESSAGE),
    }
    Ok(serde_json::json!({
        "epoch": epoch,
        "nonce": nonce,
        "ciphertext": ciphertext,
    }))
}

/// Handle channel join and load initial history.
pub(super) async fn handle_join(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: &mut i32,
    chan_tx: &mut tokio::sync::broadcast::Sender<crate::Frame>,
    chan_rx: &mut tokio::sync::broadcast::Receiver<crate::Frame>,
    user_name: &Option<String>,
) {
    if let Some(ch_id) = v.get("channelId").and_then(|c| c.as_i64()) {
        let ch_id = ch_id as i32;
        // Refuse to switch to a channel the user cannot see, so a non-viewer is
        // never even subscribed to the channel's live broadcast.
        if !can_view_text(state, user_name, ch_id).await {
            return;
        }
        *channel_id = ch_id;
        *chan_tx = get_or_create_channel(state, *channel_id).await;
        *chan_rx = chan_tx.subscribe();
        db::send_history(&state.db, sender, *channel_id, None, DEFAULT_HISTORY_LIMIT).await;
        super::pins::send_pins(state, sender, *channel_id).await;
        super::wiki::send_wiki_index(state, sender, *channel_id).await;
    }
}

/// Handle history loading request.
pub(super) async fn handle_load_history(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    channel_id: i32,
    user_name: &Option<String>,
) {
    if !can_view_text(state, user_name, channel_id).await {
        return;
    }
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

/// Wiki page hits for a search, as client-facing JSON. Errors are logged and
/// reported as "no pages" so a broken wiki index cannot swallow the message
/// results the user actually asked for.
async fn search_wiki_hits(
    state: &Arc<AppState>,
    channel_id: i32,
    query: &str,
    limit: i64,
) -> Vec<Value> {
    let limit = limit.min(MAX_WIKI_SEARCH_RESULTS);
    match db::search_wiki_pages(&state.db, channel_id, query, limit).await {
        Ok(hits) => hits
            .iter()
            .map(|hit| {
                serde_json::json!({
                    "slug": hit.slug,
                    "title": hit.title,
                    "snippet": hit.snippet,
                    "updatedBy": hit.updated_by,
                    "updatedAt": hit.updated_at,
                })
            })
            .collect(),
        Err(error) => {
            error!("Wiki search failed for channel {channel_id}: {error}");
            Vec::new()
        }
    }
}

/// Handle search history request.
///
/// Answers with both the matching messages and the channel's matching wiki
/// pages: the two live in separate FTS indexes but are one search to the
/// user. A wiki index failure degrades to no page hits rather than failing
/// the whole search — the messages are the primary answer.
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
            "pages": [],
        });
        let _ = sender.send(Message::Text(payload.to_string().into())).await;
        return;
    }

    let channel_to_search = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
        .unwrap_or(channel_id);

    // The frame names the channel, so a client could ask about one it may not
    // see; a private channel's messages and wiki pages must not leak through
    // search any more than through history.
    if !can_view_text(state, user_name, channel_to_search).await {
        let payload = serde_json::json!({
            "type": "search-results",
            "requestId": request_id,
            "channelId": channel_to_search,
            "messages": [],
            "pages": [],
        });
        let _ = sender.send(Message::Text(payload.to_string().into())).await;
        return;
    }

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
                    if let Some(reactions) = reaction_map.get(&id)
                        && let Ok(reaction_value) = serde_json::to_value(reactions)
                    {
                        value["reactions"] = reaction_value;
                    }
                    if value.get("reactions").is_none() {
                        value["reactions"] = Value::Object(Map::new());
                    }
                    messages.push(value);
                }
            }

            let pages = search_wiki_hits(state, channel_to_search, trimmed_query, limit).await;

            let payload = serde_json::json!({
                "type": "search-results",
                "requestId": request_id,
                "channelId": channel_to_search,
                "messages": messages,
                "pages": pages,
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

    // Sending requires seeing the channel and holding SEND_MESSAGES within it
    // (per-channel overrides included). Enforced server-side so a client whose
    // permission was revoked cannot post by ignoring the disabled composer.
    if !can_view_channel(state, user, ChannelKind::Text, channel_id).await
        || !has_channel_permission(
            state,
            user,
            ChannelKind::Text,
            channel_id,
            crate::permissions::SEND_MESSAGES,
        )
        .await
    {
        send_error(sender, errors::SEND_PERMISSION_DENIED).await;
        return;
    }

    if !security::check_message_rate_limit(&state.rate_limiter, user).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return;
    }

    if super::moderation::is_muted(state, user).await {
        send_error(sender, errors::MUTED).await;
        return;
    }

    // Slow mode is checked before anything is written, and only recorded once
    // the message is actually on its way (see the end of this function), so a
    // rejected message never costs the sender their turn.
    if !super::chat_settings::slow_mode_allows(state, user).await {
        send_error(sender, errors::SLOW_MODE).await;
        return;
    }

    // An encrypted channel takes sealed envelopes and nothing else; a
    // plaintext one takes plaintext and no `enc`. Deciding this from the
    // channel's stored flag rather than from the frame is what keeps a client
    // from opting a message out of the channel's encryption.
    let max_length = super::chat_settings::max_message_length(state).await;
    let encrypted = channel_is_e2ee(state, channel_id).await;
    if encrypted {
        if PLAINTEXT_MESSAGE_FIELDS
            .iter()
            .any(|field| v.get(field).is_some_and(|value| !value.is_null()))
        {
            send_error(sender, errors::CHANNEL_REQUIRES_ENCRYPTION).await;
            return;
        }
        match sealed_message(v, max_length) {
            Ok(enc) => v["enc"] = enc,
            Err(code) => {
                send_error(sender, code).await;
                return;
            }
        }
    } else {
        if let Some(text) = v.get("text").and_then(|t| t.as_str())
            && text.len() > max_length
        {
            send_error(sender, errors::MESSAGE_TOO_LONG).await;
            return;
        }
        if let Some(map) = v.as_object_mut() {
            map.remove("enc");
        }
    }

    // Masking happens before the message is stored or broadcast, so the
    // filtered form is the only one any other client can ever see. It is a
    // no-op in an encrypted channel: the server holds no text to mask there,
    // which is a limit of the profanity filter rather than a way around it.
    super::chat_settings::apply_profanity_filter(state, v).await;

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
                // The quoted snippet is rebuilt from the stored message so a
                // client cannot forge it. In an encrypted channel there is no
                // stored text to quote, so the snippet stays empty and the
                // sender's own copy of it travels inside the ciphertext
                // instead — forgeable only to the members who already hold the
                // key, which is the same set that can read both messages.
                let quoted_text = if encrypted {
                    String::new()
                } else {
                    reply_preview(
                        record
                            .content
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or(""),
                        MAX_REPLY_PREVIEW_CHARS,
                    )
                };
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

    publish_message(state, v, channel_id, user, &timestamp, ephemeral_expiry).await;
}

/// Store a finished message frame and announce it everywhere it has to be
/// announced: to the destination channel's own broadcast, and server-wide as
/// a `message-notify` so clients viewing another channel can still count it.
///
/// Shared by a new message and by a forwarded copy. Both are an ordinary
/// message by the time the frame is built, and both have to land on every one
/// of these steps — a copy that skipped the notify would simply never raise
/// anyone's unread badge.
async fn publish_message(
    state: &Arc<AppState>,
    v: &mut Value,
    channel_id: i32,
    user: &str,
    timestamp: &DateTime<Utc>,
    ephemeral_expiry: Option<DateTime<Utc>>,
) {
    ensure_reactions(v);
    ensure_time(v, timestamp);

    let out = serde_json::to_string(&v).unwrap_or_else(|_| v.to_string());
    match db::insert_message(&state.db, channel_id, &out).await {
        Ok(id) => {
            v["id"] = Value::from(id);
            let out_with_id = serde_json::to_string(&v).unwrap_or_else(|_| out.clone());
            let chan_tx = get_or_create_channel(state, channel_id).await;
            let _ = chan_tx.send(out_with_id.into());

            // Channel broadcasts only reach clients joined to this channel, so
            // additionally announce the message globally. Clients use this to
            // track unread counts and mentions for channels they are not
            // currently viewing.
            // Carries the sealed envelope rather than the text for an
            // encrypted channel: the frame is already filtered to members (see
            // `channel_scope` in the socket loop), and they hold the key, so
            // mention highlighting keeps working without the server ever
            // handling the plaintext.
            let mut notify = serde_json::json!({
                "type": "message-notify",
                "channelId": channel_id,
                "id": id,
                "user": user,
                "text": v.get("text").cloned().unwrap_or(Value::Null),
            });
            if let Some(enc) = v.get("enc") {
                notify["enc"] = enc.clone();
            }
            let _ = state.tx.send(notify.to_string().into());

            if let Some(expiry) = ephemeral_expiry {
                schedule_ephemeral_deletion(Arc::clone(state), id, channel_id, expiry);
            }

            // Lifetime stats (no-op unless server and user both opted in).
            super::stats::record(state, user, super::stats::chat_message_deltas(v)).await;

            // Starts this sender's slow mode interval; a no-op while slow
            // mode is off.
            super::chat_settings::note_message_sent(state, user).await;
        }
        Err(e) => error!("db insert error: {e}"),
    }
}

/// Handle a request to forward an existing message into another text channel.
///
/// The client names a message id and a destination and nothing else: the words
/// *and* the attribution are both copied out of the row the server stored (see
/// [`forwarded_body`]). A client-composed "quote" would let anyone make any
/// member appear to have said anything, in front of a channel with no way to
/// check — the same reason a reply's snippet is rebuilt rather than trusted.
///
/// Forwarding into a direct message is not this frame. DM content is
/// end-to-end encrypted, so there is no copy for the server to make; the
/// client composes that one itself and the attribution travels sealed with it
/// (see `murmer_client/src/lib/chat/forward.ts`).
pub(super) async fn handle_forward_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };

    let Some(message_id) = v.get("messageId").and_then(|m| m.as_i64()) else {
        send_error(sender, errors::INVALID_MESSAGE_ID).await;
        return;
    };
    let Some(target_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .and_then(|c| i32::try_from(c).ok())
    else {
        send_error(sender, errors::UNKNOWN_CHANNEL).await;
        return;
    };

    if !security::check_message_rate_limit(&state.rate_limiter, &user).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return;
    }
    if super::moderation::is_muted(state, &user).await {
        send_error(sender, errors::MUTED).await;
        return;
    }
    if !super::chat_settings::slow_mode_allows(state, &user).await {
        send_error(sender, errors::SLOW_MODE).await;
        return;
    }

    let mut out = match prepare_forward(state, &user, message_id, target_id).await {
        Ok(body) => body,
        Err(code) => {
            send_error(sender, code).await;
            return;
        }
    };

    // The stored copy passed the length limit when it was written, but the
    // limit may have been lowered since and the copy is a new message under
    // the current policy.
    let max_length = super::chat_settings::max_message_length(state).await;
    if out
        .get("text")
        .and_then(|t| t.as_str())
        .is_some_and(|text| text.len() > max_length)
    {
        send_error(sender, errors::MESSAGE_TOO_LONG).await;
        return;
    }

    out["type"] = Value::String("chat".to_string());
    out["user"] = Value::String(user.clone());
    out["channelId"] = Value::from(target_id);
    let timestamp = Utc::now();
    out["timestamp"] = Value::String(timestamp.to_rfc3339());

    // Re-filtered rather than trusted: the word list may have grown since the
    // original was stored, and a forward is the cheapest way to bring an old
    // message back under the current policy.
    super::chat_settings::apply_profanity_filter(state, &mut out).await;

    publish_message(state, &mut out, target_id, &user, &timestamp, None).await;
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
    if !allowed
        && has_channel_permission(
            state,
            &requester,
            ChannelKind::Text,
            channel_id,
            crate::permissions::MANAGE_MESSAGES,
        )
        .await
    {
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
            let _ = chan_sender.send(payload.to_string().into());

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

    // An edit replaces the message's payload, so it has to arrive in whatever
    // form the channel stores: a sealed envelope in an encrypted channel, text
    // in a plaintext one.
    let encrypted = channel_is_e2ee(state, channel_id).await;
    let max_length = super::chat_settings::max_message_length(state).await;
    let mut new_text = String::new();
    let mut new_enc = Value::Null;
    if encrypted {
        if v.get("text").is_some_and(|value| !value.is_null()) {
            send_error(sender, errors::CHANNEL_REQUIRES_ENCRYPTION).await;
            return;
        }
        match sealed_message(v, max_length) {
            Ok(enc) => new_enc = enc,
            Err(code) => {
                send_error(sender, code).await;
                return;
            }
        }
    } else {
        let Some(raw_text) = v.get("text").and_then(|t| t.as_str()) else {
            send_error(sender, errors::INVALID_MESSAGE_TEXT).await;
            return;
        };
        if raw_text.trim().is_empty() || raw_text.len() > max_length {
            send_error(sender, errors::MESSAGE_TOO_LONG).await;
            return;
        }
        // An edit runs through the same filter as a new message: otherwise
        // posting and immediately editing would walk straight past it. There
        // is nothing to filter in an encrypted channel — the server never sees
        // the text, before or after the edit.
        new_text = super::chat_settings::mask_text(state, raw_text).await;
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

    if let Err(code) = may_edit_message(&record.content, &requester) {
        send_error(sender, code).await;
        return;
    }

    let mut content = record.content.clone();
    let edited_at = Utc::now().to_rfc3339();
    if encrypted {
        content["enc"] = new_enc.clone();
    } else {
        content["text"] = Value::String(new_text.clone());
    }
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
            let mut payload = serde_json::json!({
                "type": "message-edited",
                "id": message_id,
                "channelId": record.channel_id,
                "editedAt": edited_at,
            });
            if encrypted {
                payload["enc"] = new_enc;
            } else {
                payload["text"] = Value::String(new_text.clone());
            }
            let chan_sender = get_or_create_channel(state, record.channel_id).await;
            let _ = chan_sender.send(payload.to_string().into());

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
                    if let Some(reactions) = reaction_map.get(&id)
                        && let Ok(reaction_value) = serde_json::to_value(reactions)
                    {
                        value["reactions"] = reaction_value;
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
    if let Some(prev) = last_typing_broadcast
        && prev.elapsed() < throttle
    {
        return;
    }
    *last_typing_broadcast = Some(std::time::Instant::now());

    let payload = serde_json::json!({
        "type": "typing",
        "user": user,
        "channelId": channel_id,
    });
    let chan_tx = get_or_create_channel(state, channel_id).await;
    let _ = chan_tx.send(payload.to_string().into());
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

    // Reacting requires seeing the channel; adding a reaction additionally
    // requires SEND_MESSAGES there. Removing your own reaction stays allowed as
    // long as you can still see the channel.
    if !can_view_channel(state, &user, ChannelKind::Text, target_channel_id).await
        || (action == "add"
            && !has_channel_permission(
                state,
                &user,
                ChannelKind::Text,
                target_channel_id,
                crate::permissions::SEND_MESSAGES,
            )
            .await)
    {
        send_error(sender, errors::SEND_PERMISSION_DENIED).await;
        return;
    }

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
    let _ = chan_sender.send(payload.to_string().into());
}
