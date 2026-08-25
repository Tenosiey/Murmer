//! Reminders and scheduled messages: the queue behaviour the scheduler rests
//! on.
//!
//! Everything here is invisible until it goes wrong, and the two failure modes
//! are opposites. A scheduled message that is claimed twice posts words to a
//! channel that were written once — so the claim has to be exclusive, and
//! nothing may re-enter the queue after a delivery attempt. A reminder that is
//! cleared when it fires is a reminder its owner never sees if they happened
//! to be offline — so the claim has to *keep* the row. Neither shows up in a
//! smoke test, where every queue is empty and every user is connected.
//!
//! The ownership scoping is the other half: `cancel_*` puts the owner into the
//! statement rather than checking before it, and the assertion that matters is
//! that somebody else's id comes back as "not found" rather than as a delete.

use chrono::{Duration, Utc};
use murmer_server::db;

/// Comfortably above any per-user cap these tests exercise deliberately.
const NO_LIMIT: i64 = 1_000;

async fn setup() -> (db::Db, i32) {
    let db = db::init(":memory:").await.expect("in-memory db");
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("default channel exists");
    (db, channel)
}

async fn schedule(db: &db::Db, user: &str, channel: i32, minutes: i64) -> i64 {
    let body = serde_json::json!({ "type": "chat", "user": user, "text": "later" }).to_string();
    db::insert_scheduled_message(
        db,
        user,
        channel,
        &body,
        Utc::now() + Duration::minutes(minutes),
        NO_LIMIT,
    )
    .await
    .expect("insert scheduled message")
    .expect("under the cap")
}

async fn remind(db: &db::Db, user: &str, minutes: i64) -> i64 {
    db::insert_reminder(
        db,
        user,
        "check the deploy",
        Utc::now() + Duration::minutes(minutes),
        None,
        NO_LIMIT,
    )
    .await
    .expect("insert reminder")
    .expect("under the cap")
}

#[tokio::test]
async fn claiming_a_scheduled_message_takes_it_out_of_the_queue() {
    let (db, channel) = setup().await;
    let due = schedule(&db, "ada", channel, -1).await;
    schedule(&db, "ada", channel, 60).await;

    let claimed = db::claim_due_scheduled_messages(&db, Utc::now())
        .await
        .expect("claim");
    assert_eq!(claimed.len(), 1, "only the past-due row is claimed");
    assert_eq!(claimed[0].id, due);
    assert_eq!(claimed[0].user_name, "ada");
    assert_eq!(claimed[0].channel_id, channel);

    // The row is still there — the caller owes it a delete or a failure — but
    // a second pass must not hand it out again. This is the assertion that
    // stands between a scheduled message and a double post.
    let second = db::claim_due_scheduled_messages(&db, Utc::now())
        .await
        .expect("second claim");
    assert!(second.is_empty(), "a claimed row is never claimed twice");
}

#[tokio::test]
async fn a_failed_scheduled_message_is_kept_and_never_retried() {
    let (db, channel) = setup().await;
    let id = schedule(&db, "ada", channel, -1).await;

    db::claim_due_scheduled_messages(&db, Utc::now())
        .await
        .expect("claim");
    db::fail_scheduled_message(&db, id, "send-permission-denied")
        .await
        .expect("mark failed");

    let rows = db::get_scheduled_messages(&db, "ada").await.expect("list");
    assert_eq!(rows.len(), 1, "the author still sees what did not go out");
    assert_eq!(
        rows[0].failed_reason.as_deref(),
        Some("send-permission-denied")
    );

    // Releasing the claim must not put it back in the queue: a refusal is
    // something to tell the author about, not to retry every tick.
    let claimed = db::claim_due_scheduled_messages(&db, Utc::now())
        .await
        .expect("claim after failure");
    assert!(claimed.is_empty());
}

#[tokio::test]
async fn a_delivered_scheduled_message_leaves_no_row() {
    let (db, channel) = setup().await;
    let id = schedule(&db, "ada", channel, -1).await;

    db::claim_due_scheduled_messages(&db, Utc::now())
        .await
        .expect("claim");
    db::delete_scheduled_message(&db, id).await.expect("delete");

    assert!(
        db::get_scheduled_messages(&db, "ada")
            .await
            .expect("list")
            .is_empty()
    );
}

#[tokio::test]
async fn interrupted_claims_become_visible_failures() {
    let (db, channel) = setup().await;
    schedule(&db, "ada", channel, -1).await;
    let untouched = schedule(&db, "ada", channel, 60).await;

    // A process that claimed a row and then died leaves it claimed. It may or
    // may not have posted, so the startup sweep reports it rather than
    // re-queueing it.
    db::claim_due_scheduled_messages(&db, Utc::now())
        .await
        .expect("claim");
    let marked = db::fail_claimed_scheduled_messages(&db, "interrupted")
        .await
        .expect("sweep");
    assert_eq!(marked, 1);

    let rows = db::get_scheduled_messages(&db, "ada").await.expect("list");
    let future = rows.iter().find(|row| row.id == untouched).expect("future");
    assert!(
        future.failed_reason.is_none(),
        "a row that was never claimed is left alone"
    );
    assert!(
        rows.iter()
            .any(|row| row.failed_reason.as_deref() == Some("interrupted"))
    );

    // Idempotent: a second startup with nothing claimed marks nothing.
    assert_eq!(
        db::fail_claimed_scheduled_messages(&db, "interrupted")
            .await
            .expect("second sweep"),
        0
    );
}

#[tokio::test]
async fn a_scheduled_message_can_only_be_cancelled_by_its_author() {
    let (db, channel) = setup().await;
    let id = schedule(&db, "ada", channel, 60).await;

    assert!(
        !db::cancel_scheduled_message(&db, "mallory", id)
            .await
            .expect("cancel as somebody else"),
        "an id belonging to somebody else reads as not found"
    );
    assert_eq!(
        db::get_scheduled_messages(&db, "ada")
            .await
            .expect("list")
            .len(),
        1
    );

    assert!(
        db::cancel_scheduled_message(&db, "ada", id)
            .await
            .expect("cancel as the author")
    );
}

#[tokio::test]
async fn the_per_user_cap_counts_failed_rows_too() {
    let (db, channel) = setup().await;
    let body = serde_json::json!({ "type": "chat", "user": "ada", "text": "later" }).to_string();
    let at = Utc::now() + Duration::minutes(5);

    let first = db::insert_scheduled_message(&db, "ada", channel, &body, at, 1)
        .await
        .expect("insert");
    assert!(first.is_some());
    assert!(
        db::insert_scheduled_message(&db, "ada", channel, &body, at, 1)
            .await
            .expect("insert at the cap")
            .is_none()
    );

    // A failed row still occupies its slot; only clearing it frees one.
    db::fail_scheduled_message(&db, first.unwrap(), "muted")
        .await
        .expect("mark failed");
    assert!(
        db::insert_scheduled_message(&db, "ada", channel, &body, at, 1)
            .await
            .expect("insert with a failed row in the way")
            .is_none()
    );

    // The cap is per user, not per server.
    assert!(
        db::insert_scheduled_message(&db, "grace", channel, &body, at, 1)
            .await
            .expect("insert for another user")
            .is_some()
    );
}

#[tokio::test]
async fn deleting_a_channel_takes_its_scheduled_messages_with_it() {
    let (db, _general) = setup().await;
    let channel = db::add_channel(&db, "standup", None)
        .await
        .expect("create channel")
        .expect("name is free")
        .id;
    schedule(&db, "ada", channel, 60).await;

    db::remove_channel(&db, channel)
        .await
        .expect("delete channel");

    assert!(
        db::get_scheduled_messages(&db, "ada")
            .await
            .expect("list")
            .is_empty(),
        "a message bound for a channel that no longer exists cannot be posted"
    );
}

#[tokio::test]
async fn a_due_reminder_is_marked_but_kept() {
    let (db, _channel) = setup().await;
    let due = remind(&db, "ada", -1).await;
    remind(&db, "ada", 60).await;

    let claimed = db::claim_due_reminders(&db, Utc::now())
        .await
        .expect("claim");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, due);
    assert_eq!(claimed[0].user_name, "ada");

    // The row survives the claim: an owner who was offline when it fired finds
    // it waiting, already marked due, on their next connection.
    let rows = db::get_reminders(&db, "ada").await.expect("list");
    assert_eq!(rows.len(), 2);
    let fired = rows.iter().find(|row| row.id == due).expect("the due one");
    assert!(fired.fired_at.is_some());

    // Marked once, announced once.
    assert!(
        db::claim_due_reminders(&db, Utc::now())
            .await
            .expect("second claim")
            .is_empty()
    );
}

#[tokio::test]
async fn a_reminder_can_only_be_dismissed_by_its_owner() {
    let (db, _channel) = setup().await;
    let id = remind(&db, "ada", 60).await;

    assert!(
        !db::cancel_reminder(&db, "mallory", id)
            .await
            .expect("dismiss as somebody else")
    );
    assert_eq!(db::get_reminders(&db, "ada").await.expect("list").len(), 1);

    assert!(db::cancel_reminder(&db, "ada", id).await.expect("dismiss"));
    assert!(
        db::get_reminders(&db, "ada")
            .await
            .expect("list")
            .is_empty()
    );
}

#[tokio::test]
async fn a_reminder_remembers_the_message_it_was_set_on() {
    let (db, channel) = setup().await;
    let message = db::insert_message(&db, channel, r#"{"type":"chat","user":"a","text":"hi"}"#)
        .await
        .expect("insert message");

    db::insert_reminder(
        &db,
        "ada",
        "follow up",
        Utc::now() + Duration::minutes(5),
        Some((channel, message)),
        NO_LIMIT,
    )
    .await
    .expect("insert")
    .expect("under the cap");

    let rows = db::get_reminders(&db, "ada").await.expect("list");
    assert_eq!(rows[0].channel_id, Some(channel));
    assert_eq!(rows[0].message_id, Some(message));
}

#[tokio::test]
async fn a_server_reset_clears_scheduled_messages_but_not_reminders() {
    let (db, channel) = setup().await;
    schedule(&db, "ada", channel, 60).await;
    remind(&db, "ada", 60).await;

    db::reset_server(&db).await.expect("reset");

    assert!(
        db::get_scheduled_messages(&db, "ada")
            .await
            .expect("list")
            .is_empty(),
        "a queued message would post into a server that no longer resembles \
         the one it was written for"
    );
    assert_eq!(
        db::get_reminders(&db, "ada").await.expect("list").len(),
        1,
        "reminders are personal notes, not server structure"
    );
}
