//! Pin persistence: the per-channel cap, the projection a client renders a
//! pin from, and the order the pins come back in. A pin that quietly never
//! shows up — refused by the cap, dropped by the join, or sorted to the
//! bottom of a long list — looks exactly like a channel nobody pinned
//! anything in, so none of this surfaces in a smoke test.

use murmer_server::db::{self, DbCall};
use serde_json::{Value, json};

/// Mirrors `ws::constants::MAX_PINS_PER_CHANNEL`. The constant is
/// crate-private and the database call takes the cap as an argument, so the
/// value here only has to match what the handlers pass in.
const MAX_PINS: i64 = 25;

async fn setup() -> (db::Db, i32) {
    let db = db::init(":memory:").await.expect("in-memory db");
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("default channel exists");
    (db, channel)
}

/// Store a message with raw JSON content and return its id.
async fn insert_raw(db: &db::Db, channel: i32, content: &str) -> i64 {
    db::insert_message(db, channel, content)
        .await
        .expect("insert message")
}

async fn insert_chat(db: &db::Db, channel: i32, user: &str, text: &str) -> i64 {
    let content = json!({
        "type": "chat",
        "user": user,
        "text": text,
        "timestamp": "2026-01-02T03:04:05.000Z",
    })
    .to_string();
    insert_raw(db, channel, &content).await
}

async fn pin(db: &db::Db, message_id: i64, channel: i32, by: &str) -> bool {
    db::add_pin(db, message_id, channel, by, MAX_PINS)
        .await
        .expect("add pin")
}

async fn pins(db: &db::Db, channel: i32) -> Vec<Value> {
    db::get_pins_for_channel(db, channel)
        .await
        .expect("load pins")
}

/// Force a pin's timestamp: pins created within one test land in the same
/// millisecond, which exercises the tiebreak rather than the ordering.
async fn set_pinned_at(db: &db::Db, message_id: i64, at: &str) {
    let at = at.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "UPDATE pins SET pinned_at = ?2 WHERE message_id = ?1",
            (message_id, at),
        )
    })
    .await
    .expect("set pinned_at");
}

#[tokio::test]
async fn a_pin_carries_the_message_the_client_renders() {
    let (db, channel) = setup().await;
    let content = json!({
        "type": "chat",
        "user": "alice",
        "text": "the release plan",
        "image": "/files/plan.png",
        "timestamp": "2026-01-02T03:04:05.000Z",
    })
    .to_string();
    let id = insert_raw(&db, channel, &content).await;
    assert!(pin(&db, id, channel, "bob").await);

    let list = pins(&db, channel).await;
    assert_eq!(list.len(), 1);
    let entry = &list[0];
    assert_eq!(entry["id"], id);
    assert_eq!(entry["user"], "alice");
    assert_eq!(entry["text"], "the release plan");
    assert_eq!(entry["image"], "/files/plan.png");
    assert_eq!(entry["timestamp"], "2026-01-02T03:04:05.000Z");
    // Who pinned it is the pin's own metadata, not the message author's.
    assert_eq!(entry["pinnedBy"], "bob");
    let pinned_at = entry["pinnedAt"].as_str().expect("pinnedAt is a string");
    assert!(
        chrono::DateTime::parse_from_rfc3339(pinned_at).is_ok(),
        "pinnedAt {pinned_at:?} should be RFC 3339"
    );
}

#[tokio::test]
async fn absent_message_fields_come_back_as_null() {
    let (db, channel) = setup().await;
    let id = insert_raw(&db, channel, &json!({"user": "alice"}).to_string()).await;
    assert!(pin(&db, id, channel, "alice").await);

    let list = pins(&db, channel).await;
    assert_eq!(list.len(), 1);
    for field in ["text", "image", "enc", "timestamp"] {
        assert_eq!(list[0][field], Value::Null, "{field} should be null");
    }
}

/// An encrypted channel stores no plaintext, so the sealed envelope is the
/// only thing a pin can hand the client to render from.
#[tokio::test]
async fn encrypted_messages_are_pinned_as_their_envelope() {
    let (db, channel) = setup().await;
    let content = json!({
        "type": "chat",
        "user": "alice",
        "enc": {"epoch": 2, "nonce": "bm9uY2U=", "ciphertext": "c2VhbGVk"},
        "timestamp": "2026-01-02T03:04:05.000Z",
    })
    .to_string();
    let id = insert_raw(&db, channel, &content).await;
    assert!(pin(&db, id, channel, "alice").await);

    let list = pins(&db, channel).await;
    assert_eq!(list[0]["text"], Value::Null);
    assert_eq!(list[0]["enc"]["epoch"], 2);
    assert_eq!(list[0]["enc"]["ciphertext"], "c2VhbGVk");
}

#[tokio::test]
async fn the_cap_counts_per_channel() {
    let (db, general) = setup().await;
    let other = db::add_channel(&db, "other", None)
        .await
        .expect("create channel")
        .expect("channel is new")
        .id;

    for i in 0..MAX_PINS {
        let id = insert_chat(&db, general, "alice", &format!("message {i}")).await;
        assert!(pin(&db, id, general, "alice").await, "pin {i} should fit");
    }
    let over = insert_chat(&db, general, "alice", "one too many").await;
    assert!(!pin(&db, over, general, "alice").await);
    assert_eq!(pins(&db, general).await.len(), MAX_PINS as usize);

    // A full channel says nothing about any other channel.
    let elsewhere = insert_chat(&db, other, "alice", "unrelated").await;
    assert!(pin(&db, elsewhere, other, "alice").await);
    assert_eq!(pins(&db, other).await.len(), 1);
}

/// Two clients can pin the same message at once, so the second call has to be
/// a no-op that still reports success — including in a full channel, where
/// counting it as a new pin would refuse a pin that already exists.
#[tokio::test]
async fn pinning_an_already_pinned_message_succeeds_without_duplicating_it() {
    let (db, channel) = setup().await;
    let first = insert_chat(&db, channel, "alice", "keep this").await;
    assert!(pin(&db, first, channel, "alice").await);

    for i in 1..MAX_PINS {
        let id = insert_chat(&db, channel, "alice", &format!("filler {i}")).await;
        assert!(pin(&db, id, channel, "alice").await);
    }
    assert_eq!(pins(&db, channel).await.len(), MAX_PINS as usize);

    assert!(pin(&db, first, channel, "bob").await);
    let list = pins(&db, channel).await;
    assert_eq!(list.len(), MAX_PINS as usize);
    // The original pin is untouched: the re-pin neither duplicated the row
    // nor rewrote who pinned it.
    let entry = list
        .iter()
        .find(|p| p["id"] == first)
        .expect("original pin survives");
    assert_eq!(entry["pinnedBy"], "alice");
}

#[tokio::test]
async fn unpinning_reports_the_channel_and_frees_a_slot() {
    let (db, channel) = setup().await;
    let mut ids = Vec::new();
    for i in 0..MAX_PINS {
        let id = insert_chat(&db, channel, "alice", &format!("message {i}")).await;
        assert!(pin(&db, id, channel, "alice").await);
        ids.push(id);
    }

    // The returned channel is the caller's only way of knowing which pin list
    // to rebroadcast after an unpin.
    assert_eq!(
        db::remove_pin(&db, ids[0]).await.expect("unpin"),
        Some(channel)
    );
    // Unpinning again (a concurrent unpin) is not an error, just nothing.
    assert_eq!(db::remove_pin(&db, ids[0]).await.expect("unpin"), None);
    assert_eq!(
        db::remove_pin(&db, 9_999).await.expect("unpin unknown"),
        None
    );

    let replacement = insert_chat(&db, channel, "alice", "the freed slot").await;
    assert!(pin(&db, replacement, channel, "alice").await);
    assert_eq!(pins(&db, channel).await.len(), MAX_PINS as usize);
}

#[tokio::test]
async fn pins_come_back_newest_first() {
    let (db, channel) = setup().await;
    let mut ids = Vec::new();
    for i in 0..3 {
        let id = insert_chat(&db, channel, "alice", &format!("message {i}")).await;
        assert!(pin(&db, id, channel, "alice").await);
        ids.push(id);
    }
    // Pinned in an order unrelated to when the messages were sent.
    set_pinned_at(&db, ids[0], "2026-03-01T12:00:00.000Z").await;
    set_pinned_at(&db, ids[1], "2026-01-01T12:00:00.000Z").await;
    set_pinned_at(&db, ids[2], "2026-02-01T12:00:00.000Z").await;

    let list = pins(&db, channel).await;
    let order: Vec<&Value> = list.iter().map(|p| &p["id"]).collect();
    assert_eq!(order, vec![&json!(ids[0]), &json!(ids[2]), &json!(ids[1])]);
}

/// Pins made in the same millisecond — what a burst of pinning produces,
/// since `pinned_at` has no finer resolution — must still come back in a
/// stable order rather than whichever one SQLite happens to pick.
#[tokio::test]
async fn simultaneous_pins_fall_back_to_the_message_order() {
    let (db, channel) = setup().await;
    let mut ids = Vec::new();
    for i in 0..3 {
        let id = insert_chat(&db, channel, "alice", &format!("message {i}")).await;
        assert!(pin(&db, id, channel, "alice").await);
        set_pinned_at(&db, id, "2026-03-01T12:00:00.000Z").await;
        ids.push(id);
    }

    let list = pins(&db, channel).await;
    let order: Vec<&Value> = list.iter().map(|p| &p["id"]).collect();
    assert_eq!(order, vec![&json!(ids[2]), &json!(ids[1]), &json!(ids[0])]);
}

#[tokio::test]
async fn a_channels_pins_stay_in_that_channel() {
    let (db, general) = setup().await;
    let other = db::add_channel(&db, "other", None)
        .await
        .expect("create channel")
        .expect("channel is new")
        .id;
    let id = insert_chat(&db, general, "alice", "hello").await;
    assert!(pin(&db, id, general, "alice").await);

    assert_eq!(pins(&db, general).await.len(), 1);
    assert!(pins(&db, other).await.is_empty());
}

/// Pins carry no foreign key, so the message delete path is the only thing
/// keeping the table from filling with rows that point at nothing.
#[tokio::test]
async fn deleting_a_message_takes_its_pin_with_it() {
    let (db, channel) = setup().await;
    let id = insert_chat(&db, channel, "alice", "temporary").await;
    assert!(pin(&db, id, channel, "alice").await);

    assert!(db::delete_message(&db, id).await.expect("delete message"));
    assert!(pins(&db, channel).await.is_empty());
    assert_eq!(db::remove_pin(&db, id).await.expect("unpin"), None);
}

/// Anything a pin cannot be rendered from is skipped, so one bad row never
/// costs a channel its whole pin list.
#[tokio::test]
async fn unrenderable_pins_are_skipped_rather_than_fatal() {
    let (db, channel) = setup().await;

    let dangling = insert_chat(&db, channel, "alice", "will be orphaned").await;
    assert!(pin(&db, dangling, channel, "alice").await);
    // Drop the message behind the pin without going through `delete_message`,
    // leaving exactly the dangling row the join has to absorb.
    db.call_db(move |conn| conn.execute("DELETE FROM messages WHERE id = ?1", (dangling,)))
        .await
        .expect("delete message row");

    let unparsable = insert_raw(&db, channel, "not json at all").await;
    assert!(pin(&db, unparsable, channel, "alice").await);

    let good = insert_chat(&db, channel, "alice", "still fine").await;
    assert!(pin(&db, good, channel, "alice").await);

    let list = pins(&db, channel).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], good);
}
