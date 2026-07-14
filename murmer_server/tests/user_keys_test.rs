use murmer_server::db;

#[tokio::test]
async fn first_key_claims_a_name_until_unbound() {
    let db = db::init(":memory:").await.expect("in-memory db");

    assert_eq!(db::get_user_key(&db, "alice").await.expect("lookup"), None);

    db::bind_user_key(&db, "alice", "key-a")
        .await
        .expect("bind");
    assert_eq!(
        db::get_user_key(&db, "alice").await.expect("lookup"),
        Some("key-a".to_string())
    );

    // A later claim with a different key must not overwrite the binding.
    db::bind_user_key(&db, "alice", "key-b")
        .await
        .expect("rebind attempt");
    assert_eq!(
        db::get_user_key(&db, "alice").await.expect("lookup"),
        Some("key-a".to_string())
    );

    // Unbinding releases the name for a new key.
    assert!(db::unbind_user_name(&db, "alice").await.expect("unbind"));
    assert!(!db::unbind_user_name(&db, "alice")
        .await
        .expect("second unbind"));
    db::bind_user_key(&db, "alice", "key-b")
        .await
        .expect("bind new key");
    assert_eq!(
        db::get_user_key(&db, "alice").await.expect("lookup"),
        Some("key-b".to_string())
    );
}
