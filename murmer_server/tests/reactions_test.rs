//! Tests for the cap on distinct reactions per message.
//!
//! Every `reaction-update` re-sends a message's whole summary to its channel,
//! so the cap is what stops one member from making every later reaction on a
//! message expensive for everyone. It is enforced in `db::add_reaction`, the
//! one path both the WebSocket and the bot API go through.

use murmer_server::db;

async fn message() -> (db::Db, i64) {
    let db = db::init(":memory:").await.expect("in-memory db");
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("general channel");
    let id = db::insert_message(&db, channel, r#"{"user":"a","text":"hi"}"#)
        .await
        .expect("insert");
    (db, id)
}

/// Fill the message up to the cap with distinct emojis from user `a`.
async fn fill(db: &db::Db, id: i64) {
    for i in 0..db::MAX_REACTIONS_PER_MESSAGE {
        let emoji = format!("e{i}");
        assert!(db::add_reaction(db, id, "a", &emoji).await.expect("react"));
    }
}

#[tokio::test]
async fn a_new_emoji_beyond_the_cap_is_refused() {
    let (db, id) = message().await;
    fill(&db, id).await;

    assert!(!db::add_reaction(&db, id, "b", "new").await.expect("react"));
    let summary = db::get_reaction_summary(&db, id).await.expect("summary");
    assert_eq!(summary.len() as i64, db::MAX_REACTIONS_PER_MESSAGE);
    assert!(!summary.contains_key("new"));
}

#[tokio::test]
async fn joining_an_existing_emoji_at_the_cap_still_works() {
    let (db, id) = message().await;
    fill(&db, id).await;

    assert!(db::add_reaction(&db, id, "b", "e0").await.expect("react"));
    // A repeat of a reaction already held is a no-op, not a refusal.
    assert!(db::add_reaction(&db, id, "a", "e0").await.expect("react"));
    let summary = db::get_reaction_summary(&db, id).await.expect("summary");
    assert_eq!(summary["e0"], ["a", "b"]);
}

#[tokio::test]
async fn removing_an_emoji_frees_a_slot() {
    let (db, id) = message().await;
    fill(&db, id).await;

    db::remove_reaction(&db, id, "a", "e0")
        .await
        .expect("unreact");
    assert!(db::add_reaction(&db, id, "b", "new").await.expect("react"));
}
