//! Tests for the server-wide upload policy: defaults, persistence and the
//! fixed extension safe-list that no setting may widen — plus the headers
//! `/files` serves uploads back with, the storage quotas, and which files a
//! deleted message takes with it.
//!
//! The file lifecycle fails silently both ways: a file kept forever is a
//! "deleted" photo anyone with the link still opens, and a file swept while
//! a forward or an avatar still names it is a broken image nobody can fix.

use murmer_server::db::{self, UploadConfig};
use murmer_server::upload::{
    DEFAULT_MAX_FILE_SIZE, UPLOAD_CATEGORIES, classify_extension, default_category_ids,
    files_router, is_known_category,
};

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

#[tokio::test]
async fn unconfigured_servers_get_the_permissive_defaults() {
    let db = setup().await;

    let config = db::upload_config(&db).await.expect("load config");
    assert_eq!(config.max_bytes, DEFAULT_MAX_FILE_SIZE as u64);
    assert_eq!(config.categories, default_category_ids());
}

#[tokio::test]
async fn policy_round_trips_through_the_database() {
    let db = setup().await;

    let stored = UploadConfig {
        max_bytes: 2 * 1024 * 1024,
        categories: vec!["images".into(), "documents".into()],
    };
    db::set_upload_config(&db, &stored).await.expect("store");

    assert_eq!(db::upload_config(&db).await.expect("load"), stored);

    // An empty list is a real setting (uploads off), not "unset".
    let disabled = UploadConfig {
        max_bytes: 1024 * 1024,
        categories: Vec::new(),
    };
    db::set_upload_config(&db, &disabled).await.expect("store");
    assert_eq!(db::upload_config(&db).await.expect("load"), disabled);
}

#[tokio::test]
async fn unknown_stored_categories_are_dropped_on_load() {
    let db = setup().await;

    // Simulates a row written by a future build (or tampered with directly):
    // ids this build does not know must never reach the safe-list check.
    db::set_upload_config(
        &db,
        &UploadConfig {
            max_bytes: DEFAULT_MAX_FILE_SIZE as u64,
            categories: vec!["images".into(), "executables".into()],
        },
    )
    .await
    .expect("store");

    let config = db::upload_config(&db).await.expect("load");
    assert_eq!(config.categories, vec!["images".to_string()]);
    assert!(!is_known_category("executables"));
}

#[tokio::test]
async fn active_content_is_never_classified_into_a_category() {
    for filename in [
        "payload.html",
        "payload.htm",
        "payload.svg",
        "payload.js",
        "payload.css",
        "payload.xml",
        "payload.exe",
        "payload.sh",
        "noextension",
    ] {
        assert!(
            classify_extension(filename).is_none(),
            "{filename} must not be on the safe-list"
        );
    }

    assert_eq!(classify_extension("cat.PNG").expect("image").id, "images");
    assert_eq!(
        classify_extension("notes.pdf").expect("doc").id,
        "documents"
    );
    assert_eq!(
        classify_extension("backup.7z").expect("archive").id,
        "archives"
    );
    assert_eq!(classify_extension("song.flac").expect("audio").id, "audio");
    assert_eq!(classify_extension("clip.mkv").expect("video").id, "video");
}

#[tokio::test]
async fn every_category_id_is_unique_and_known() {
    for category in UPLOAD_CATEGORIES {
        assert!(is_known_category(category.id));
        assert!(!category.label.is_empty());
        assert!(!category.extensions.is_empty());
    }
    let mut ids = default_category_ids();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "category ids must be unique");
}

/// Fetch `/<name>` from a `/files` router over `dir` and return the CSP and
/// `Content-Disposition` it answered with.
async fn served_headers(dir: &std::path::Path, name: &str) -> (Option<String>, Option<String>) {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    let response = files_router(dir)
        .oneshot(
            Request::get(format!("/{name}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert!(response.status().is_success(), "{name} was not served");
    let header = |key| {
        response
            .headers()
            .get(key)
            .map(|v: &axum::http::HeaderValue| v.to_str().expect("ascii").to_string())
    };
    (
        header(axum::http::header::CONTENT_SECURITY_POLICY),
        header(axum::http::header::CONTENT_DISPOSITION),
    )
}

#[tokio::test]
async fn files_are_sandboxed_and_only_media_is_served_inline() {
    let dir = std::env::temp_dir().join(format!("murmer-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");
    for name in ["cat.png", "horn.mp3", "clip.mp4", "notes.txt", "report.pdf"] {
        std::fs::write(dir.join(name), b"x").expect("write");
    }

    // Media stays inline: the client embeds it, and a sandbox does not stop
    // an <img> or <audio> element from loading it.
    for name in ["cat.png", "horn.mp3", "clip.mp4"] {
        let (csp, disposition) = served_headers(&dir, name).await;
        assert_eq!(csp.as_deref(), Some("sandbox"), "{name}");
        assert_eq!(disposition, None, "{name}");
    }
    // Everything else downloads instead of rendering in the server's origin.
    for name in ["notes.txt", "report.pdf"] {
        let (csp, disposition) = served_headers(&dir, name).await;
        assert_eq!(csp.as_deref(), Some("sandbox"), "{name}");
        assert_eq!(disposition.as_deref(), Some("attachment"), "{name}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

const MB: u64 = 1024 * 1024;

#[tokio::test]
async fn the_quota_counts_per_user_and_server_wide() {
    let db = setup().await;
    let reserve = |key: &'static str, user: &'static str, size: u64| {
        let db = db.clone();
        async move {
            db::reserve_upload(&db, key, user, size, 10 * MB, 15 * MB)
                .await
                .expect("reserve")
        }
    };

    assert!(reserve("a1", "ada", 6 * MB).await);
    assert!(
        reserve("a2", "ada", 4 * MB).await,
        "exactly at the user quota"
    );
    assert!(!reserve("a3", "ada", 1).await, "one byte past it");
    assert!(reserve("b1", "bob", 5 * MB).await, "the server is now full");
    assert!(!reserve("b2", "bob", 1).await, "and refuses bob too");

    db::forget_upload(&db, "a1").await.expect("forget");
    assert!(
        reserve("b2", "bob", 5 * MB).await,
        "a removed file frees its bytes"
    );

    // Zero is "no limit", not "nothing allowed".
    assert!(
        db::reserve_upload(&db, "c1", "cy", 100 * MB, 0, 0)
            .await
            .expect("reserve")
    );
}

async fn post(db: &db::Db, channel: i32, content: serde_json::Value) -> i64 {
    db::insert_message(db, channel, &content.to_string())
        .await
        .expect("insert")
}

#[tokio::test]
async fn deleting_a_message_releases_its_file_unless_something_else_names_it() {
    let db = setup().await;
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("general");
    for key in ["k-photo", "k-doc", "k-forwarded", "k-unposted"] {
        assert!(
            db::reserve_upload(&db, key, "ada", 1, 0, 0)
                .await
                .expect("reserve")
        );
    }
    // Clients store absolute URLs, so the match is on what follows /files/.
    let photo = post(
        &db,
        channel,
        serde_json::json!({ "user": "ada", "image": "https://h/files/k-photo" }),
    )
    .await;
    let doc = post(
        &db,
        channel,
        serde_json::json!({ "user": "ada", "attachment": { "url": "https://h/files/k-doc", "name": "d.pdf" } }),
    )
    .await;
    let original = post(
        &db,
        channel,
        serde_json::json!({ "user": "ada", "image": "https://h/files/k-forwarded" }),
    )
    .await;
    post(
        &db,
        channel,
        serde_json::json!({ "user": "bob", "image": "https://h/files/k-forwarded" }),
    )
    .await;

    for id in [photo, doc, original] {
        assert!(db::delete_message(&db, id).await.expect("delete"));
    }
    let mut swept = db::take_unreferenced_uploads(&db).await.expect("sweep");
    swept.sort();
    assert_eq!(swept, ["k-doc", "k-photo"]);

    // The forwarded copy kept its file, and nothing was left marked: a
    // second sweep finds nothing until another delete releases it again.
    assert!(
        db::take_unreferenced_uploads(&db)
            .await
            .expect("sweep")
            .is_empty()
    );
}

#[tokio::test]
async fn a_purge_releases_files_the_same_way() {
    let db = setup().await;
    let channel = db::get_channel_id_by_name(&db, "general")
        .await
        .expect("general");
    assert!(
        db::reserve_upload(&db, "k-purged", "ada", 1, 0, 0)
            .await
            .expect("reserve")
    );
    post(
        &db,
        channel,
        serde_json::json!({ "user": "ada", "image": "https://h/files/k-purged" }),
    )
    .await;

    db::purge_all_messages(&db).await.expect("purge");

    assert_eq!(
        db::take_unreferenced_uploads(&db).await.expect("sweep"),
        ["k-purged"]
    );
}
