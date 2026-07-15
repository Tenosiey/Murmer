//! REST API endpoints for bot management and bot actions.
//!
//! Admin endpoints (require `ADMIN_TOKEN`):
//!   - `POST   /api/v1/bots`                 – create a bot
//!   - `GET    /api/v1/bots`                 – list all bots
//!   - `GET    /api/v1/bots/:bot_id`         – get bot details
//!   - `PATCH  /api/v1/bots/:bot_id`         – update bot
//!   - `DELETE /api/v1/bots/:bot_id`         – delete bot
//!   - `POST   /api/v1/bots/:bot_id/reset-token` – regenerate token
//!
//! Bot endpoints (require bot token via `Authorization: Bearer <token>`):
//!   - `GET    /api/v1/channels`                                      – list channels
//!   - `POST   /api/v1/channels`                                      – create channel
//!   - `PATCH  /api/v1/channels/:channel_id`                          – update channel (topic)
//!   - `DELETE /api/v1/channels/:channel_id`                          – delete channel
//!   - `GET    /api/v1/channels/:channel_id/messages`                 – read messages
//!   - `POST   /api/v1/channels/:channel_id/messages`                 – send message (supports replies)
//!   - `GET    /api/v1/channels/:channel_id/messages/search`          – search messages
//!   - `PATCH  /api/v1/channels/:channel_id/messages/:message_id`     – edit own message
//!   - `DELETE /api/v1/channels/:channel_id/messages/:message_id`     – delete message
//!   - `GET    /api/v1/channels/:channel_id/messages/:message_id/thread`             – load thread
//!   - `POST   /api/v1/channels/:channel_id/messages/:message_id/reactions`          – add reaction
//!   - `DELETE /api/v1/channels/:channel_id/messages/:message_id/reactions/:emoji`   – remove reaction
//!   - `GET    /api/v1/channels/:channel_id/pins`                     – list pinned messages
//!   - `PUT    /api/v1/channels/:channel_id/pins/:message_id`         – pin message
//!   - `DELETE /api/v1/channels/:channel_id/pins/:message_id`         – unpin message
//!   - `POST   /api/v1/channels/:channel_id/typing`                   – broadcast typing indicator
//!   - `GET    /api/v1/emojis`                                        – list custom emojis
//!   - `GET    /api/v1/users`                                         – list users
//!   - `GET    /api/v1/server/info`                                   – server metadata

use crate::{db, security, ws, AppState};
use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::error;

use super::{
    db as bot_db,
    models::{
        generate_bot_id, generate_token, hash_token, AddReactionRequest, BotPermissions, BotRecord,
        CreateBotRequest, CreateChannelRequest, EditMessageRequest, MessageQuery, SearchQuery,
        SendMessageRequest, UpdateBotRequest, UpdateChannelRequest,
    },
};

const MAX_BOT_MESSAGE_LENGTH: usize = 4000;
const MAX_BOT_DESCRIPTION_LENGTH: usize = 256;
const MIN_EPHEMERAL_SECONDS: i64 = 5;
const MAX_EPHEMERAL_SECONDS: i64 = 86_400;
const DEFAULT_MESSAGE_LIMIT: i64 = 50;
const MAX_MESSAGE_LIMIT: i64 = 200;
// The following mirror the WebSocket handler limits in `ws::constants`
// (private to the ws module) so bots behave identically to regular clients.
const MAX_SEARCH_RESULTS: i64 = 200;
const MAX_THREAD_MESSAGES: i64 = 200;
const MAX_PINS_PER_CHANNEL: i64 = 25;
const MAX_REPLY_PREVIEW_CHARS: usize = 200;

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

fn verify_admin(state: &AppState, token: &str) -> bool {
    state
        .admin_token
        .as_ref()
        .is_some_and(|expected| expected.as_bytes().ct_eq(token.as_bytes()).into())
}

async fn verify_bot(state: &AppState, token: &str) -> Option<BotRecord> {
    let h = hash_token(token);
    bot_db::get_bot_by_token_hash(&state.db, &h)
        .await
        .ok()
        .flatten()
        .filter(|b| b.active)
}

async fn format_messages(db: &db::Db, rows: Vec<(i64, String)>, channel_id: i32) -> Vec<Value> {
    let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();

    let reaction_map = if ids.is_empty() {
        HashMap::new()
    } else {
        db::get_reactions_for_messages(db, &ids)
            .await
            .unwrap_or_default()
    };

    rows.into_iter()
        .map(|(id, content)| {
            let mut msg = serde_json::from_str::<Value>(&content).unwrap_or(Value::Null);
            msg["id"] = Value::from(id);
            if msg.get("channelId").is_none() {
                msg["channelId"] = Value::from(channel_id);
            }
            if let Some(reactions) = reaction_map.get(&id) {
                if let Ok(val) = serde_json::to_value(reactions) {
                    msg["reactions"] = val;
                }
            }
            if msg.get("reactions").is_none() {
                msg["reactions"] = Value::Object(Map::new());
            }
            msg
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Admin endpoints
// ---------------------------------------------------------------------------

async fn create_bot(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateBotRequest>,
) -> Response {
    if !verify_admin(&state, bearer.token()) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-admin-token");
    }

    let name = body.name.trim();
    if name.is_empty() || name.len() > 32 {
        return json_error(StatusCode::BAD_REQUEST, "invalid-bot-name");
    }
    if !security::validate_user_name(name) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-bot-name");
    }
    if body.description.len() > MAX_BOT_DESCRIPTION_LENGTH {
        return json_error(StatusCode::BAD_REQUEST, "description-too-long");
    }

    let permissions = body
        .permissions
        .as_ref()
        .map(|p| BotPermissions::from_list(p))
        .unwrap_or(BotPermissions::READ_MESSAGES | BotPermissions::SEND_MESSAGES);

    let id = generate_bot_id();
    let token = generate_token();
    let token_hash = hash_token(&token);

    match bot_db::create_bot(
        &state.db,
        &id,
        name,
        &token_hash,
        &body.owner_key,
        permissions,
        &body.description,
    )
    .await
    {
        Ok(record) => {
            let mut info = serde_json::to_value(record.to_info()).unwrap_or(Value::Null);
            info["token"] = Value::String(token);
            (StatusCode::CREATED, Json(serde_json::json!({"data": info}))).into_response()
        }
        Err(e) => {
            error!("Failed to create bot: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "bot-creation-failed")
        }
    }
}

async fn list_bots(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> Response {
    if !verify_admin(&state, bearer.token()) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-admin-token");
    }

    match bot_db::list_bots(&state.db).await {
        Ok(bots) => {
            let infos: Vec<_> = bots.iter().map(|b| b.to_info()).collect();
            Json(serde_json::json!({"data": infos})).into_response()
        }
        Err(e) => {
            error!("Failed to list bots: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "list-failed")
        }
    }
}

async fn get_bot(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(bot_id): Path<String>,
) -> Response {
    if !verify_admin(&state, bearer.token()) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-admin-token");
    }

    match bot_db::get_bot_by_id(&state.db, &bot_id).await {
        Ok(Some(bot)) => Json(serde_json::json!({"data": bot.to_info()})).into_response(),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "bot-not-found"),
        Err(e) => {
            error!("Failed to get bot {bot_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed")
        }
    }
}

async fn update_bot_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(bot_id): Path<String>,
    Json(body): Json<UpdateBotRequest>,
) -> Response {
    if !verify_admin(&state, bearer.token()) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-admin-token");
    }

    if let Some(ref n) = body.name {
        let n = n.trim();
        if n.is_empty() || n.len() > 32 || !security::validate_user_name(n) {
            return json_error(StatusCode::BAD_REQUEST, "invalid-bot-name");
        }
    }
    if let Some(ref d) = body.description {
        if d.len() > MAX_BOT_DESCRIPTION_LENGTH {
            return json_error(StatusCode::BAD_REQUEST, "description-too-long");
        }
    }

    let perms = body
        .permissions
        .as_ref()
        .map(|p| BotPermissions::from_list(p));

    match bot_db::update_bot(
        &state.db,
        &bot_id,
        body.name.as_deref(),
        perms,
        body.description.as_deref(),
        body.active,
    )
    .await
    {
        Ok(true) => match bot_db::get_bot_by_id(&state.db, &bot_id).await {
            Ok(Some(bot)) => Json(serde_json::json!({"data": bot.to_info()})).into_response(),
            _ => json_error(StatusCode::INTERNAL_SERVER_ERROR, "refetch-failed"),
        },
        Ok(false) => json_error(StatusCode::NOT_FOUND, "bot-not-found"),
        Err(e) => {
            error!("Failed to update bot {bot_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "update-failed")
        }
    }
}

async fn delete_bot_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(bot_id): Path<String>,
) -> Response {
    if !verify_admin(&state, bearer.token()) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-admin-token");
    }

    match bot_db::delete_bot(&state.db, &bot_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "bot-not-found"),
        Err(e) => {
            error!("Failed to delete bot {bot_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "delete-failed")
        }
    }
}

async fn reset_bot_token(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(bot_id): Path<String>,
) -> Response {
    if !verify_admin(&state, bearer.token()) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-admin-token");
    }

    let token = generate_token();
    let token_hash = hash_token(&token);

    match bot_db::update_bot_token(&state.db, &bot_id, &token_hash).await {
        Ok(true) => {
            Json(serde_json::json!({"data": {"token": token, "bot_id": bot_id}})).into_response()
        }
        Ok(false) => json_error(StatusCode::NOT_FOUND, "bot-not-found"),
        Err(e) => {
            error!("Failed to reset token for bot {bot_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "token-reset-failed")
        }
    }
}

// ---------------------------------------------------------------------------
// Bot API endpoints
// ---------------------------------------------------------------------------

async fn list_channels_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::READ_CHANNELS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:read_channels");
    }

    let channels = db::get_channels(&state.db).await;
    let data: Vec<Value> = channels
        .iter()
        .map(|ch| {
            serde_json::json!({
                "id": ch.id,
                "name": ch.name,
                "categoryId": ch.category_id,
            })
        })
        .collect();
    Json(serde_json::json!({"data": {"channels": data}})).into_response()
}

async fn get_messages(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
    Query(params): Query<MessageQuery>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::READ_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:read_messages");
    }

    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        return json_error(StatusCode::NOT_FOUND, "channel-not-found");
    }

    let limit = params
        .limit
        .unwrap_or(DEFAULT_MESSAGE_LIMIT)
        .clamp(1, MAX_MESSAGE_LIMIT);

    let rows = if let Some(after) = params.after {
        bot_db::fetch_messages_after(&state.db, channel_id, after, limit).await
    } else {
        db::fetch_history(&state.db, channel_id, params.before, limit).await
    };

    match rows {
        Ok(mut rows) => {
            if params.after.is_none() {
                rows.reverse();
            }
            let has_more = rows.len() as i64 == limit;
            let messages = format_messages(&state.db, rows, channel_id).await;
            Json(serde_json::json!({
                "data": {
                    "channelId": channel_id,
                    "messages": messages,
                    "has_more": has_more,
                }
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to fetch messages for channel {channel_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "fetch-failed")
        }
    }
}

async fn send_message(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
    Json(body): Json<SendMessageRequest>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::SEND_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:send_messages");
    }

    let rate_key = format!("bot::{}", bot.id);
    if !security::check_message_rate_limit(&state.rate_limiter, &rate_key).await {
        return json_error(StatusCode::TOO_MANY_REQUESTS, "rate-limit-exceeded");
    }

    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        return json_error(StatusCode::NOT_FOUND, "channel-not-found");
    }

    let text = body.text.trim();
    if text.is_empty() || text.len() > MAX_BOT_MESSAGE_LENGTH {
        return json_error(StatusCode::BAD_REQUEST, "invalid-message-text");
    }

    let now = Utc::now();
    let mut msg = serde_json::json!({
        "type": "chat",
        "user": bot.name,
        "text": text,
        "timestamp": now.to_rfc3339(),
        "channelId": channel_id,
        "bot": true,
        "reactions": {},
    });

    // Replies carry only the target id; the quoted snippet and thread root
    // are rebuilt from the stored message so bots cannot forge quotes or
    // attach messages to arbitrary threads (same rules as the WS handler).
    if let Some(target_id) = body.reply_to {
        match db::get_message_record(&state.db, target_id).await {
            Ok(Some(record)) if record.channel_id == channel_id => {
                let quoted_user = record
                    .content
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let quoted_text = ws::helpers::reply_preview(
                    record
                        .content
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or(""),
                    MAX_REPLY_PREVIEW_CHARS,
                );
                msg["replyTo"] = serde_json::json!({
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
                msg["threadId"] = Value::from(thread_root);
            }
            Ok(_) => return json_error(StatusCode::NOT_FOUND, "reply-target-not-found"),
            Err(e) => {
                error!("Failed to load reply target {target_id}: {e}");
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
            }
        }
    }

    let mut ephemeral_expiry: Option<DateTime<Utc>> = None;
    if body.ephemeral {
        let seconds = body
            .expires_in_seconds
            .unwrap_or(60)
            .clamp(MIN_EPHEMERAL_SECONDS, MAX_EPHEMERAL_SECONDS);
        let expiry = now + ChronoDuration::seconds(seconds);
        msg["ephemeral"] = serde_json::json!(true);
        msg["expiresAt"] = serde_json::json!(expiry.to_rfc3339());
        ephemeral_expiry = Some(expiry);
    }

    let content = msg.to_string();
    let id = match db::insert_message(&state.db, channel_id, &content).await {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to insert bot message: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "message-insert-failed");
        }
    };
    msg["id"] = serde_json::json!(id);

    let chan_tx = ws::helpers::get_or_create_channel(&state, channel_id).await;
    let _ = chan_tx.send(msg.to_string());

    // Announce the message globally so clients viewing other channels can
    // update unread counts, mirroring the WebSocket chat handler.
    let notify = serde_json::json!({
        "type": "message-notify",
        "channelId": channel_id,
        "id": id,
        "user": bot.name,
        "text": text,
    });
    let _ = state.tx.send(notify.to_string());

    if let Some(expiry) = ephemeral_expiry {
        ws::helpers::schedule_ephemeral_deletion(Arc::clone(&state), id, channel_id, expiry);
    }

    (StatusCode::CREATED, Json(serde_json::json!({"data": msg}))).into_response()
}

async fn delete_message_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id)): Path<(i32, i64)>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    let perms = BotPermissions(bot.permissions);

    let record = match db::get_message_record(&state.db, message_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message {message_id}: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if record.channel_id != channel_id {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    let is_own = record
        .content
        .get("user")
        .and_then(|u| u.as_str())
        .is_some_and(|u| u == bot.name);

    if is_own && !perms.has(BotPermissions::SEND_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:send_messages");
    }
    if !is_own && !perms.has(BotPermissions::MANAGE_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:manage_messages");
    }

    match db::delete_message(&state.db, message_id).await {
        Ok(true) => {
            let payload = serde_json::json!({
                "type": "message-deleted",
                "id": message_id,
                "channelId": channel_id,
            });
            let chan_tx = ws::helpers::get_or_create_channel(&state, channel_id).await;
            let _ = chan_tx.send(payload.to_string());
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to delete message {message_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "delete-failed")
        }
    }
}

async fn add_reaction_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id)): Path<(i32, i64)>,
    Json(body): Json<AddReactionRequest>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::ADD_REACTIONS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:add_reactions");
    }

    let emoji = body.emoji.trim();
    // Custom emoji shortcodes (`:name:`) may exceed the 16-byte cap that
    // bounds regular unicode reactions, mirroring the WS handler.
    let shortcode = ws::validation::is_emoji_shortcode(emoji);
    if !shortcode
        && (emoji.is_empty()
            || emoji.len() > 16
            || emoji.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        return json_error(StatusCode::BAD_REQUEST, "invalid-emoji");
    }

    // Shortcode reactions require the custom emoji to actually exist so junk
    // shortcodes cannot be planted.
    if shortcode {
        match db::emoji_exists(&state.db, emoji.trim_matches(':')).await {
            Ok(true) => {}
            Ok(false) => return json_error(StatusCode::BAD_REQUEST, "invalid-emoji"),
            Err(e) => {
                error!("db emoji lookup error: {e}");
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
            }
        }
    }

    let target_channel_id = match db::get_message_channel_id(&state.db, message_id).await {
        Ok(Some(ch)) => ch,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message channel for reaction: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if target_channel_id != channel_id {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    if let Err(e) = db::add_reaction(&state.db, message_id, &bot.name, emoji).await {
        error!("db reaction error: {e}");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
    }

    let reactions = match db::get_reaction_summary(&state.db, message_id).await {
        Ok(map) => map,
        Err(e) => {
            error!("db reaction summary error: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
        }
    };

    let payload = serde_json::json!({
        "type": "reaction-update",
        "channelId": channel_id,
        "messageId": message_id,
        "reactions": reactions,
    });
    let chan_tx = ws::helpers::get_or_create_channel(&state, channel_id).await;
    let _ = chan_tx.send(payload.to_string());

    Json(serde_json::json!({"data": {"messageId": message_id, "reactions": reactions}}))
        .into_response()
}

async fn remove_reaction_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id, emoji)): Path<(i32, i64, String)>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::ADD_REACTIONS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:add_reactions");
    }

    let target_channel_id = match db::get_message_channel_id(&state.db, message_id).await {
        Ok(Some(ch)) => ch,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message channel for reaction removal: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if target_channel_id != channel_id {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    if let Err(e) = db::remove_reaction(&state.db, message_id, &bot.name, &emoji).await {
        error!("db reaction removal error: {e}");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
    }

    let reactions = match db::get_reaction_summary(&state.db, message_id).await {
        Ok(map) => map,
        Err(e) => {
            error!("db reaction summary error: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
        }
    };

    let payload = serde_json::json!({
        "type": "reaction-update",
        "channelId": channel_id,
        "messageId": message_id,
        "reactions": reactions,
    });
    let chan_tx = ws::helpers::get_or_create_channel(&state, channel_id).await;
    let _ = chan_tx.send(payload.to_string());

    Json(serde_json::json!({"data": {"messageId": message_id, "reactions": reactions}}))
        .into_response()
}

async fn edit_message_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id)): Path<(i32, i64)>,
    Json(body): Json<EditMessageRequest>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::SEND_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:send_messages");
    }

    let new_text = body.text.trim();
    if new_text.is_empty() || new_text.len() > MAX_BOT_MESSAGE_LENGTH {
        return json_error(StatusCode::BAD_REQUEST, "invalid-message-text");
    }

    let record = match db::get_message_record(&state.db, message_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message {message_id} for edit: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if record.channel_id != channel_id {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    // Editing rewrites someone's words, so it is never extended to
    // `manage_messages` - a bot may only edit its own messages.
    let is_own = record
        .content
        .get("user")
        .and_then(|u| u.as_str())
        .is_some_and(|u| u == bot.name)
        && record
            .content
            .get("bot")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
    if !is_own {
        return json_error(StatusCode::FORBIDDEN, "not-message-author");
    }

    let mut content = record.content.clone();
    let edited_at = Utc::now().to_rfc3339();
    content["text"] = Value::String(new_text.to_string());
    content["edited"] = Value::Bool(true);
    content["editedAt"] = Value::String(edited_at.clone());

    let serialized = match serde_json::to_string(&content) {
        Ok(out) => out,
        Err(e) => {
            error!("Failed to serialize edited message {message_id}: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "edit-failed");
        }
    };

    match db::update_message_content(&state.db, message_id, &serialized).await {
        Ok(true) => {
            let payload = serde_json::json!({
                "type": "message-edited",
                "id": message_id,
                "channelId": channel_id,
                "text": new_text,
                "editedAt": edited_at,
            });
            let chan_tx = ws::helpers::get_or_create_channel(&state, channel_id).await;
            let _ = chan_tx.send(payload.to_string());

            content["id"] = Value::from(message_id);
            Json(serde_json::json!({"data": content})).into_response()
        }
        Ok(false) => json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to edit message {message_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "edit-failed")
        }
    }
}

async fn search_messages_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
    Query(params): Query<SearchQuery>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::READ_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:read_messages");
    }

    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        return json_error(StatusCode::NOT_FOUND, "channel-not-found");
    }

    let query = params.q.trim();
    if query.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "missing-query");
    }

    let limit = params
        .limit
        .unwrap_or(DEFAULT_MESSAGE_LIMIT)
        .clamp(1, MAX_SEARCH_RESULTS);

    match db::search_messages(&state.db, channel_id, query, limit).await {
        Ok(rows) => {
            let messages = format_messages(&state.db, rows, channel_id).await;
            Json(serde_json::json!({
                "data": {
                    "channelId": channel_id,
                    "query": query,
                    "messages": messages,
                }
            }))
            .into_response()
        }
        Err(e) => {
            error!("Bot search failed for channel {channel_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "search-failed")
        }
    }
}

async fn get_thread_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id)): Path<(i32, i64)>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::READ_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:read_messages");
    }

    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        return json_error(StatusCode::NOT_FOUND, "channel-not-found");
    }

    match db::fetch_thread(&state.db, channel_id, message_id, MAX_THREAD_MESSAGES).await {
        Ok(rows) => {
            if rows.is_empty() {
                return json_error(StatusCode::NOT_FOUND, "message-not-found");
            }
            let messages = format_messages(&state.db, rows, channel_id).await;
            Json(serde_json::json!({
                "data": {
                    "channelId": channel_id,
                    "rootId": message_id,
                    "messages": messages,
                }
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to load thread {message_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "thread-load-failed")
        }
    }
}

/// Broadcast the current pin snapshot for a channel, mirroring the WS handler.
async fn broadcast_pins(state: &Arc<AppState>, channel_id: i32) {
    match db::get_pins_for_channel(&state.db, channel_id).await {
        Ok(pins) => {
            let payload = serde_json::json!({
                "type": "pins",
                "channelId": channel_id,
                "pins": pins,
            });
            let chan_tx = ws::helpers::get_or_create_channel(state, channel_id).await;
            let _ = chan_tx.send(payload.to_string());
        }
        Err(e) => error!("Failed to load pins for channel {channel_id}: {e}"),
    }
}

async fn list_pins_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::READ_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:read_messages");
    }

    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        return json_error(StatusCode::NOT_FOUND, "channel-not-found");
    }

    match db::get_pins_for_channel(&state.db, channel_id).await {
        Ok(pins) => Json(serde_json::json!({
            "data": {"channelId": channel_id, "pins": pins}
        }))
        .into_response(),
        Err(e) => {
            error!("Failed to list pins for channel {channel_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "pin-list-failed")
        }
    }
}

async fn pin_message_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id)): Path<(i32, i64)>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::MANAGE_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:manage_messages");
    }

    let record = match db::get_message_record(&state.db, message_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message {message_id} for pinning: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if record.channel_id != channel_id {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    match db::add_pin(
        &state.db,
        message_id,
        channel_id,
        &bot.name,
        MAX_PINS_PER_CHANNEL,
    )
    .await
    {
        Ok(true) => {
            broadcast_pins(&state, channel_id).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => json_error(StatusCode::CONFLICT, "pin-limit-reached"),
        Err(e) => {
            error!("Failed to pin message {message_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "pin-failed")
        }
    }
}

async fn unpin_message_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id)): Path<(i32, i64)>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::MANAGE_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:manage_messages");
    }

    match db::remove_pin(&state.db, message_id).await {
        Ok(Some(pin_channel_id)) => {
            if pin_channel_id != channel_id {
                // The pin lived in a different channel; the removal already
                // happened, so broadcast where it actually was.
                broadcast_pins(&state, pin_channel_id).await;
            } else {
                broadcast_pins(&state, channel_id).await;
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => json_error(StatusCode::NOT_FOUND, "pin-not-found"),
        Err(e) => {
            error!("Failed to unpin message {message_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "pin-failed")
        }
    }
}

async fn create_channel_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateChannelRequest>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::MANAGE_CHANNELS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:manage_channels");
    }

    let name = body.name.trim();
    if !security::validate_channel_name(name) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-channel-name");
    }

    match db::add_channel(&state.db, name, body.category_id).await {
        Ok(Some(record)) => {
            ws::helpers::get_or_create_channel(&state, record.id).await;
            ws::helpers::broadcast_new_channel(&state, &record).await;
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "data": {
                        "id": record.id,
                        "name": record.name,
                        "categoryId": record.category_id,
                        "topic": record.description,
                    }
                })),
            )
                .into_response()
        }
        Ok(None) => json_error(StatusCode::CONFLICT, "channel-already-exists"),
        Err(e) => {
            error!("Bot failed to create channel: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "channel-creation-failed")
        }
    }
}

async fn update_channel_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
    Json(body): Json<UpdateChannelRequest>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::MANAGE_CHANNELS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:manage_channels");
    }

    let Some(raw_topic) = body.topic else {
        return json_error(StatusCode::BAD_REQUEST, "missing-topic");
    };
    let topic = raw_topic.trim();
    if !ws::validation::validate_channel_topic(topic) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-channel-topic");
    }

    match db::set_channel_description(&state.db, channel_id, topic).await {
        Ok(true) => {
            ws::helpers::broadcast_channel_topic(&state, channel_id, topic).await;
            Json(serde_json::json!({
                "data": {"channelId": channel_id, "topic": topic}
            }))
            .into_response()
        }
        Ok(false) => json_error(StatusCode::NOT_FOUND, "channel-not-found"),
        Err(e) => {
            error!("Bot failed to set topic for channel {channel_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "topic-update-failed")
        }
    }
}

async fn delete_channel_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::MANAGE_CHANNELS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:manage_channels");
    }

    let record = match db::get_channel_by_id(&state.db, channel_id).await {
        Some(r) => r,
        None => return json_error(StatusCode::NOT_FOUND, "channel-not-found"),
    };

    if record.name == "general" {
        return json_error(StatusCode::FORBIDDEN, "cannot-delete-general");
    }

    match db::remove_channel(&state.db, channel_id).await {
        Ok(()) => {
            state.channels.lock().await.remove(&channel_id);
            ws::helpers::broadcast_remove_channel(&state, channel_id).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            error!("Bot failed to delete channel {channel_id}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "channel-deletion-failed")
        }
    }
}

async fn typing_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::SEND_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:send_messages");
    }

    // Typing events are transient and never persisted. They share the message
    // rate-limit window but under a separate key, so a chatty typing loop
    // cannot exhaust the bot's message budget (and vice versa).
    let rate_key = format!("bot-typing::{}", bot.id);
    if !security::check_message_rate_limit(&state.rate_limiter, &rate_key).await {
        return json_error(StatusCode::TOO_MANY_REQUESTS, "rate-limit-exceeded");
    }

    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        return json_error(StatusCode::NOT_FOUND, "channel-not-found");
    }

    let payload = serde_json::json!({
        "type": "typing",
        "user": bot.name,
        "channelId": channel_id,
    });
    let chan_tx = ws::helpers::get_or_create_channel(&state, channel_id).await;
    let _ = chan_tx.send(payload.to_string());

    StatusCode::NO_CONTENT.into_response()
}

async fn list_emojis_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> Response {
    if verify_bot(&state, bearer.token()).await.is_none() {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token");
    }

    match db::get_emojis(&state.db).await {
        Ok(emojis) => {
            let data: Vec<Value> = emojis
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "name": e.name,
                        "url": e.url,
                        "uploadedBy": e.uploaded_by,
                        "createdAt": e.created_at,
                    })
                })
                .collect();
            Json(serde_json::json!({"data": {"emojis": data}})).into_response()
        }
        Err(e) => {
            error!("Failed to list emojis for bot: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "emoji-list-failed")
        }
    }
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::READ_USERS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:read_users");
    }

    let (online, all) = ws::helpers::get_user_lists(&state).await;
    let statuses: HashMap<String, String> = state.statuses.lock().await.clone();

    Json(serde_json::json!({
        "data": {
            "online": online,
            "all": all,
            "statuses": statuses,
        }
    }))
    .into_response()
}

async fn server_info(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> Response {
    if verify_bot(&state, bearer.token()).await.is_none() {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token");
    }

    let online_count = state.users.lock().await.len();
    let channels = db::get_channels(&state.db).await;
    let data: Vec<Value> = channels
        .iter()
        .map(|ch| {
            serde_json::json!({
                "id": ch.id,
                "name": ch.name,
                "categoryId": ch.category_id,
            })
        })
        .collect();

    Json(serde_json::json!({
        "data": {
            "version": env!("CARGO_PKG_VERSION"),
            "bot_api_version": "1",
            "online_users": online_count,
            "channels": data,
            "has_password": state.password.is_some(),
        }
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // Admin bot management
        .route("/api/v1/bots", post(create_bot).get(list_bots))
        .route(
            "/api/v1/bots/{bot_id}",
            get(get_bot)
                .patch(update_bot_handler)
                .delete(delete_bot_handler),
        )
        .route("/api/v1/bots/{bot_id}/reset-token", post(reset_bot_token))
        // Bot API
        .route(
            "/api/v1/channels",
            get(list_channels_handler).post(create_channel_handler),
        )
        .route(
            "/api/v1/channels/{channel_id}",
            axum::routing::patch(update_channel_handler).delete(delete_channel_handler),
        )
        .route(
            "/api/v1/channels/{channel_id}/messages",
            get(get_messages).post(send_message),
        )
        .route(
            "/api/v1/channels/{channel_id}/messages/search",
            get(search_messages_handler),
        )
        .route(
            "/api/v1/channels/{channel_id}/messages/{message_id}",
            delete(delete_message_handler).patch(edit_message_handler),
        )
        .route(
            "/api/v1/channels/{channel_id}/messages/{message_id}/thread",
            get(get_thread_handler),
        )
        .route(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions",
            post(add_reaction_handler),
        )
        .route(
            "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
            delete(remove_reaction_handler),
        )
        .route("/api/v1/channels/{channel_id}/pins", get(list_pins_handler))
        .route(
            "/api/v1/channels/{channel_id}/pins/{message_id}",
            put(pin_message_handler).delete(unpin_message_handler),
        )
        .route("/api/v1/channels/{channel_id}/typing", post(typing_handler))
        .route("/api/v1/emojis", get(list_emojis_handler))
        .route("/api/v1/users", get(list_users))
        .route("/api/v1/server/info", get(server_info))
}
