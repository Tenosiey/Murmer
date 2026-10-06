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

use crate::channel_overrides::ChannelKind;
use crate::ws::constants::{
    DEFAULT_HISTORY_LIMIT, MAX_EPHEMERAL_SECONDS, MAX_PINS_PER_CHANNEL, MAX_REPLY_PREVIEW_CHARS,
    MAX_SEARCH_RESULTS, MAX_THREAD_MESSAGES, MIN_EPHEMERAL_SECONDS,
};
use crate::{AppState, db, security, ws};
use axum::{
    Router,
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::error;

use super::{
    db as bot_db,
    models::{
        AddReactionRequest, BotPermissions, BotRecord, CreateBotRequest, CreateChannelRequest,
        EditMessageRequest, MessageQuery, SearchQuery, SendMessageRequest, UpdateBotRequest,
        UpdateChannelRequest, generate_bot_id, generate_token, hash_token,
    },
};

const MAX_BOT_MESSAGE_LENGTH: usize = 4000;
const MAX_BOT_DESCRIPTION_LENGTH: usize = 256;

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

/// Authenticate a bot token and check it holds `perm`, or produce the
/// rejection to return.
async fn require_bot(
    state: &AppState,
    bearer: &Bearer,
    perm: i32,
) -> Result<BotRecord, Box<Response>> {
    let Some(bot) = verify_bot(state, bearer.token()).await else {
        return Err(Box::new(json_error(
            StatusCode::UNAUTHORIZED,
            "invalid-bot-token",
        )));
    };
    if !BotPermissions(bot.permissions).has(perm) {
        let code = format!("missing-permission:{}", BotPermissions::name(perm));
        return Err(Box::new(json_error(StatusCode::FORBIDDEN, &code)));
    }
    Ok(bot)
}

/// [`require_bot`] for an endpoint scoped to one text channel, which must
/// exist and be visible to the bot. Bots hold no roles, so that means visible
/// to `@everyone`: a private channel is out of reach of every bot. A hidden
/// channel answers exactly like a missing one, so its id reveals nothing.
async fn require_bot_in(
    state: &Arc<AppState>,
    bearer: &Bearer,
    channel_id: i32,
    perm: i32,
) -> Result<BotRecord, Box<Response>> {
    let bot = require_bot(state, bearer, perm).await?;
    if db::get_channel_by_id(&state.db, channel_id).await.is_none()
        || !ws::helpers::can_view_channel(state, &bot.name, ChannelKind::Text, channel_id).await
    {
        return Err(Box::new(json_error(
            StatusCode::NOT_FOUND,
            "channel-not-found",
        )));
    }
    Ok(bot)
}

/// Refuse a bot name that an account or another bot already uses. A bot
/// sharing a member's name would share everything keyed by it in memory —
/// their roles first of all.
async fn check_bot_name_free(
    state: &AppState,
    name: &str,
    except_id: Option<&str>,
) -> Result<(), Box<Response>> {
    let taken = match (
        db::get_user_key(&state.db, name).await,
        bot_db::bot_name_taken(&state.db, name, except_id).await,
    ) {
        (Ok(key), Ok(bot)) => key.is_some() || bot,
        (Err(e), _) | (_, Err(e)) => {
            error!("Failed to check whether bot name {name} is free: {e}");
            return Err(Box::new(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "name-check-failed",
            )));
        }
    };
    if taken {
        return Err(Box::new(json_error(StatusCode::CONFLICT, "name-taken")));
    }
    Ok(())
}

/// The text channels `bot` may see; see [`require_bot_in`].
async fn visible_channels(state: &Arc<AppState>, bot: &str) -> Vec<db::ChannelRecord> {
    let mut visible = Vec::new();
    for ch in db::get_channels(&state.db).await {
        if ws::helpers::can_view_channel(state, bot, ChannelKind::Text, ch.id).await {
            visible.push(ch);
        }
    }
    visible
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
    if !security::validate_user_name(name) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-bot-name");
    }
    if let Err(denied) = check_bot_name_free(&state, name, None).await {
        return *denied;
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
        if !security::validate_user_name(n) {
            return json_error(StatusCode::BAD_REQUEST, "invalid-bot-name");
        }
        if let Err(denied) = check_bot_name_free(&state, n, Some(&bot_id)).await {
            return *denied;
        }
    }
    if let Some(ref d) = body.description
        && d.len() > MAX_BOT_DESCRIPTION_LENGTH
    {
        return json_error(StatusCode::BAD_REQUEST, "description-too-long");
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
    let bot = match require_bot(&state, &bearer, BotPermissions::READ_CHANNELS).await {
        Ok(bot) => bot,
        Err(denied) => return *denied,
    };

    let data: Vec<Value> = visible_channels(&state, &bot.name)
        .await
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
    if let Err(denied) =
        require_bot_in(&state, &bearer, channel_id, BotPermissions::READ_MESSAGES).await
    {
        return *denied;
    }

    let limit = ws::validation::history_limit(params.limit);

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
            let messages = db::hydrate_messages(&state.db, rows, channel_id).await;
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
    let bot = match require_bot_in(&state, &bearer, channel_id, BotPermissions::SEND_MESSAGES).await
    {
        Ok(bot) => bot,
        Err(denied) => return *denied,
    };

    let rate_key = format!("bot::{}", bot.id);
    if !security::check_message_rate_limit(&state.rate_limiter, &rate_key).await {
        return json_error(StatusCode::TOO_MANY_REQUESTS, "rate-limit-exceeded");
    }

    match db::get_channel_by_id(&state.db, channel_id).await {
        None => return json_error(StatusCode::NOT_FOUND, "channel-not-found"),
        // Bots have no identity key, so they are not on an encrypted channel's
        // key roster and have nothing to encrypt with. Posting plaintext there
        // would put on the server exactly what the channel exists to keep off
        // it, so the endpoint refuses instead.
        Some(record) if record.e2ee => {
            return json_error(StatusCode::FORBIDDEN, "channel-requires-encryption");
        }
        Some(_) => {}
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

    ws::helpers::send_to_channel(&state, channel_id, &msg).await;

    // Announce the message globally so clients viewing other channels can
    // update unread counts, mirroring the WebSocket chat handler.
    let notify = serde_json::json!({
        "type": "message-notify",
        "channelId": channel_id,
        "id": id,
        "user": bot.name,
        "text": text,
    });
    ws::helpers::broadcast(&state, &notify);

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
    // Which permission applies depends on whose message it is, so only the
    // token and the channel are checked up front.
    let bot = match require_bot_in(&state, &bearer, channel_id, 0).await {
        Ok(bot) => bot,
        Err(denied) => return *denied,
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
            ws::helpers::send_to_channel(&state, channel_id, &payload).await;
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
    let bot = match require_bot_in(&state, &bearer, channel_id, BotPermissions::ADD_REACTIONS).await
    {
        Ok(bot) => bot,
        Err(denied) => return *denied,
    };

    let emoji = body.emoji.trim();
    if !ws::validation::is_valid_reaction_key(emoji) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-emoji");
    }

    // Shortcode reactions require the custom emoji to actually exist so junk
    // shortcodes cannot be planted.
    if ws::validation::is_emoji_shortcode(emoji) {
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

    match db::add_reaction(&state.db, message_id, &bot.name, emoji).await {
        Ok(true) => {}
        Ok(false) => return json_error(StatusCode::CONFLICT, "reaction-limit"),
        Err(e) => {
            error!("db reaction error: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
        }
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
    ws::helpers::send_to_channel(&state, channel_id, &payload).await;

    Json(serde_json::json!({"data": {"messageId": message_id, "reactions": reactions}}))
        .into_response()
}

async fn remove_reaction_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id, emoji)): Path<(i32, i64, String)>,
) -> Response {
    let bot = match require_bot_in(&state, &bearer, channel_id, BotPermissions::ADD_REACTIONS).await
    {
        Ok(bot) => bot,
        Err(denied) => return *denied,
    };

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
    ws::helpers::send_to_channel(&state, channel_id, &payload).await;

    Json(serde_json::json!({"data": {"messageId": message_id, "reactions": reactions}}))
        .into_response()
}

async fn edit_message_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel_id, message_id)): Path<(i32, i64)>,
    Json(body): Json<EditMessageRequest>,
) -> Response {
    let bot = match require_bot_in(&state, &bearer, channel_id, BotPermissions::SEND_MESSAGES).await
    {
        Ok(bot) => bot,
        Err(denied) => return *denied,
    };

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
            ws::helpers::send_to_channel(&state, channel_id, &payload).await;

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
    if let Err(denied) =
        require_bot_in(&state, &bearer, channel_id, BotPermissions::READ_MESSAGES).await
    {
        return *denied;
    }

    let query = params.q.trim();
    if query.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "missing-query");
    }

    let limit = params
        .limit
        .unwrap_or(DEFAULT_HISTORY_LIMIT)
        .clamp(1, MAX_SEARCH_RESULTS);

    match db::search_messages(
        &state.db,
        channel_id,
        query,
        &db::SearchFilters::default(),
        limit,
    )
    .await
    {
        Ok(rows) => {
            let messages = db::hydrate_messages(&state.db, rows, channel_id).await;
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
    if let Err(denied) =
        require_bot_in(&state, &bearer, channel_id, BotPermissions::READ_MESSAGES).await
    {
        return *denied;
    }

    match db::fetch_thread(&state.db, channel_id, message_id, MAX_THREAD_MESSAGES).await {
        Ok(rows) => {
            if rows.is_empty() {
                return json_error(StatusCode::NOT_FOUND, "message-not-found");
            }
            let messages = db::hydrate_messages(&state.db, rows, channel_id).await;
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

async fn list_pins_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel_id): Path<i32>,
) -> Response {
    if let Err(denied) =
        require_bot_in(&state, &bearer, channel_id, BotPermissions::READ_MESSAGES).await
    {
        return *denied;
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
    let bot =
        match require_bot_in(&state, &bearer, channel_id, BotPermissions::MANAGE_MESSAGES).await {
            Ok(bot) => bot,
            Err(denied) => return *denied,
        };

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
            ws::broadcast_pins(&state, channel_id).await;
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
    if let Err(denied) =
        require_bot_in(&state, &bearer, channel_id, BotPermissions::MANAGE_MESSAGES).await
    {
        return *denied;
    }

    // The pin is removed by message id alone, so the message must be shown
    // to live in the channel just vetted, not in one the bot cannot see.
    match db::get_message_channel_id(&state.db, message_id).await {
        Ok(Some(ch)) if ch == channel_id => {}
        Ok(_) => return json_error(StatusCode::NOT_FOUND, "pin-not-found"),
        Err(e) => {
            error!("Failed to look up message channel for unpin: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    }

    match db::remove_pin(&state.db, message_id).await {
        Ok(Some(pin_channel_id)) => {
            ws::broadcast_pins(&state, pin_channel_id).await;
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
    if let Err(denied) = require_bot(&state, &bearer, BotPermissions::MANAGE_CHANNELS).await {
        return *denied;
    }

    let name = body.name.trim();
    if !security::validate_channel_name(name) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-channel-name");
    }

    match db::add_channel(&state.db, name, body.category_id).await {
        Ok(Some(record)) => {
            ws::helpers::get_or_create_channel(&state, record.id).await;
            ws::helpers::broadcast_new_channel(&state, &record);
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
    if let Err(denied) =
        require_bot_in(&state, &bearer, channel_id, BotPermissions::MANAGE_CHANNELS).await
    {
        return *denied;
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
            ws::helpers::broadcast_channel_topic(&state, channel_id, topic);
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
    if let Err(denied) =
        require_bot_in(&state, &bearer, channel_id, BotPermissions::MANAGE_CHANNELS).await
    {
        return *denied;
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
            ws::helpers::broadcast_remove_channel(&state, channel_id);
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
    let bot = match require_bot_in(&state, &bearer, channel_id, BotPermissions::SEND_MESSAGES).await
    {
        Ok(bot) => bot,
        Err(denied) => return *denied,
    };

    // Typing events are transient and never persisted. They share the message
    // rate-limit window but under a separate key, so a chatty typing loop
    // cannot exhaust the bot's message budget (and vice versa).
    let rate_key = format!("bot-typing::{}", bot.id);
    if !security::check_message_rate_limit(&state.rate_limiter, &rate_key).await {
        return json_error(StatusCode::TOO_MANY_REQUESTS, "rate-limit-exceeded");
    }

    let payload = serde_json::json!({
        "type": "typing",
        "user": bot.name,
        "channelId": channel_id,
    });
    ws::helpers::send_to_channel(&state, channel_id, &payload).await;

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
    if let Err(denied) = require_bot(&state, &bearer, BotPermissions::READ_USERS).await {
        return *denied;
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
    let Some(bot) = verify_bot(&state, bearer.token()).await else {
        return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token");
    };

    let online_count = state.users.lock().await.len();
    let data: Vec<Value> = visible_channels(&state, &bot.name)
        .await
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
