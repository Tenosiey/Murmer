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
        let rows = db::search_messages(&db, channel, query, &db::SearchFilters::default(), 50)
            .await
            .expect("search");
        assert_eq!(rows.len(), 1, "query {query:?} should match once");
        assert_eq!(rows[0].0, id);
    }

    // Phrase adjacency: the words exist but not next to each other.
    assert!(
        db::search_messages(&db, channel, "quick fox", &db::SearchFilters::default(), 50)
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
        db::search_messages(&db, other, "hello", &db::SearchFilters::default(), 50)
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
        db::search_messages(&db, channel, "original", &db::SearchFilters::default(), 50)
            .await
            .expect("search")
            .is_empty()
    );
    assert_eq!(
        db::search_messages(&db, channel, "revised", &db::SearchFilters::default(), 50)
            .await
            .expect("search")
            .len(),
        1
    );

    assert!(db::delete_message(&db, id).await.expect("delete"));
    assert!(
        db::search_messages(&db, channel, "revised", &db::SearchFilters::default(), 50)
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

    let rows = db::search_messages(&db, channel, "historic", &db::SearchFilters::default(), 50)
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
        let result =
            db::search_messages(&db, channel, query, &db::SearchFilters::default(), 50).await;
        assert!(result.is_ok(), "query {query:?} must not error");
    }
    assert!(
        db::search_messages(&db, channel, "!!!", &db::SearchFilters::default(), 50)
            .await
            .expect("search")
            .is_empty()
    );
    // Operator words still work as literal search terms.
    insert_text(&db, channel, "mix AND match").await;
    assert_eq!(
        db::search_messages(&db, channel, "AND", &db::SearchFilters::default(), 50)
            .await
            .expect("search")
            .len(),
        1
    );
}

async fn insert_json(db: &db::Db, channel: i32, content: serde_json::Value) -> i64 {
    db::insert_message(db, channel, &content.to_string())
        .await
        .expect("insert message")
}

#[tokio::test]
async fn filters_narrow_by_author_file_and_date() {
    let (db, channel) = setup().await;
    let old = insert_json(
        &db,
        channel,
        serde_json::json!({"type": "chat", "user": "alice", "text": "status report",
            "timestamp": "2026-01-01T10:00:00.5+00:00"}),
    )
    .await;
    let file = insert_json(
        &db,
        channel,
        serde_json::json!({"type": "chat", "user": "bob", "text": "status report",
            "attachment": {"url": "/files/a", "name": "a.txt", "size": 1},
            "timestamp": "2026-02-01T10:00:00+00:00"}),
    )
    .await;
    let image = insert_json(
        &db,
        channel,
        serde_json::json!({"type": "chat", "user": "alice", "image": "/files/b",
            "timestamp": "2026-03-01T10:00:00+00:00"}),
    )
    .await;
    // A malformed row must be skipped, not abort the whole search.
    db::insert_message(&db, channel, "not json")
        .await
        .expect("insert raw");

    let ids = |rows: Vec<(i64, String)>| rows.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
    let search = |query: &'static str, filters: db::SearchFilters| {
        let db = db.clone();
        async move {
            db::search_messages(&db, channel, query, &filters, 50)
                .await
                .expect("search")
        }
    };

    let from_alice = db::SearchFilters {
        from: Some("alice".into()),
        ..Default::default()
    };
    // A filter alone is a search; with words it narrows them.
    assert_eq!(ids(search("", from_alice.clone()).await), vec![image, old]);
    assert_eq!(ids(search("status", from_alice).await), vec![old]);

    let has_file = db::SearchFilters {
        has_file: true,
        ..Default::default()
    };
    assert_eq!(ids(search("", has_file).await), vec![image, file]);

    let window = db::SearchFilters {
        after: Some("2026-01-01T10:00:00+00:00".into()),
        before: Some("2026-03-01T10:00:00+00:00".into()),
        ..Default::default()
    };
    // `after` is inclusive and `before` exclusive, and a fractional second
    // still compares after the whole second it belongs to.
    assert_eq!(ids(search("", window).await), vec![file, old]);

    // No words and no filters is still no search at all.
    assert!(search("", db::SearchFilters::default()).await.is_empty());
}

async fn add_page(db: &db::Db, channel: i32, slug: &str, title: &str, body: &str) {
    match db::create_wiki_page(db, channel, slug, title, body, "alice", 100)
        .await
        .expect("create wiki page")
    {
        db::CreateWikiResult::Created => {}
        _ => panic!("page {slug} should be new"),
    }
}

#[tokio::test]
async fn finds_wiki_pages_by_title_and_body() {
    let (db, channel) = setup().await;
    add_page(
        &db,
        channel,
        "onboarding",
        "Onboarding guide",
        "Ask in the lobby for a server invite.",
    )
    .await;
    add_page(&db, channel, "unrelated", "Something else", "nothing here").await;

    for query in ["onboarding", "ONBOARD", "invite", "server invite"] {
        let hits = db::search_wiki_pages(&db, channel, query, 20)
            .await
            .expect("wiki search");
        assert_eq!(hits.len(), 1, "query {query:?} should match once");
        assert_eq!(hits[0].slug, "onboarding");
        assert_eq!(hits[0].title, "Onboarding guide");
    }

    // The body excerpt is what the search UI shows instead of the page.
    let hits = db::search_wiki_pages(&db, channel, "invite", 20)
        .await
        .expect("wiki search");
    assert!(
        hits[0].snippet.contains("invite"),
        "snippet {:?} should carry the match",
        hits[0].snippet
    );
}

#[tokio::test]
async fn wiki_search_respects_channel_boundaries() {
    let (db, general) = setup().await;
    let other = db::add_channel(&db, "other", None)
        .await
        .expect("create channel")
        .expect("channel is new")
        .id;
    add_page(&db, general, "rules", "House rules", "be excellent").await;

    assert!(
        db::search_wiki_pages(&db, other, "rules", 20)
            .await
            .expect("wiki search")
            .is_empty()
    );
}

#[tokio::test]
async fn wiki_index_follows_edits_and_deletions() {
    let (db, channel) = setup().await;
    add_page(&db, channel, "notes", "Original notes", "first draft").await;

    let saved = db::update_wiki_page(
        &db,
        channel,
        "notes",
        "Revised notes",
        "second draft",
        "bob",
        1,
        50,
    )
    .await
    .expect("update wiki page");
    assert!(matches!(saved, db::UpdateWikiResult::Saved(2)));

    assert!(
        db::search_wiki_pages(&db, channel, "original", 20)
            .await
            .expect("wiki search")
            .is_empty()
    );
    assert_eq!(
        db::search_wiki_pages(&db, channel, "second", 20)
            .await
            .expect("wiki search")
            .len(),
        1
    );

    assert!(
        db::delete_wiki_page(&db, channel, "notes")
            .await
            .expect("delete page")
    );
    assert!(
        db::search_wiki_pages(&db, channel, "second", 20)
            .await
            .expect("wiki search")
            .is_empty()
    );
}

#[tokio::test]
async fn hostile_wiki_queries_neither_error_nor_match_everything() {
    let (db, channel) = setup().await;
    add_page(&db, channel, "plain", "Plain page", "plain body").await;

    for query in ["\"", "*", "AND", "!!!", "( OR )", "title: plain"] {
        assert!(
            db::search_wiki_pages(&db, channel, query, 20).await.is_ok(),
            "query {query:?} must not error"
        );
    }
    assert!(
        db::search_wiki_pages(&db, channel, "!!!", 20)
            .await
            .expect("wiki search")
            .is_empty()
    );
}
