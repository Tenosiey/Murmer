use murmer_server::db::{self, DbCall};

async fn setup() -> (db::Db, i32) {
    let db = db::init(":memory:").await.expect("in-memory db");
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("default channel exists");
    (db, channel)
}

async fn insert_text(db: &db::Db, channel: i32, text: &str) -> i64 {
    let content = serde_json::json!({"type": "chat", "user": "alice", "text": text}).to_string();
    db::insert_message(db, channel, &content)
        .await
        .expect("insert message")
}

#[tokio::test]
async fn finds_messages_by_word_and_prefix() {
    let (db, channel) = setup().await;
    let id = insert_text(&db, channel, "The quick brown fox").await;
    insert_text(&db, channel, "completely unrelated").await;

    for query in ["quick", "QUICK", "bro", "quick brown"] {
        let rows = db::search_messages(&db, channel, query, 50)
            .await
            .expect("search");
        assert_eq!(rows.len(), 1, "query {query:?} should match once");
        assert_eq!(rows[0].0, id);
    }

    // Phrase adjacency: the words exist but not next to each other.
    assert!(
        db::search_messages(&db, channel, "quick fox", 50)
            .await
            .expect("search")
            .is_empty()
    );
}

#[tokio::test]
async fn respects_channel_boundaries() {
    let (db, general) = setup().await;
    let other = db::add_channel(&db, "other", None)
        .await
        .expect("create channel")
        .expect("channel is new")
        .id;
    insert_text(&db, general, "hello world").await;

    assert!(
        db::search_messages(&db, other, "hello", 50)
            .await
            .expect("search")
            .is_empty()
    );
}

#[tokio::test]
async fn index_follows_edits_and_deletions() {
    let (db, channel) = setup().await;
    let id = insert_text(&db, channel, "original wording").await;

    let edited = serde_json::json!({"type": "chat", "user": "alice", "text": "revised phrasing"})
        .to_string();
    assert!(
        db::update_message_content(&db, id, &edited)
            .await
            .expect("edit")
    );
    assert!(
        db::search_messages(&db, channel, "original", 50)
            .await
            .expect("search")
            .is_empty()
    );
    assert_eq!(
        db::search_messages(&db, channel, "revised", 50)
            .await
            .expect("search")
            .len(),
        1
    );

    assert!(db::delete_message(&db, id).await.expect("delete"));
    assert!(
        db::search_messages(&db, channel, "revised", 50)
            .await
            .expect("search")
            .is_empty()
    );
}

/// Databases created before the FTS index existed must be backfilled when the
/// schema initialisation runs again (i.e. on the first startup after the
/// upgrade).
#[tokio::test]
async fn backfills_index_for_pre_fts_messages() {
    let (db, channel) = setup().await;

    // Simulate a pre-FTS database: drop the index and its triggers, then
    // insert a message that consequently never gets indexed.
    db.call_db(|conn| {
        conn.execute_batch(
            "DROP TRIGGER messages_fts_insert;
             DROP TRIGGER messages_fts_update;
             DROP TRIGGER messages_fts_delete;
             DROP TABLE messages_fts;",
        )
    })
    .await
    .expect("drop fts schema");
    let id = insert_text(&db, channel, "historic message").await;

    // Re-running the schema pass (as a restart would) must recreate and
    // backfill the index.
    db::run_schema(&db).await.expect("re-init schema");

    let rows = db::search_messages(&db, channel, "historic", 50)
        .await
        .expect("search");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, id);
}

#[tokio::test]
async fn hostile_queries_neither_error_nor_match_everything() {
    let (db, channel) = setup().await;
    insert_text(&db, channel, "plain message").await;

    // FTS5 operators and punctuation-only input must not reach the MATCH
    // parser: no syntax errors, no accidental wildcard matches.
    for query in ["\"", "*", "AND", "!!!", "( OR )", "text: plain"] {
        let result = db::search_messages(&db, channel, query, 50).await;
        assert!(result.is_ok(), "query {query:?} must not error");
    }
    assert!(
        db::search_messages(&db, channel, "!!!", 50)
            .await
            .expect("search")
            .is_empty()
    );
    // Operator words still work as literal search terms.
    insert_text(&db, channel, "mix AND match").await;
    assert_eq!(
        db::search_messages(&db, channel, "AND", 50)
            .await
            .expect("search")
            .len(),
        1
    );
}
