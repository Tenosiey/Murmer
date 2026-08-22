//! Tests for the server-wide chat policy: persistence, the bounds a stored
//! row is held to, and the ban list the Moderation tab reads.

use murmer_server::db::{
    self, ChatSettings, MAX_MESSAGE_LENGTH, MAX_SLOW_MODE_SECONDS, MIN_CONFIGURABLE_MESSAGE_LENGTH,
};

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

#[tokio::test]
async fn unconfigured_servers_get_the_permissive_defaults() {
    let db = setup().await;

    let settings = db::chat_settings(&db).await.expect("load settings");
    assert_eq!(settings, ChatSettings::default());
    assert_eq!(settings.slow_mode_seconds, 0);
    assert_eq!(settings.max_message_length, MAX_MESSAGE_LENGTH);
    assert!(!settings.profanity_filter);
    assert!(settings.profanity_words.is_empty());
}

#[tokio::test]
async fn policy_round_trips_through_the_database() {
    let db = setup().await;

    let stored = ChatSettings {
        slow_mode_seconds: 30,
        max_message_length: 500,
        profanity_filter: true,
        profanity_words: vec!["damn".into(), "heck".into()],
    };
    let written = db::set_chat_settings(&db, &stored).await.expect("store");
    assert_eq!(written, stored);
    assert_eq!(db::chat_settings(&db).await.expect("load"), stored);

    // An empty word list with the filter on is a real state, not "unset".
    let empty = ChatSettings {
        profanity_filter: true,
        profanity_words: Vec::new(),
        ..stored
    };
    db::set_chat_settings(&db, &empty).await.expect("store");
    assert_eq!(db::chat_settings(&db).await.expect("load"), empty);
}

#[tokio::test]
async fn stored_values_are_clamped_on_the_way_in_and_out() {
    let db = setup().await;

    // A setting may only ever lower the hard message cap, and slow mode is
    // bounded no matter what a client asks for.
    let written = db::set_chat_settings(
        &db,
        &ChatSettings {
            slow_mode_seconds: MAX_SLOW_MODE_SECONDS * 4,
            max_message_length: MAX_MESSAGE_LENGTH * 4,
            profanity_filter: true,
            profanity_words: vec!["Damn".into(), "damn".into(), " HECK ".into()],
        },
    )
    .await
    .expect("store");
    assert_eq!(written.slow_mode_seconds, MAX_SLOW_MODE_SECONDS);
    assert_eq!(written.max_message_length, MAX_MESSAGE_LENGTH);
    assert_eq!(written.profanity_words, vec!["damn", "heck"]);

    // A row written by another build (or by hand) is held to the same bounds
    // on read, so the filter and the cap can never be handed nonsense.
    let loaded = db::chat_settings(&db).await.expect("load");
    assert_eq!(loaded, written);

    let tiny = db::set_chat_settings(
        &db,
        &ChatSettings {
            max_message_length: 1,
            ..ChatSettings::default()
        },
    )
    .await
    .expect("store");
    assert_eq!(tiny.max_message_length, MIN_CONFIGURABLE_MESSAGE_LENGTH);
}

#[tokio::test]
async fn voice_defaults_round_trip_and_keep_lossless() {
    let db = setup().await;

    assert_eq!(
        db::voice_defaults(&db).await.expect("load"),
        db::VoiceDefaults::default()
    );

    let high = db::VoiceDefaults {
        quality: "high".into(),
        bitrate: Some(96_000),
    };
    db::set_voice_defaults(&db, &high).await.expect("store");
    assert_eq!(db::voice_defaults(&db).await.expect("load"), high);

    // "Lossless" is a real default, and must not come back as the fallback
    // bitrate just because it is stored as the absence of one.
    let lossless = db::VoiceDefaults {
        quality: "lossless".into(),
        bitrate: None,
    };
    db::set_voice_defaults(&db, &lossless).await.expect("store");
    assert_eq!(db::voice_defaults(&db).await.expect("load"), lossless);
}

#[tokio::test]
async fn ban_list_reports_who_is_banned_and_by_whom() {
    let db = setup().await;

    assert!(db::list_bans(&db).await.expect("list").is_empty());

    db::add_ban(&db, "key-a", "spammer", "mod")
        .await
        .expect("ban");
    db::add_ban(&db, "key-b", "raider", "owner")
        .await
        .expect("ban");

    let bans = db::list_bans(&db).await.expect("list");
    assert_eq!(bans.len(), 2);
    let spammer = bans
        .iter()
        .find(|ban| ban.user_name == "spammer")
        .expect("spammer listed");
    assert_eq!(spammer.public_key, "key-a");
    assert_eq!(spammer.banned_by, "mod");

    // Lifting a ban removes the row the dashboard renders.
    assert!(db::remove_ban_by_name(&db, "spammer").await.expect("unban"));
    let bans = db::list_bans(&db).await.expect("list");
    assert_eq!(bans.len(), 1);
    assert_eq!(bans[0].user_name, "raider");
}
