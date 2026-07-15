//! Tests for lifetime user statistics: double opt-in gating, counter
//! semantics and the purge path.

use murmer_server::db::{self, Stat};

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

async fn messages_sent(db: &db::Db, user: &str) -> i64 {
    db::get_user_stats(db, user)
        .await
        .expect("load stats")
        .map(|record| record.messages_sent)
        .unwrap_or(0)
}

#[tokio::test]
async fn nothing_is_recorded_without_double_opt_in() {
    let db = setup().await;

    // Neither switch enabled.
    let recorded = db::record_user_stats(&db, "alice", vec![(Stat::MessagesSent, 1)], None)
        .await
        .expect("record");
    assert!(!recorded);
    assert_eq!(messages_sent(&db, "alice").await, 0);

    // Only the user opted in — server toggle still off.
    db::set_stats_opt_in(&db, "alice", true)
        .await
        .expect("opt in");
    let recorded = db::record_user_stats(&db, "alice", vec![(Stat::MessagesSent, 1)], None)
        .await
        .expect("record");
    assert!(!recorded);
    assert_eq!(messages_sent(&db, "alice").await, 0);

    // Only the server enabled — a user without opt-in stays untracked.
    db::set_stats_opt_in(&db, "alice", false)
        .await
        .expect("opt out");
    db::set_stats_server_enabled(&db, true)
        .await
        .expect("enable server toggle");
    let recorded = db::record_user_stats(&db, "alice", vec![(Stat::MessagesSent, 1)], None)
        .await
        .expect("record");
    assert!(!recorded);
    assert_eq!(messages_sent(&db, "alice").await, 0);
}

#[tokio::test]
async fn records_when_both_switches_are_on() {
    let db = setup().await;
    db::set_stats_server_enabled(&db, true)
        .await
        .expect("enable");
    db::set_stats_opt_in(&db, "alice", true)
        .await
        .expect("opt in");

    let recorded = db::record_user_stats(
        &db,
        "alice",
        vec![(Stat::MessagesSent, 1), (Stat::MessageChars, 42)],
        None,
    )
    .await
    .expect("record");
    assert!(recorded);

    let stats = db::get_user_stats(&db, "alice")
        .await
        .expect("load stats")
        .expect("stats row exists");
    assert_eq!(stats.messages_sent, 1);
    assert_eq!(stats.message_chars, 42);

    // Other users remain unaffected and gated.
    assert_eq!(messages_sent(&db, "bob").await, 0);
}

#[tokio::test]
async fn longest_message_is_a_running_maximum() {
    let db = setup().await;
    db::set_stats_server_enabled(&db, true)
        .await
        .expect("enable");
    db::set_stats_opt_in(&db, "alice", true)
        .await
        .expect("opt in");

    for length in [100, 400, 250] {
        db::record_user_stats(
            &db,
            "alice",
            vec![(Stat::LongestMessageChars, length)],
            None,
        )
        .await
        .expect("record");
    }

    let stats = db::get_user_stats(&db, "alice")
        .await
        .expect("load stats")
        .expect("stats row exists");
    assert_eq!(stats.longest_message_chars, 400);
}

#[tokio::test]
async fn favorite_reactions_count_per_emoji() {
    let db = setup().await;
    db::set_stats_server_enabled(&db, true)
        .await
        .expect("enable");
    db::set_stats_opt_in(&db, "alice", true)
        .await
        .expect("opt in");

    for emoji in ["🎉", "🎉", "👍"] {
        db::record_user_stats(
            &db,
            "alice",
            vec![(Stat::ReactionsGiven, 1)],
            Some(emoji.to_string()),
        )
        .await
        .expect("record");
    }

    let favorites = db::get_favorite_reactions(&db, "alice", 5)
        .await
        .expect("load favorites");
    assert_eq!(favorites[0], ("🎉".to_string(), 2));
    assert_eq!(favorites[1], ("👍".to_string(), 1));

    let stats = db::get_user_stats(&db, "alice")
        .await
        .expect("load stats")
        .expect("stats row exists");
    assert_eq!(stats.reactions_given, 3);
}

#[tokio::test]
async fn purge_deletes_counters_but_keeps_the_opt_in() {
    let db = setup().await;
    db::set_stats_server_enabled(&db, true)
        .await
        .expect("enable");
    db::set_stats_opt_in(&db, "alice", true)
        .await
        .expect("opt in");
    db::record_user_stats(
        &db,
        "alice",
        vec![(Stat::MessagesSent, 5)],
        Some("🎉".to_string()),
    )
    .await
    .expect("record");

    db::purge_user_stats(&db, "alice").await.expect("purge");

    assert_eq!(messages_sent(&db, "alice").await, 0);
    assert!(db::get_favorite_reactions(&db, "alice", 5)
        .await
        .expect("load favorites")
        .is_empty());
    // The preference survives, so tracking resumes with fresh counters.
    assert!(db::stats_opt_in(&db, "alice").await.expect("opt-in state"));
    let recorded = db::record_user_stats(&db, "alice", vec![(Stat::MessagesSent, 1)], None)
        .await
        .expect("record");
    assert!(recorded);
    assert_eq!(messages_sent(&db, "alice").await, 1);
}

#[tokio::test]
async fn disabling_the_server_toggle_stops_recording() {
    let db = setup().await;
    db::set_stats_server_enabled(&db, true)
        .await
        .expect("enable");
    db::set_stats_opt_in(&db, "alice", true)
        .await
        .expect("opt in");
    db::record_user_stats(&db, "alice", vec![(Stat::MessagesSent, 1)], None)
        .await
        .expect("record");

    db::set_stats_server_enabled(&db, false)
        .await
        .expect("disable");
    let recorded = db::record_user_stats(&db, "alice", vec![(Stat::MessagesSent, 1)], None)
        .await
        .expect("record");
    assert!(!recorded);

    // Existing counters are kept (hidden client-side), not wiped.
    assert_eq!(messages_sent(&db, "alice").await, 1);
}
