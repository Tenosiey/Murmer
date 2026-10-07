//! Integration tests for polls: one vote per account, counted by the server.
//!
//! The decisions worth pinning are the refusals. A vote in a channel the
//! voter cannot see would make polls a probe for hidden message ids; a vote
//! without `SEND_MESSAGES` would let a read-only member take part where they
//! may not; a vote in an encrypted channel would have the server learn who
//! chose what where it promised to learn nothing. None of these fail
//! visibly — they just quietly count.

use std::{collections::HashMap, sync::Arc};

use murmer_server::channel_overrides::{ChannelKind, OverridePair, OverrideSet};
use murmer_server::permissions::{DEFAULT_EVERYONE, SEND_MESSAGES, VIEW_CHANNELS};
use murmer_server::ws::constants::{MAX_POLL_OPTION_LENGTH, MAX_POLL_OPTIONS};
use murmer_server::ws::errors;
use murmer_server::ws::helpers::{parse_poll, prepare_forward, prepare_poll_vote};
use murmer_server::{AppState, RoleDef, db};
use serde_json::{Value, json};

async fn make_state() -> Arc<AppState> {
    let database = db::init(":memory:").await.expect("in-memory db");
    let state = Arc::new(AppState::new(database));
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
    state
}

async fn channel(state: &Arc<AppState>, name: &str) -> i32 {
    db::add_channel(&state.db, name, None)
        .await
        .expect("add channel")
        .expect("channel created")
        .id
}

/// Store a two-option poll in a channel and return its id.
async fn poll(state: &Arc<AppState>, channel_id: i32) -> i64 {
    let content = json!({
        "type": "chat",
        "user": "alice",
        "text": "Lunch?",
        "poll": { "options": ["Pizza", "Sushi"] },
    });
    db::insert_message(&state.db, channel_id, &content.to_string())
        .await
        .expect("insert poll")
}

async fn deny_everyone(state: &Arc<AppState>, channel_id: i32, deny: u64) {
    state.channel_overrides.lock().await.insert(
        (ChannelKind::Text, channel_id),
        OverrideSet {
            everyone: OverridePair { allow: 0, deny },
            roles: HashMap::new(),
            users: HashMap::new(),
        },
    );
}

/// The poll as history serves it, tally included.
async fn served(state: &Arc<AppState>, channel_id: i32, id: i64) -> Value {
    let rows = db::fetch_history(&state.db, channel_id, None, 50)
        .await
        .expect("history");
    let messages = db::hydrate_messages(&state.db, rows, channel_id).await;
    messages
        .into_iter()
        .find(|m| m["id"] == json!(id))
        .expect("poll served")
}

// ── Parsing ─────────────────────────────────────────────────────────────────

#[test]
fn a_poll_is_rebuilt_from_trimmed_options_alone() {
    let raw = json!({ "options": ["  Yes ", "No"], "votes": [["mallory"], []] });
    assert_eq!(parse_poll(&raw), Ok(json!({ "options": ["Yes", "No"] })));
}

#[test]
fn malformed_polls_are_refused() {
    let long = "x".repeat(MAX_POLL_OPTION_LENGTH + 1);
    let many: Vec<String> = (0..=MAX_POLL_OPTIONS).map(|i| i.to_string()).collect();
    for raw in [
        json!({}),
        json!({ "options": ["only one"] }),
        json!({ "options": many }),
        json!({ "options": ["fine", "   "] }),
        json!({ "options": ["fine", long] }),
        json!({ "options": ["fine", 2] }),
    ] {
        assert_eq!(parse_poll(&raw), Err(errors::INVALID_POLL), "{raw}");
    }
}

// ── Voting ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn one_vote_per_account_which_can_change_and_be_taken_back() {
    let state = make_state().await;
    let general = channel(&state, "lunch").await;
    let id = poll(&state, general).await;

    db::set_poll_vote(&state.db, id, "bob", Some(0))
        .await
        .unwrap();
    db::set_poll_vote(&state.db, id, "carol", Some(0))
        .await
        .unwrap();
    // Changing a vote moves it rather than adding a second one.
    db::set_poll_vote(&state.db, id, "bob", Some(1))
        .await
        .unwrap();
    assert_eq!(
        served(&state, general, id).await["poll"]["votes"],
        json!([["carol"], ["bob"]])
    );

    db::set_poll_vote(&state.db, id, "carol", None)
        .await
        .unwrap();
    assert_eq!(
        served(&state, general, id).await["poll"]["votes"],
        json!([[], ["bob"]])
    );
}

#[tokio::test]
async fn votes_go_with_their_message() {
    let state = make_state().await;
    let general = channel(&state, "lunch").await;
    let id = poll(&state, general).await;
    db::set_poll_vote(&state.db, id, "bob", Some(0))
        .await
        .unwrap();

    db::delete_message(&state.db, id).await.unwrap();
    let left = db::get_poll_votes_for_messages(&state.db, &[id])
        .await
        .unwrap();
    assert!(left.is_empty());
}

#[tokio::test]
async fn a_member_may_vote_within_the_options() {
    let state = make_state().await;
    let general = channel(&state, "lunch").await;
    let id = poll(&state, general).await;

    let (channel_id, _) = prepare_poll_vote(&state, "bob", id, Some(1))
        .await
        .expect("vote allowed");
    assert_eq!(channel_id, general);
    assert_eq!(
        prepare_poll_vote(&state, "bob", id, Some(2)).await.err(),
        Some(errors::INVALID_POLL)
    );
}

#[tokio::test]
async fn a_hidden_poll_looks_missing() {
    let state = make_state().await;
    let private = channel(&state, "private").await;
    let id = poll(&state, private).await;
    deny_everyone(&state, private, VIEW_CHANNELS).await;

    assert_eq!(
        prepare_poll_vote(&state, "bob", id, Some(0)).await.err(),
        Some(errors::MESSAGE_NOT_FOUND)
    );
}

#[tokio::test]
async fn a_read_only_member_may_take_a_vote_back_but_not_cast_one() {
    let state = make_state().await;
    let announcements = channel(&state, "announcements").await;
    let id = poll(&state, announcements).await;
    deny_everyone(&state, announcements, SEND_MESSAGES).await;

    assert_eq!(
        prepare_poll_vote(&state, "bob", id, Some(0)).await.err(),
        Some(errors::SEND_PERMISSION_DENIED)
    );
    assert!(prepare_poll_vote(&state, "bob", id, None).await.is_ok());
}

#[tokio::test]
async fn an_ordinary_message_takes_no_votes() {
    let state = make_state().await;
    let general = channel(&state, "lunch").await;
    let id = db::insert_message(&state.db, general, r#"{"user":"a","text":"hi"}"#)
        .await
        .unwrap();

    assert_eq!(
        prepare_poll_vote(&state, "bob", id, Some(0)).await.err(),
        Some(errors::INVALID_POLL)
    );
}

#[tokio::test]
async fn an_encrypted_channel_counts_no_votes() {
    let state = make_state().await;
    let sealed = channel(&state, "sealed").await;
    // Posted while the channel was still plaintext.
    let id = poll(&state, sealed).await;
    db::set_channel_e2ee(&state.db, sealed, true).await.unwrap();

    assert_eq!(
        prepare_poll_vote(&state, "bob", id, Some(0)).await.err(),
        Some(errors::CANNOT_POLL_ENCRYPTED)
    );
}

#[tokio::test]
async fn a_poll_cannot_be_forwarded() {
    let state = make_state().await;
    let general = channel(&state, "lunch").await;
    let other = channel(&state, "other").await;
    let id = poll(&state, general).await;

    assert_eq!(
        prepare_forward(&state, "bob", id, other).await.err(),
        Some(errors::CANNOT_FORWARD_POLL)
    );
}
