//! A deleted message must not survive in the database file, because
//! operators copy that file for backups and a copy can leak.

use murmer_server::db;

const MARKER: &[u8] = b"zqxsecretmarkerzqx";

fn file_contains(path: &std::path::Path, needle: &[u8]) -> bool {
    std::fs::read(path)
        .map(|bytes| bytes.windows(needle.len()).any(|w| w == needle))
        .unwrap_or(false)
}

#[tokio::test]
async fn deleted_message_leaves_no_trace_on_disk() {
    let dir = std::env::temp_dir().join(format!("murmer-secure-delete-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("murmer.db");
    let database = db::init(path.to_str().unwrap()).await.expect("open db");
    let channel = db::get_channel_id_by_name(&database, "general")
        .await
        .expect("default channel exists");

    let text = std::str::from_utf8(MARKER).unwrap();
    let content = serde_json::json!({"type": "chat", "user": "alice", "text": text}).to_string();
    let id = db::insert_message(&database, channel, &content)
        .await
        .expect("insert");
    db::checkpoint(&database).await.expect("checkpoint");
    assert!(file_contains(&path, MARKER), "marker reached the main file");

    assert!(db::delete_message(&database, id).await.expect("delete"));
    db::checkpoint(&database).await.expect("checkpoint");
    assert!(!file_contains(&path, MARKER), "deleted text still on disk");
    assert!(!file_contains(&dir.join("murmer.db-wal"), MARKER));

    drop(database);
    let _ = std::fs::remove_dir_all(&dir);
}
