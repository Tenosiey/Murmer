//! Tests for per-user avatar persistence.

use murmer_server::db;

#[tokio::test]
async fn avatar_round_trip() {
    let db = db::init(":memory:").await.expect("in-memory db");

    // No binding row yet: the avatar cannot be stored.
    assert!(
        !db::set_user_avatar(&db, "alice", "/files/1-a.png")
            .await
            .expect("set")
    );

    assert!(
        db::bind_user_key(&db, "alice", "pk-alice")
            .await
            .expect("bind")
    );
    assert_eq!(db::get_user_avatar(&db, "alice").await.expect("get"), None);

    assert!(
        db::set_user_avatar(&db, "alice", "/files/1-a.png")
            .await
            .expect("set")
    );
    assert_eq!(
        db::get_user_avatar(&db, "alice").await.expect("get"),
        Some("/files/1-a.png".to_string())
    );

    // Clearing stores the empty string, which reads back as unset.
    assert!(db::set_user_avatar(&db, "alice", "").await.expect("clear"));
    assert_eq!(db::get_user_avatar(&db, "alice").await.expect("get"), None);
}

#[tokio::test]
async fn avatar_snapshot_and_reference_counting() {
    let db = db::init(":memory:").await.expect("in-memory db");

    db::bind_user_key(&db, "alice", "pk-alice")
        .await
        .expect("bind");
    db::bind_user_key(&db, "bob", "pk-bob").await.expect("bind");
    db::set_user_avatar(&db, "alice", "/files/1-shared.png")
        .await
        .expect("set");
    db::set_user_avatar(&db, "bob", "/files/1-shared.png")
        .await
        .expect("set");

    let mut snapshot = db::get_all_avatars(&db).await.expect("snapshot");
    snapshot.sort();
    assert_eq!(
        snapshot,
        [
            ("alice".to_string(), "/files/1-shared.png".to_string()),
            ("bob".to_string(), "/files/1-shared.png".to_string()),
        ]
    );

    // Two references; after alice clears hers, one remains — the file must
    // not be deleted until the count reaches zero.
    assert_eq!(
        db::count_avatar_references(&db, "/files/1-shared.png")
            .await
            .expect("count"),
        2
    );
    db::set_user_avatar(&db, "alice", "").await.expect("clear");
    assert_eq!(
        db::count_avatar_references(&db, "/files/1-shared.png")
            .await
            .expect("count"),
        1
    );
}
