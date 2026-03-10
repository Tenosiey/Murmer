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
//!   - `GET    /api/v1/channels`                                   – list channels
//!   - `GET    /api/v1/channels/:channel/messages`                 – read messages
//!   - `POST   /api/v1/channels/:channel/messages`                 – send message
//!   - `DELETE /api/v1/channels/:channel/messages/:message_id`     – delete message
//!   - `POST   /api/v1/channels/:channel/messages/:message_id/reactions`          – add reaction
//!   - `DELETE /api/v1/channels/:channel/messages/:message_id/reactions/:emoji`   – remove reaction
//!   - `GET    /api/v1/users`                                      – list users
//!   - `GET    /api/v1/server/info`                                – server metadata

use crate::{db, security, ws, AppState};
use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
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
        CreateBotRequest, MessageQuery, SendMessageRequest, UpdateBotRequest,
    },
};

const MAX_BOT_MESSAGE_LENGTH: usize = 4000;
const MAX_BOT_DESCRIPTION_LENGTH: usize = 256;
const MIN_EPHEMERAL_SECONDS: i64 = 5;
const MAX_EPHEMERAL_SECONDS: i64 = 86_400;
const DEFAULT_MESSAGE_LIMIT: i64 = 50;
const MAX_MESSAGE_LIMIT: i64 = 200;

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

fn verify_admin(state: &AppState, token: &str) -> bool {
    state.admin_token.as_ref().map_or(false, |expected| {
        expected.as_bytes().ct_eq(token.as_bytes()).into()
    })
}

async fn verify_bot(state: &AppState, token: &str) -> Option<BotRecord> {
    let h = hash_token(token);
    bot_db::get_bot_by_token_hash(&state.db, &h)
        .await
        .ok()
        .flatten()
        .filter(|b| b.active)
}

async fn format_messages(
    db: &tokio_postgres::Client,
    rows: Vec<(i64, String)>,
    channel: &str,
) -> Vec<Value> {
    let ids32: Vec<i32> = rows
        .iter()
        .filter_map(|(id, _)| i32::try_from(*id).ok())
        .collect();

    let reaction_map = if ids32.is_empty() {
        HashMap::new()
    } else {
        db::get_reactions_for_messages(db, &ids32)
            .await
            .unwrap_or_default()
    };

    rows.into_iter()
        .map(|(id, content)| {
            let mut msg = serde_json::from_str::<Value>(&content).unwrap_or(Value::Null);
            msg["id"] = Value::from(id);
            if msg.get("channel").and_then(|c| c.as_str()).is_none() {
                msg["channel"] = Value::String(channel.to_string());
            }
            if let Ok(id32) = i32::try_from(id) {
                if let Some(reactions) = reaction_map.get(&id32) {
                    if let Ok(val) = serde_json::to_value(reactions) {
                        msg["reactions"] = val;
                    }
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
    Json(serde_json::json!({"data": {"channels": channels}})).into_response()
}

async fn get_messages(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel): Path<String>,
    Query(params): Query<MessageQuery>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::READ_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:read_messages");
    }

    if !security::validate_channel_name(&channel) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-channel-name");
    }

    let channels = db::get_channels(&state.db).await;
    if !channels.contains(&channel) {
        return json_error(StatusCode::NOT_FOUND, "channel-not-found");
    }

    let limit = params
        .limit
        .unwrap_or(DEFAULT_MESSAGE_LIMIT)
        .clamp(1, MAX_MESSAGE_LIMIT);

    let rows = if let Some(after) = params.after {
        bot_db::fetch_messages_after(&state.db, &channel, after, limit).await
    } else {
        db::fetch_history(&state.db, &channel, params.before, limit).await
    };

    match rows {
        Ok(mut rows) => {
            // fetch_history returns DESC order; normalize to ASC
            if params.after.is_none() {
                rows.reverse();
            }
            let has_more = rows.len() as i64 == limit;
            let messages = format_messages(&state.db, rows, &channel).await;
            Json(serde_json::json!({
                "data": {
                    "channel": channel,
                    "messages": messages,
                    "has_more": has_more,
                }
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to fetch messages for channel {channel}: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "fetch-failed")
        }
    }
}

async fn send_message(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(channel): Path<String>,
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

    if !security::validate_channel_name(&channel) {
        return json_error(StatusCode::BAD_REQUEST, "invalid-channel-name");
    }

    let channels = db::get_channels(&state.db).await;
    if !channels.contains(&channel) {
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
        "time": now.format("%H:%M:%S").to_string(),
        "channel": channel,
        "bot": true,
        "reactions": {},
    });

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
    let row = match state
        .db
        .query_one(
            "INSERT INTO messages (channel, content) VALUES ($1, $2) RETURNING id::bigint",
            &[&channel, &content],
        )
        .await
    {
        Ok(row) => row,
        Err(e) => {
            error!("Failed to insert bot message: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "message-insert-failed");
        }
    };

    let id: i64 = row.get(0);
    msg["id"] = serde_json::json!(id);

    let chan_tx = ws::helpers::get_or_create_channel(&state, &channel).await;
    let _ = chan_tx.send(msg.to_string());

    if let Some(expiry) = ephemeral_expiry {
        let state_clone = Arc::clone(&state);
        let channel_clone = channel.clone();
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
                            "channel": channel_clone,
                        });
                        let chan =
                            ws::helpers::get_or_create_channel(&state_clone, &channel_clone).await;
                        let _ = chan.send(payload.to_string());
                    }
                    Ok(false) => {}
                    Err(e) => error!("failed to delete ephemeral bot message {id}: {e}"),
                }
            }
        });
    }

    (StatusCode::CREATED, Json(serde_json::json!({"data": msg}))).into_response()
}

async fn delete_message_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel, message_id)): Path<(String, i64)>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    let perms = BotPermissions(bot.permissions);

    let message_id32 = match i32::try_from(message_id) {
        Ok(v) => v,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid-message-id"),
    };

    let record = match db::get_message_record(&state.db, message_id32).await {
        Ok(Some(r)) => r,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message {message_id}: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if record.channel != channel {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    let is_own = record
        .content
        .get("user")
        .and_then(|u| u.as_str())
        .map_or(false, |u| u == bot.name);

    if is_own && !perms.has(BotPermissions::SEND_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:send_messages");
    }
    if !is_own && !perms.has(BotPermissions::MANAGE_MESSAGES) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:manage_messages");
    }

    match db::delete_message(&state.db, message_id32).await {
        Ok(true) => {
            let payload = serde_json::json!({
                "type": "message-deleted",
                "id": message_id,
                "channel": channel,
            });
            let chan_tx = ws::helpers::get_or_create_channel(&state, &channel).await;
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
    Path((channel, message_id)): Path<(String, i64)>,
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
    if emoji.is_empty()
        || emoji.len() > 16
        || emoji.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return json_error(StatusCode::BAD_REQUEST, "invalid-emoji");
    }

    let message_id32 = match i32::try_from(message_id) {
        Ok(v) => v,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid-message-id"),
    };

    let target_channel = match db::get_message_channel(&state.db, message_id32).await {
        Ok(Some(ch)) => ch,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message channel for reaction: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if target_channel != channel {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    if let Err(e) = db::add_reaction(&state.db, message_id32, &bot.name, emoji).await {
        error!("db reaction error: {e}");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
    }

    let reactions = match db::get_reaction_summary(&state.db, message_id32).await {
        Ok(map) => map,
        Err(e) => {
            error!("db reaction summary error: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
        }
    };

    let payload = serde_json::json!({
        "type": "reaction-update",
        "channel": channel,
        "messageId": message_id,
        "reactions": reactions,
    });
    let chan_tx = ws::helpers::get_or_create_channel(&state, &channel).await;
    let _ = chan_tx.send(payload.to_string());

    Json(serde_json::json!({"data": {"messageId": message_id, "reactions": reactions}}))
        .into_response()
}

async fn remove_reaction_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((channel, message_id, emoji)): Path<(String, i64, String)>,
) -> Response {
    let bot = match verify_bot(&state, bearer.token()).await {
        Some(b) => b,
        None => return json_error(StatusCode::UNAUTHORIZED, "invalid-bot-token"),
    };

    if !BotPermissions(bot.permissions).has(BotPermissions::ADD_REACTIONS) {
        return json_error(StatusCode::FORBIDDEN, "missing-permission:add_reactions");
    }

    let message_id32 = match i32::try_from(message_id) {
        Ok(v) => v,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid-message-id"),
    };

    let target_channel = match db::get_message_channel(&state.db, message_id32).await {
        Ok(Some(ch)) => ch,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "message-not-found"),
        Err(e) => {
            error!("Failed to look up message channel for reaction removal: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup-failed");
        }
    };

    if target_channel != channel {
        return json_error(StatusCode::NOT_FOUND, "message-not-found");
    }

    if let Err(e) = db::remove_reaction(&state.db, message_id32, &bot.name, &emoji).await {
        error!("db reaction removal error: {e}");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
    }

    let reactions = match db::get_reaction_summary(&state.db, message_id32).await {
        Ok(map) => map,
        Err(e) => {
            error!("db reaction summary error: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "reaction-failed");
        }
    };

    let payload = serde_json::json!({
        "type": "reaction-update",
        "channel": channel,
        "messageId": message_id,
        "reactions": reactions,
    });
    let chan_tx = ws::helpers::get_or_create_channel(&state, &channel).await;
    let _ = chan_tx.send(payload.to_string());

    Json(serde_json::json!({"data": {"messageId": message_id, "reactions": reactions}}))
        .into_response()
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

    Json(serde_json::json!({
        "data": {
            "version": env!("CARGO_PKG_VERSION"),
            "bot_api_version": "1",
            "online_users": online_count,
            "channels": channels,
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
            "/api/v1/bots/:bot_id",
            get(get_bot)
                .patch(update_bot_handler)
                .delete(delete_bot_handler),
        )
        .route("/api/v1/bots/:bot_id/reset-token", post(reset_bot_token))
        // Bot API
        .route("/api/v1/channels", get(list_channels_handler))
        .route(
            "/api/v1/channels/:channel/messages",
            get(get_messages).post(send_message),
        )
        .route(
            "/api/v1/channels/:channel/messages/:message_id",
            delete(delete_message_handler),
        )
        .route(
            "/api/v1/channels/:channel/messages/:message_id/reactions",
            post(add_reaction_handler),
        )
        .route(
            "/api/v1/channels/:channel/messages/:message_id/reactions/:emoji",
            delete(remove_reaction_handler),
        )
        .route("/api/v1/users", get(list_users))
        .route("/api/v1/server/info", get(server_info))
}
