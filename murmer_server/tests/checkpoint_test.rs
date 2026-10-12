//! A checkpoint must leave `murmer.db` self-contained, because operators back
//! up by copying that one file.

use murmer_server::db;

#[tokio::test]
async fn checkpoint_folds_wal_into_main_file() {
    let dir = std::env::temp_dir().join(format!("murmer-checkpoint-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("murmer.db");
    let database = db::init(path.to_str().unwrap()).await.expect("open db");

    let wal = dir.join("murmer.db-wal");
    assert!(
        std::fs::metadata(&wal).unwrap().len() > 0,
        "schema sits in the WAL"
    );

    db::checkpoint(&database).await.expect("checkpoint");
    assert_eq!(std::fs::metadata(&wal).unwrap().len(), 0);

    // A copy of the main file alone, without the WAL, holds the schema.
    let copy = dir.join("copy.db");
    std::fs::copy(&path, &copy).unwrap();
    let conn = rusqlite::Connection::open(&copy).unwrap();
    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(tables > 0);

    drop(database);
    let _ = std::fs::remove_dir_all(&dir);
}
