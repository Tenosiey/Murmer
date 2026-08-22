//! Wiki page persistence: the revision compare-and-swap, the history the
//! client's revision view reads and the restore that re-applies an old
//! version. These are the paths where a mistake silently loses somebody's
//! writing, and none of them are visible in a smoke test.

use murmer_server::db;

const MAX_REVISIONS: i64 = 50;

async fn setup() -> (db::Db, i32) {
    let db = db::init(":memory:").await.expect("in-memory db");
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("default channel exists");
    (db, channel)
}

async fn create(db: &db::Db, channel: i32, slug: &str, body: &str) {
    match db::create_wiki_page(db, channel, slug, "Home", body, "alice", 100)
        .await
        .expect("create page")
    {
        db::CreateWikiResult::Created => {}
        _ => panic!("page {slug} should be new"),
    }
}

async fn save(
    db: &db::Db,
    channel: i32,
    slug: &str,
    title: &str,
    body: &str,
    editor: &str,
    expected: i64,
) -> db::UpdateWikiResult {
    db::update_wiki_page(
        db,
        channel,
        slug,
        title,
        body,
        editor,
        expected,
        MAX_REVISIONS,
    )
    .await
    .expect("update page")
}

async fn history(db: &db::Db, channel: i32, slug: &str) -> Vec<db::WikiRevisionMeta> {
    db::list_wiki_revisions(db, channel, slug, MAX_REVISIONS)
        .await
        .expect("list revisions")
}

#[tokio::test]
async fn history_records_every_version_newest_first() {
    let (db, channel) = setup().await;
    create(&db, channel, "home", "first").await;
    save(&db, channel, "home", "Home", "second", "bob", 1).await;
    save(&db, channel, "home", "Renamed", "third", "carol", 2).await;

    let revisions = history(&db, channel, "home").await;
    let numbers: Vec<i64> = revisions.iter().map(|r| r.revision).collect();
    assert_eq!(numbers, vec![3, 2, 1]);
    assert_eq!(revisions[0].title, "Renamed");
    assert_eq!(revisions[0].author, "carol");
    // The size is reported in bytes, which is the unit the editor's cap uses.
    assert_eq!(revisions[2].bytes, "first".len() as i64);

    let first = db::get_wiki_revision(&db, channel, "home", 1)
        .await
        .expect("load revision")
        .expect("revision 1 exists");
    assert_eq!(first.body, "first");
    assert_eq!(first.author, "alice");
}

#[tokio::test]
async fn history_is_scoped_to_its_channel() {
    let (db, general) = setup().await;
    let other = db::add_channel(&db, "other", None)
        .await
        .expect("create channel")
        .expect("channel is new")
        .id;
    create(&db, general, "home", "general body").await;
    create(&db, other, "home", "other body").await;

    let revisions = history(&db, other, "home").await;
    assert_eq!(revisions.len(), 1);
    let page = db::get_wiki_revision(&db, other, "home", 1)
        .await
        .expect("load revision")
        .expect("revision 1 exists");
    assert_eq!(page.body, "other body");
    assert!(history(&db, general, "missing").await.is_empty());
}

#[tokio::test]
async fn stale_save_conflicts_without_touching_history() {
    let (db, channel) = setup().await;
    create(&db, channel, "home", "first").await;
    save(&db, channel, "home", "Home", "second", "bob", 1).await;

    // Carol started editing from revision 1, which bob has since replaced.
    match save(&db, channel, "home", "Home", "carol's text", "carol", 1).await {
        db::UpdateWikiResult::Conflict(current) => {
            assert_eq!(current.revision, 2);
            assert_eq!(current.body, "second");
        }
        _ => panic!("a stale base revision must conflict"),
    }
    assert_eq!(history(&db, channel, "home").await.len(), 2);
}

#[tokio::test]
async fn restore_reapplies_an_old_version_as_a_new_revision() {
    let (db, channel) = setup().await;
    create(&db, channel, "home", "first").await;
    save(&db, channel, "home", "Vandalised", "junk", "mallory", 1).await;

    match db::restore_wiki_revision(&db, channel, "home", 1, "carol", 2, MAX_REVISIONS)
        .await
        .expect("restore")
    {
        db::RestoreWikiResult::Saved(revision) => assert_eq!(revision, 3),
        _ => panic!("restore should have applied"),
    }

    let page = db::get_wiki_page(&db, channel, "home")
        .await
        .expect("load page")
        .expect("page exists");
    assert_eq!(page.body, "first");
    assert_eq!(page.title, "Home");
    // The restorer, not the original author, is who changed the page.
    assert_eq!(page.updated_by, "carol");
    assert_eq!(page.revision, 3);

    // History is appended to, never rewound: the vandalism stays readable and
    // the restore is itself undoable.
    let numbers: Vec<i64> = history(&db, channel, "home")
        .await
        .iter()
        .map(|r| r.revision)
        .collect();
    assert_eq!(numbers, vec![3, 2, 1]);
    let junk = db::get_wiki_revision(&db, channel, "home", 2)
        .await
        .expect("load revision")
        .expect("revision 2 exists");
    assert_eq!(junk.body, "junk");
}

#[tokio::test]
async fn restore_respects_the_compare_and_swap() {
    let (db, channel) = setup().await;
    create(&db, channel, "home", "first").await;
    save(&db, channel, "home", "Home", "second", "bob", 1).await;

    // Carol was looking at revision 1 when she asked for the restore.
    match db::restore_wiki_revision(&db, channel, "home", 1, "carol", 1, MAX_REVISIONS)
        .await
        .expect("restore")
    {
        db::RestoreWikiResult::Conflict(current) => assert_eq!(current.revision, 2),
        _ => panic!("a stale base revision must conflict"),
    }
    assert_eq!(history(&db, channel, "home").await.len(), 2);
}

#[tokio::test]
async fn restore_tells_a_missing_revision_from_a_missing_page() {
    let (db, channel) = setup().await;
    create(&db, channel, "home", "first").await;

    assert!(matches!(
        db::restore_wiki_revision(&db, channel, "home", 99, "carol", 1, MAX_REVISIONS)
            .await
            .expect("restore"),
        db::RestoreWikiResult::RevisionNotFound
    ));
    assert!(matches!(
        db::restore_wiki_revision(&db, channel, "gone", 1, "carol", 1, MAX_REVISIONS)
            .await
            .expect("restore"),
        db::RestoreWikiResult::NotFound
    ));
}

#[tokio::test]
async fn old_revisions_are_pruned_to_the_cap() {
    let (db, channel) = setup().await;
    create(&db, channel, "home", "v1").await;
    for revision in 1..=4 {
        db::update_wiki_page(
            &db,
            channel,
            "home",
            "Home",
            &format!("v{}", revision + 1),
            "bob",
            revision,
            // Keep only the newest three versions.
            3,
        )
        .await
        .expect("update page");
    }

    let numbers: Vec<i64> = history(&db, channel, "home")
        .await
        .iter()
        .map(|r| r.revision)
        .collect();
    assert_eq!(numbers, vec![5, 4, 3]);
    assert!(
        db::get_wiki_revision(&db, channel, "home", 1)
            .await
            .expect("load revision")
            .is_none()
    );
}
