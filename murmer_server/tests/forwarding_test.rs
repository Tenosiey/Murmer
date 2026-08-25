//! Integration tests for forwarding a message into another channel.
//!
//! A forward is the one frame that posts words the sender did not write, under
//! a name the sender does not own, into a room that has no way to check either.
//! Every failure here is silent in the UI and serious:
//!
//! - an attribution taken from the client instead of from the stored row lets
//!   anyone make any member appear to have said anything;
//! - a missing view check on the *source* turns forwarding into a read oracle
//!   over private channels — message ids are small integers, so guess one,
//!   forward it somewhere you can post, and read the answer;
//! - distinguishable "not found" and "not allowed" replies leak the existence
//!   of hidden messages even when the content stays put.
//!
//! None of that shows up as an error anywhere. It shows up as a message that
//! looks exactly like a real one.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use murmer_server::channel_overrides::{ChannelKind, OverridePair, OverrideSet};
use murmer_server::permissions::{DEFAULT_EVERYONE, SEND_MESSAGES, VIEW_CHANNELS};
use murmer_server::ws::helpers::{forwarded_body, may_edit_message, prepare_forward};
use murmer_server::{AppState, RateLimiter, RoleDef, db};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};

async fn make_state() -> Arc<AppState> {
    let database = db::init(":memory:").await.expect("in-memory db");
    let (tx, _) = broadcast::channel(64);
    Arc::new(AppState {
        tx,
        channels: Arc::new(Mutex::new(HashMap::new())),
        direct: Arc::new(Mutex::new(HashMap::new())),
        db: database,
        users: Arc::new(Mutex::new(Default::default())),
        known_users: Arc::new(Mutex::new(Default::default())),
        voice_channels: Arc::new(Mutex::new(HashMap::new())),
        role_defs: Arc::new(Mutex::new(HashMap::new())),
        user_roles: Arc::new(Mutex::new(HashMap::new())),
        channel_overrides: Arc::new(Mutex::new(HashMap::new())),
        statuses: Arc::new(Mutex::new(HashMap::new())),
        user_keys: Arc::new(Mutex::new(HashMap::new())),
        mutes: Arc::new(Mutex::new(HashMap::new())),
        active_screen_shares: Arc::new(Mutex::new(HashMap::new())),
        active_webcams: Arc::new(Mutex::new(HashMap::new())),
        voice_mutes: Arc::new(Mutex::new(HashMap::new())),
        connection_stats: Arc::new(Mutex::new(HashMap::new())),
        voice_session_starts: Arc::new(Mutex::new(HashMap::new())),
        screenshare_session_starts: Arc::new(Mutex::new(HashMap::new())),
        soundboard_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        upload_dir: PathBuf::from("uploads"),
        password: None,
        admin_token: Some("token".to_string()),
        rate_limiter: RateLimiter::new(),
        stats_enabled: std::sync::atomic::AtomicBool::new(false),
        chat_settings: Arc::new(Mutex::new(murmer_server::db::ChatSettings::default())),
        automod: Arc::new(Mutex::new(murmer_server::automod::RuleSet::default())),
        slow_mode_sends: Arc::new(Mutex::new(HashMap::new())),
        visibility_epoch: std::sync::atomic::AtomicU64::new(0),
    })
}

/// One `@everyone` role granting the usual view + send baseline.
async fn seed_everyone(state: &Arc<AppState>) {
    state.role_defs.lock().await.insert(
        1,
        RoleDef {
            id: 1,
            name: "@everyone".to_string(),
            color: None,
            icon: None,
            permissions: DEFAULT_EVERYONE,
            is_default: true,
            is_owner: false,
            position: 0,
        },
    );
}

/// Create a text channel and return its id.
async fn channel(state: &Arc<AppState>, name: &str) -> i32 {
    db::add_channel(&state.db, name, None)
        .await
        .expect("add channel")
        .expect("channel created")
        .id
}

/// Store a message in a channel and return its id.
async fn post(state: &Arc<AppState>, channel_id: i32, content: Value) -> i64 {
    db::insert_message(&state.db, channel_id, &content.to_string())
        .await
        .expect("insert message")
}

/// Hide a channel from `@everyone`, the shape a private channel takes.
async fn make_private(state: &Arc<AppState>, channel_id: i32) {
    state.channel_overrides.lock().await.insert(
        (ChannelKind::Text, channel_id),
        OverrideSet {
            everyone: OverridePair {
                allow: 0,
                deny: VIEW_CHANNELS,
            },
            roles: HashMap::new(),
            users: HashMap::new(),
        },
    );
}

/// Deny `@everyone` writing to a channel while leaving it visible.
async fn make_read_only(state: &Arc<AppState>, channel_id: i32) {
    state.channel_overrides.lock().await.insert(
        (ChannelKind::Text, channel_id),
        OverrideSet {
            everyone: OverridePair {
                allow: 0,
                deny: SEND_MESSAGES,
            },
            roles: HashMap::new(),
            users: HashMap::new(),
        },
    );
}

// ── The copy itself ─────────────────────────────────────────────────────────

#[test]
fn copy_carries_content_and_a_server_built_attribution() {
    let source = json!({
        "type": "chat",
        "user": "alice",
        "text": "the original",
        "attachment": { "url": "https://example.test/f", "name": "f", "size": 1 },
        "reactions": { "👍": ["bob"] },
        "threadId": 7,
        "replyTo": { "id": 7, "user": "carol", "text": "…" },
        "edited": true,
        "id": 42,
    });

    let copy = forwarded_body(42, &source, 3, "general").expect("body");

    assert_eq!(copy["text"], json!("the original"));
    assert_eq!(copy["attachment"]["name"], json!("f"));
    assert_eq!(
        copy["forwardedFrom"],
        json!({ "id": 42, "user": "alice", "channel": "general", "channelId": 3 })
    );

    // Everything else on the stored frame described the original posting and
    // would be a lie on the copy.
    for stale in ["reactions", "threadId", "replyTo", "edited", "id", "user"] {
        assert!(copy.get(stale).is_none(), "copy still carries {stale}");
    }
}

#[test]
fn forwarding_a_forward_keeps_the_first_author() {
    let first = json!({ "user": "alice", "text": "the original" });
    let once = forwarded_body(42, &first, 3, "general").expect("body");

    // Bob's forward is now itself a stored message authored by bob.
    let stored = json!({
        "user": "bob",
        "text": once["text"].clone(),
        "forwardedFrom": once["forwardedFrom"].clone(),
    });
    let twice = forwarded_body(99, &stored, 5, "random").expect("body");

    assert_eq!(twice["forwardedFrom"], once["forwardedFrom"]);
}

#[test]
fn a_message_with_no_copyable_content_is_refused() {
    // A sealed message from an encrypted channel: the server holds the
    // envelope and nothing that can be quoted out of it.
    let sealed = json!({ "user": "alice", "enc": { "epoch": 1, "nonce": "n", "ciphertext": "c" } });
    assert!(forwarded_body(1, &sealed, 3, "secret").is_none());
}

// ── The decision ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_visible_message_is_copied_under_its_original_author() {
    let state = make_state().await;
    seed_everyone(&state).await;
    let source = channel(&state, "source").await;
    let target = channel(&state, "target").await;
    let id = post(&state, source, json!({ "user": "alice", "text": "hello" })).await;

    let copy = prepare_forward(&state, "bob", id, target)
        .await
        .expect("forward allowed");
    assert_eq!(copy["text"], json!("hello"));
    assert_eq!(copy["forwardedFrom"]["user"], json!("alice"));
    assert_eq!(copy["forwardedFrom"]["channel"], json!("source"));
}

#[tokio::test]
async fn a_source_the_forwarder_cannot_see_is_refused() {
    let state = make_state().await;
    seed_everyone(&state).await;
    let source = channel(&state, "leadership").await;
    let target = channel(&state, "target").await;
    make_private(&state, source).await;
    let id = post(
        &state,
        source,
        json!({ "user": "alice", "text": "confidential" }),
    )
    .await;

    assert_eq!(
        prepare_forward(&state, "bob", id, target).await,
        Err(murmer_server::ws::errors::FORWARD_SOURCE_NOT_FOUND)
    );
}

#[tokio::test]
async fn a_hidden_source_answers_exactly_as_a_missing_one() {
    let state = make_state().await;
    seed_everyone(&state).await;
    let source = channel(&state, "leadership").await;
    let target = channel(&state, "target").await;
    make_private(&state, source).await;
    let hidden = post(
        &state,
        source,
        json!({ "user": "alice", "text": "confidential" }),
    )
    .await;

    // Same code for both, so probing ids cannot tell a hidden message from a
    // message that never existed.
    assert_eq!(
        prepare_forward(&state, "bob", hidden, target).await,
        prepare_forward(&state, "bob", hidden + 10_000, target).await
    );
}

#[tokio::test]
async fn forwarding_needs_send_permission_in_the_destination() {
    let state = make_state().await;
    seed_everyone(&state).await;
    let source = channel(&state, "source").await;
    let target = channel(&state, "announcements").await;
    make_read_only(&state, target).await;
    let id = post(&state, source, json!({ "user": "alice", "text": "hello" })).await;

    assert_eq!(
        prepare_forward(&state, "bob", id, target).await,
        Err(murmer_server::ws::errors::SEND_PERMISSION_DENIED)
    );
}

#[tokio::test]
async fn an_encrypted_channel_at_either_end_refuses_the_copy() {
    let state = make_state().await;
    seed_everyone(&state).await;
    let plain = channel(&state, "plain").await;
    let sealed = channel(&state, "sealed").await;
    db::set_channel_e2ee(&state.db, sealed, true)
        .await
        .expect("mark encrypted");

    // Out of the encrypted channel: the server has no plaintext to copy.
    let from_sealed = post(&state, sealed, json!({ "user": "alice", "text": "x" })).await;
    assert_eq!(
        prepare_forward(&state, "bob", from_sealed, plain).await,
        Err(murmer_server::ws::errors::CANNOT_FORWARD_ENCRYPTED)
    );

    // Into it: the server holds no key to seal the copy with.
    let from_plain = post(&state, plain, json!({ "user": "alice", "text": "x" })).await;
    assert_eq!(
        prepare_forward(&state, "bob", from_plain, sealed).await,
        Err(murmer_server::ws::errors::CANNOT_FORWARD_ENCRYPTED)
    );
}

#[tokio::test]
async fn an_ephemeral_message_is_not_copied() {
    let state = make_state().await;
    seed_everyone(&state).await;
    let source = channel(&state, "source").await;
    let target = channel(&state, "target").await;
    let id = post(
        &state,
        source,
        json!({ "user": "alice", "text": "gone in a minute", "ephemeral": true }),
    )
    .await;

    assert_eq!(
        prepare_forward(&state, "bob", id, target).await,
        Err(murmer_server::ws::errors::CANNOT_FORWARD_EPHEMERAL)
    );
}

// ── What keeps the attribution true afterwards ─────────────────────────────

#[test]
fn a_forward_cannot_be_edited_by_the_member_who_forwarded_it() {
    // The whole point of the server making the copy is undone if its author
    // can then rewrite it: the words would be theirs and the attribution would
    // still name somebody else.
    let forward = json!({
        "user": "bob",
        "text": "the original",
        "forwardedFrom": { "id": 1, "user": "alice", "channel": "general", "channelId": 1 },
    });
    assert_eq!(
        may_edit_message(&forward, "bob"),
        Err(murmer_server::ws::errors::CANNOT_EDIT_FORWARD)
    );

    // The ordinary rules are untouched: authors edit their own words, nobody
    // edits anyone else's.
    let own = json!({ "user": "bob", "text": "mine" });
    assert_eq!(may_edit_message(&own, "bob"), Ok(()));
    assert_eq!(
        may_edit_message(&own, "carol"),
        Err(murmer_server::ws::errors::MESSAGE_PERMISSION_DENIED)
    );
}
