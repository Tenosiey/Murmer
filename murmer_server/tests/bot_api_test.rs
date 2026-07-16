//! HTTP-level integration tests for the bot REST API, covering reactions,
//! replies/threads, message editing, pins, channel management, typing and
//! custom emoji listing.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use murmer_server::{AppState, RateLimiter, bot, db};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};
use tower::ServiceExt;

const ADMIN_TOKEN: &str = "test-admin-token";

async fn make_app() -> (Router, Arc<AppState>) {
    let database = db::init(":memory:").await.expect("in-memory db");
    let (tx, _) = broadcast::channel(64);
    let state = Arc::new(AppState {
        tx,
        channels: Arc::new(Mutex::new(HashMap::new())),
        db: database,
        users: Arc::new(Mutex::new(Default::default())),
        known_users: Arc::new(Mutex::new(Default::default())),
        voice_channels: Arc::new(Mutex::new(HashMap::new())),
        roles: Arc::new(Mutex::new(HashMap::new())),
        statuses: Arc::new(Mutex::new(HashMap::new())),
        user_keys: Arc::new(Mutex::new(HashMap::new())),
        mutes: Arc::new(Mutex::new(HashMap::new())),
        active_screen_shares: Arc::new(Mutex::new(HashMap::new())),
        voice_mutes: Arc::new(Mutex::new(HashMap::new())),
        connection_stats: Arc::new(Mutex::new(HashMap::new())),
        voice_session_starts: Arc::new(Mutex::new(HashMap::new())),
        screenshare_session_starts: Arc::new(Mutex::new(HashMap::new())),
        upload_dir: PathBuf::from("uploads"),
        password: None,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        rate_limiter: RateLimiter::new(),
    });
    (bot::routes::router().with_state(Arc::clone(&state)), state)
}

async fn request(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let body = match body {
        Some(v) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

/// Create a bot with the given permissions and return its token.
async fn create_bot(app: &Router, name: &str, permissions: &[&str]) -> String {
    let (status, body) = request(
        app,
        "POST",
        "/api/v1/bots",
        ADMIN_TOKEN,
        Some(json!({"name": name, "permissions": permissions})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "bot creation failed: {body}");
    body["data"]["token"].as_str().expect("token").to_string()
}

const ALL_PERMS: &[&str] = &[
    "read_messages",
    "send_messages",
    "manage_messages",
    "add_reactions",
    "read_channels",
    "manage_channels",
    "read_users",
];

async fn general_channel_id(state: &AppState) -> i32 {
    db::get_channel_id_by_name(&state.db, "general")
        .await
        .expect("default channel")
}

async fn send_text(app: &Router, token: &str, channel: i32, text: &str) -> i64 {
    let (status, body) = request(
        app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages"),
        token,
        Some(json!({"text": text})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "send failed: {body}");
    body["data"]["id"].as_i64().expect("message id")
}

#[tokio::test]
async fn reactions_roundtrip_and_shortcode_validation() {
    let (app, state) = make_app().await;
    let token = create_bot(&app, "ReactBot", ALL_PERMS).await;
    let channel = general_channel_id(&state).await;
    let msg = send_text(&app, &token, channel, "react to me").await;

    // Add a unicode reaction.
    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages/{msg}/reactions"),
        &token,
        Some(json!({"emoji": "👍"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["reactions"]["👍"][0], "ReactBot");

    // A shortcode for a non-existent custom emoji is rejected.
    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages/{msg}/reactions"),
        &token,
        Some(json!({"emoji": ":does_not_exist:"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid-emoji");

    // A shortcode for a registered custom emoji is accepted.
    db::add_emoji(&state.db, "party_parrot", "/uploads/p.png", "alice")
        .await
        .expect("register emoji");
    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages/{msg}/reactions"),
        &token,
        Some(json!({"emoji": ":party_parrot:"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Remove the unicode reaction again.
    let (status, body) = request(
        &app,
        "DELETE",
        &format!("/api/v1/channels/{channel}/messages/{msg}/reactions/👍"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["reactions"].get("👍").is_none());

    // Without the permission the request is rejected.
    let plain = create_bot(&app, "PlainBot", &["read_messages", "send_messages"]).await;
    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages/{msg}/reactions"),
        &plain,
        Some(json!({"emoji": "🎉"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "missing-permission:add_reactions");
}

#[tokio::test]
async fn replies_build_threads() {
    let (app, state) = make_app().await;
    let token = create_bot(&app, "ThreadBot", ALL_PERMS).await;
    let channel = general_channel_id(&state).await;

    let root = send_text(&app, &token, channel, "thread root").await;

    // Reply to the root.
    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages"),
        &token,
        Some(json!({"text": "first reply", "reply_to": root})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["replyTo"]["id"], root);
    assert_eq!(body["data"]["replyTo"]["text"], "thread root");
    assert_eq!(body["data"]["threadId"], root);
    let reply_id = body["data"]["id"].as_i64().expect("reply id");

    // Replying to a reply joins the same thread instead of nesting.
    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages"),
        &token,
        Some(json!({"text": "second reply", "reply_to": reply_id})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["threadId"], root);

    // The thread endpoint returns root + both replies.
    let (status, body) = request(
        &app,
        "GET",
        &format!("/api/v1/channels/{channel}/messages/{root}/thread"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["messages"].as_array().expect("array").len(), 3);

    // Replying to a missing message fails.
    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/messages"),
        &token,
        Some(json!({"text": "dangling", "reply_to": 99999})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "reply-target-not-found");
}

#[tokio::test]
async fn edit_only_own_messages() {
    let (app, state) = make_app().await;
    let token = create_bot(&app, "EditBot", ALL_PERMS).await;
    let channel = general_channel_id(&state).await;
    let own = send_text(&app, &token, channel, "tpyo").await;

    let (status, body) = request(
        &app,
        "PATCH",
        &format!("/api/v1/channels/{channel}/messages/{own}"),
        &token,
        Some(json!({"text": "typo fixed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["text"], "typo fixed");
    assert_eq!(body["data"]["edited"], true);

    // A user's message (not sent by the bot) cannot be edited, even with
    // manage_messages.
    let user_msg = json!({"type": "chat", "user": "alice", "text": "hi"}).to_string();
    let foreign = db::insert_message(&state.db, channel, &user_msg)
        .await
        .expect("insert user message");
    let (status, body) = request(
        &app,
        "PATCH",
        &format!("/api/v1/channels/{channel}/messages/{foreign}"),
        &token,
        Some(json!({"text": "hacked"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "not-message-author");
}

#[tokio::test]
async fn pins_lifecycle_and_permissions() {
    let (app, state) = make_app().await;
    let token = create_bot(&app, "PinBot", ALL_PERMS).await;
    let channel = general_channel_id(&state).await;
    let msg = send_text(&app, &token, channel, "important announcement").await;

    // Pinning needs manage_messages.
    let plain = create_bot(&app, "NoPinBot", &["read_messages", "send_messages"]).await;
    let (status, _) = request(
        &app,
        "PUT",
        &format!("/api/v1/channels/{channel}/pins/{msg}"),
        &plain,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = request(
        &app,
        "PUT",
        &format!("/api/v1/channels/{channel}/pins/{msg}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = request(
        &app,
        "GET",
        &format!("/api/v1/channels/{channel}/pins"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pins = body["data"]["pins"].as_array().expect("pins array");
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0]["id"], msg);
    assert_eq!(pins[0]["pinnedBy"], "PinBot");

    let (status, _) = request(
        &app,
        "DELETE",
        &format!("/api/v1/channels/{channel}/pins/{msg}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Unpinning again reports the pin as gone.
    let (status, body) = request(
        &app,
        "DELETE",
        &format!("/api/v1/channels/{channel}/pins/{msg}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "pin-not-found");
}

#[tokio::test]
async fn channel_management() {
    let (app, state) = make_app().await;
    let token = create_bot(&app, "ChanBot", ALL_PERMS).await;

    // Create.
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/channels",
        &token,
        Some(json!({"name": "bot-made"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let new_id = body["data"]["id"].as_i64().expect("channel id");

    // Duplicate name conflicts.
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/channels",
        &token,
        Some(json!({"name": "bot-made"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "channel-already-exists");

    // Set topic.
    let (status, body) = request(
        &app,
        "PATCH",
        &format!("/api/v1/channels/{new_id}"),
        &token,
        Some(json!({"topic": "made by a bot"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["topic"], "made by a bot");

    // Delete.
    let (status, _) = request(
        &app,
        "DELETE",
        &format!("/api/v1/channels/{new_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // "general" is protected.
    let general = general_channel_id(&state).await;
    let (status, body) = request(
        &app,
        "DELETE",
        &format!("/api/v1/channels/{general}"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "cannot-delete-general");

    // Without manage_channels everything is rejected.
    let plain = create_bot(&app, "NoChanBot", &["read_messages", "send_messages"]).await;
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/channels",
        &plain,
        Some(json!({"name": "nope"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "missing-permission:manage_channels");
}

#[tokio::test]
async fn typing_broadcasts_to_channel() {
    let (app, state) = make_app().await;
    let token = create_bot(&app, "TypistBot", ALL_PERMS).await;
    let channel = general_channel_id(&state).await;

    // Subscribe to the channel broadcast before triggering typing.
    let mut rx = {
        let mut channels = state.channels.lock().await;
        channels
            .entry(channel)
            .or_insert_with(|| broadcast::channel(64).0)
            .subscribe()
    };

    let (status, _) = request(
        &app,
        "POST",
        &format!("/api/v1/channels/{channel}/typing"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let event: Value = serde_json::from_str(&rx.recv().await.expect("typing event")).expect("json");
    assert_eq!(event["type"], "typing");
    assert_eq!(event["user"], "TypistBot");
    assert_eq!(event["channelId"], channel);
}

#[tokio::test]
async fn search_and_emojis() {
    let (app, state) = make_app().await;
    let token = create_bot(&app, "SearchBot", ALL_PERMS).await;
    let channel = general_channel_id(&state).await;

    send_text(&app, &token, channel, "the quick brown fox").await;
    send_text(&app, &token, channel, "unrelated chatter").await;

    let (status, body) = request(
        &app,
        "GET",
        &format!("/api/v1/channels/{channel}/messages/search?q=quick"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let messages = body["data"]["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["text"], "the quick brown fox");

    // Empty query is rejected.
    let (status, body) = request(
        &app,
        "GET",
        &format!("/api/v1/channels/{channel}/messages/search?q=%20"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "missing-query");

    // Custom emoji listing.
    db::add_emoji(&state.db, "blob_wave", "/uploads/w.png", "alice")
        .await
        .expect("register emoji");
    let (status, body) = request(&app, "GET", "/api/v1/emojis", &token, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let emojis = body["data"]["emojis"].as_array().expect("emojis");
    assert_eq!(emojis.len(), 1);
    assert_eq!(emojis[0]["name"], "blob_wave");
}
