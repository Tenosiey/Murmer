use murmer_server::db;

/// The startup sweep (`resume_ephemeral_deletions`) relies on this scan to
/// find ephemeral messages whose in-memory deletion timer was lost to a
/// restart.
#[tokio::test]
async fn ephemeral_scan_returns_only_flagged_messages() {
    let db = db::init(":memory:").await.expect("in-memory db");
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("default channel exists");

    db::insert_message(&db, channel, r#"{"type":"chat","user":"a","text":"hello"}"#)
        .await
        .expect("insert plain message");
    let ephemeral_id = db::insert_message(
        &db,
        channel,
        r#"{"type":"chat","user":"a","text":"psst","ephemeral":true,"expiresAt":"2000-01-01T00:00:00Z"}"#,
    )
    .await
    .expect("insert ephemeral message");
    // A message that merely quotes the flag in its text must not match: the
    // quotes inside the JSON string are escaped in the stored serialization,
    // so the LIKE prefilter cannot hit them.
    db::insert_message(
        &db,
        channel,
        &serde_json::json!({
            "type": "chat",
            "user": "a",
            "text": r#"set "ephemeral":true to hide a message"#,
        })
        .to_string(),
    )
    .await
    .expect("insert message quoting the flag");

    let rows = db::get_ephemeral_messages(&db).await.expect("scan");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, ephemeral_id);
    assert_eq!(rows[0].1, channel);

    let removed = db::delete_message(&db, ephemeral_id as i32)
        .await
        .expect("delete");
    assert!(removed);
    assert!(db::get_ephemeral_messages(&db)
        .await
        .expect("rescan")
        .is_empty());
}
