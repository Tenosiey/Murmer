//! The screen share bitrate cap. It is one value in the `server_settings`
//! key-value table shared with the server identity, the upload policy and the
//! chat policy, and it is stored as text — so what needs pinning down is the
//! encoding of "no cap", that a stored value survives the round trip
//! unchanged, and that writing it leaves its table neighbours alone. Nothing
//! here shows up in a smoke test: an uncapped share looks exactly like a
//! capped one until somebody watches their upstream.

use murmer_server::db::{self, DbCall};
use rusqlite::OptionalExtension;

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

async fn cap(db: &db::Db) -> Option<u64> {
    db::screenshare_max_bitrate(db)
        .await
        .expect("load bitrate cap")
}

async fn set_cap(db: &db::Db, bitrate: Option<u64>) {
    db::set_screenshare_max_bitrate(db, bitrate)
        .await
        .expect("store bitrate cap");
}

/// Read the raw `server_settings` value, which is what the storage format
/// claims about "no cap".
async fn raw_setting(db: &db::Db, key: &'static str) -> Option<String> {
    db.call_db(move |conn| {
        conn.query_row(
            "SELECT value FROM server_settings WHERE key = ?1",
            (key,),
            |row| row.get::<_, String>(0),
        )
        .optional()
    })
    .await
    .expect("read setting")
}

#[tokio::test]
async fn a_fresh_server_is_uncapped() {
    let db = setup().await;
    assert_eq!(cap(&db).await, None);
}

#[tokio::test]
async fn a_cap_round_trips_and_can_be_replaced() {
    let db = setup().await;

    set_cap(&db, Some(2_500_000)).await;
    assert_eq!(cap(&db).await, Some(2_500_000));

    set_cap(&db, Some(8_000_000)).await;
    assert_eq!(cap(&db).await, Some(8_000_000));

    // The upsert has to replace the row rather than add a second one, or a
    // later read picks whichever the query happens to hit first.
    let rows: i64 = db
        .call_db(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM server_settings WHERE key = 'screenshare_max_bitrate'",
                [],
                |row| row.get(0),
            )
        })
        .await
        .expect("count rows");
    assert_eq!(rows, 1);
}

/// The cap is stored as text, so it is not bounded by SQLite's signed
/// integer column type — a value past `i64::MAX` must come back as itself
/// rather than wrapping into a negative one.
#[tokio::test]
async fn values_beyond_the_signed_integer_range_survive() {
    let db = setup().await;
    set_cap(&db, Some(u64::MAX)).await;
    assert_eq!(cap(&db).await, Some(u64::MAX));
}

/// Removing the cap writes `0` rather than deleting the row, and `0` is how
/// the reader recognises "uncapped" — including when a caller passes it
/// directly instead of `None`.
#[tokio::test]
async fn zero_and_none_both_mean_uncapped() {
    let db = setup().await;

    set_cap(&db, Some(4_000_000)).await;
    set_cap(&db, None).await;
    assert_eq!(cap(&db).await, None);
    assert_eq!(
        raw_setting(&db, "screenshare_max_bitrate").await,
        Some("0".to_owned())
    );

    set_cap(&db, Some(4_000_000)).await;
    set_cap(&db, Some(0)).await;
    assert_eq!(cap(&db).await, None);
}

/// A value the reader cannot parse — an older format, a hand-edited
/// database — degrades to "uncapped" instead of failing the whole
/// `screenshare-config` frame every client gets after authenticating.
#[tokio::test]
async fn an_unparsable_stored_value_reads_as_uncapped() {
    let db = setup().await;
    for stored in ["", "not a number", "-1", "2500000.5"] {
        db.call_db(move |conn| {
            conn.execute(
                "INSERT INTO server_settings (key, value) VALUES ('screenshare_max_bitrate', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (stored,),
            )
        })
        .await
        .expect("store raw value");
        assert_eq!(cap(&db).await, None, "{stored:?} should read as uncapped");
    }
}

/// Everything in `server_settings` shares one table, so a write that reached
/// past its own key would quietly clear the server's name or its chat policy.
#[tokio::test]
async fn the_cap_shares_its_table_without_disturbing_it() {
    let db = setup().await;

    db::set_server_identity_fields(
        &db,
        vec![
            (db::IDENTITY_NAME_KEY, "Murmer HQ".to_owned()),
            (db::IDENTITY_WELCOME_KEY, "hello there".to_owned()),
        ],
    )
    .await
    .expect("store identity");
    let chat = db::ChatSettings {
        slow_mode_seconds: 15,
        ..db::ChatSettings::default()
    };
    db::set_chat_settings(&db, &chat).await.expect("store chat");

    set_cap(&db, Some(3_000_000)).await;
    set_cap(&db, None).await;

    let identity = db::get_server_identity(&db).await.expect("load identity");
    assert_eq!(identity.name, "Murmer HQ");
    assert_eq!(identity.welcome_message, "hello there");
    assert_eq!(
        db::chat_settings(&db).await.expect("load chat"),
        chat,
        "the chat policy is not the cap's to touch"
    );

    // And the neighbours' writes leave the cap alone in turn.
    set_cap(&db, Some(3_000_000)).await;
    db::set_server_identity_fields(
        &db,
        vec![(db::IDENTITY_DESCRIPTION_KEY, "a test server".to_owned())],
    )
    .await
    .expect("store identity");
    assert_eq!(cap(&db).await, Some(3_000_000));
}
