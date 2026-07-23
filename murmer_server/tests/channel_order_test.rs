//! Tests for the custom sort order of channels and categories.

use murmer_server::db;

/// New channels append at the end of their category scope, independently of
/// their names.
#[tokio::test]
async fn new_channels_append_per_category() {
    let db = db::init(":memory:").await.expect("in-memory db");

    // The seeded "general" channel occupies position 0 outside any category.
    let zebra = db::add_channel(&db, "zebra", None)
        .await
        .expect("add")
        .expect("created");
    let alpha = db::add_channel(&db, "alpha", None)
        .await
        .expect("add")
        .expect("created");
    assert_eq!(zebra.position, 1);
    assert_eq!(alpha.position, 2);

    // A separate category starts its own position sequence.
    let (cat, cat_pos) = db::add_category(&db, "Team", None).await.expect("category");
    assert_eq!(cat_pos, 0);
    let in_cat = db::add_channel(&db, "inside", Some(cat))
        .await
        .expect("add")
        .expect("created");
    assert_eq!(in_cat.position, 0);

    let names: Vec<String> = db::get_channels(&db)
        .await
        .into_iter()
        .map(|c| c.name)
        .collect();
    // Ordered by position then name: general(0), inside(0), zebra(1), alpha(2).
    assert_eq!(names, ["general", "inside", "zebra", "alpha"]);
}

/// Moving a channel into a category appends it after the existing members.
#[tokio::test]
async fn move_appends_to_target_category() {
    let db = db::init(":memory:").await.expect("in-memory db");

    let (cat, _) = db::add_category(&db, "Team", None).await.expect("category");
    let first = db::add_channel(&db, "first", Some(cat))
        .await
        .expect("add")
        .expect("created");
    let second = db::add_channel(&db, "second", None)
        .await
        .expect("add")
        .expect("created");

    let pos = db::move_channel(&db, second.id, Some(cat))
        .await
        .expect("move")
        .expect("exists");
    assert_eq!(pos, first.position + 1);

    // Moving a missing channel reports None instead of failing.
    assert_eq!(db::move_channel(&db, 9999, None).await.expect("move"), None);
}

/// Reordering rewrites category assignment and positions in list order and
/// rejects unknown ids without applying a partial update.
#[tokio::test]
async fn reorder_channels_sets_category_and_positions() {
    let db = db::init(":memory:").await.expect("in-memory db");

    let (cat, _) = db::add_category(&db, "Team", None).await.expect("category");
    let a = db::add_channel(&db, "a", None)
        .await
        .expect("add")
        .expect("created");
    let b = db::add_channel(&db, "b", None)
        .await
        .expect("add")
        .expect("created");

    // Move both into the category, b before a.
    assert!(
        db::reorder_channels(&db, Some(cat), vec![b.id, a.id], false)
            .await
            .expect("reorder")
    );
    let channels = db::get_channels(&db).await;
    let b_row = channels.iter().find(|c| c.id == b.id).expect("b");
    let a_row = channels.iter().find(|c| c.id == a.id).expect("a");
    assert_eq!((b_row.category_id, b_row.position), (Some(cat), 0));
    assert_eq!((a_row.category_id, a_row.position), (Some(cat), 1));

    // An unknown id rolls the whole reorder back.
    assert!(
        !db::reorder_channels(&db, None, vec![a.id, 9999], false)
            .await
            .expect("reorder")
    );
    let unchanged = db::get_channels(&db).await;
    let a_row = unchanged.iter().find(|c| c.id == a.id).expect("a");
    assert_eq!((a_row.category_id, a_row.position), (Some(cat), 1));
}

/// Voice channels reorder through the same call with the voice flag.
#[tokio::test]
async fn reorder_voice_channels() {
    let db = db::init(":memory:").await.expect("in-memory db");

    let lobby = db::add_voice_channel(&db, "Lobby", "standard", None, None)
        .await
        .expect("add")
        .expect("created");
    let games = db::add_voice_channel(&db, "Games", "standard", None, None)
        .await
        .expect("add")
        .expect("created");
    assert_eq!((lobby.position, games.position), (0, 1));

    assert!(
        db::reorder_channels(&db, None, vec![games.id, lobby.id], true)
            .await
            .expect("reorder")
    );
    let names: Vec<String> = db::get_voice_channels(&db)
        .await
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["Games", "Lobby"]);
}

/// Renaming reports the distinct outcomes the handler relies on: success, a
/// name already taken by another channel, and an unknown id. Names are unique,
/// so the taken-name case must not clobber the existing channel.
#[tokio::test]
async fn rename_channel_outcomes() {
    let db = db::init(":memory:").await.expect("in-memory db");

    let alpha = db::add_channel(&db, "alpha", None)
        .await
        .expect("add")
        .expect("created");
    db::add_channel(&db, "beta", None)
        .await
        .expect("add")
        .expect("created");

    // A free name renames the row and returns the updated record.
    match db::rename_channel(&db, alpha.id, "gamma")
        .await
        .expect("rename")
    {
        db::RenameResult::Renamed(record) => assert_eq!(record.name, "gamma"),
        _ => panic!("expected rename to succeed"),
    }

    // Colliding with another channel's name leaves both untouched.
    assert!(matches!(
        db::rename_channel(&db, alpha.id, "beta")
            .await
            .expect("rename"),
        db::RenameResult::NameTaken
    ));
    let names: Vec<String> = db::get_channels(&db)
        .await
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(names.contains(&"gamma".to_string()) && names.contains(&"beta".to_string()));

    // Renaming to the same current name is a no-op success, not a conflict.
    assert!(matches!(
        db::rename_channel(&db, alpha.id, "gamma")
            .await
            .expect("rename"),
        db::RenameResult::Renamed(_)
    ));

    // A missing id is reported so the handler can answer "unknown channel".
    assert!(matches!(
        db::rename_channel(&db, 9999, "whatever")
            .await
            .expect("rename"),
        db::RenameResult::NotFound
    ));
}

/// Voice channels rename through the parallel call with the same outcomes.
#[tokio::test]
async fn rename_voice_channel_outcomes() {
    let db = db::init(":memory:").await.expect("in-memory db");

    let lobby = db::add_voice_channel(&db, "Lobby", "standard", None, None)
        .await
        .expect("add")
        .expect("created");
    db::add_voice_channel(&db, "Games", "standard", None, None)
        .await
        .expect("add")
        .expect("created");

    match db::rename_voice_channel(&db, lobby.id, "Lounge")
        .await
        .expect("rename")
    {
        db::RenameResult::Renamed(record) => assert_eq!(record.name, "Lounge"),
        _ => panic!("expected rename to succeed"),
    }

    assert!(matches!(
        db::rename_voice_channel(&db, lobby.id, "Games")
            .await
            .expect("rename"),
        db::RenameResult::NameTaken
    ));

    assert!(matches!(
        db::rename_voice_channel(&db, 9999, "whatever")
            .await
            .expect("rename"),
        db::RenameResult::NotFound
    ));
}

/// Categories append on create and reorder as a whole list.
#[tokio::test]
async fn reorder_categories() {
    let db = db::init(":memory:").await.expect("in-memory db");

    let (first, p1) = db::add_category(&db, "First", None).await.expect("add");
    let (second, p2) = db::add_category(&db, "Second", None).await.expect("add");
    assert_eq!((p1, p2), (0, 1));

    assert!(
        db::reorder_categories(&db, vec![second, first])
            .await
            .expect("reorder")
    );
    let names: Vec<String> = db::get_categories(&db)
        .await
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["Second", "First"]);

    // Unknown category ids roll back.
    assert!(
        !db::reorder_categories(&db, vec![first, 9999])
            .await
            .expect("reorder")
    );
}
