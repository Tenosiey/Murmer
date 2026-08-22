//! Tests for the Danger Zone actions: purging every message and resetting the
//! server's structure. Both are irreversible, so what they *keep* matters as
//! much as what they remove.

use murmer_server::db;
use murmer_server::permissions::DEFAULT_MOD;

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

async fn general_id(db: &db::Db) -> i32 {
    db::get_channel_id_by_name(db, "general")
        .await
        .expect("general channel")
}

#[tokio::test]
async fn purge_removes_every_message_pin_and_reaction() {
    let db = setup().await;
    let channel = general_id(&db).await;

    let first = db::insert_message(&db, channel, r#"{"user":"a","text":"hello"}"#)
        .await
        .expect("insert");
    db::insert_message(&db, channel, r#"{"user":"b","text":"there"}"#)
        .await
        .expect("insert");
    db::add_pin(&db, first, channel, "mod", 25)
        .await
        .expect("pin");
    db::add_reaction(&db, first, "a", "👍")
        .await
        .expect("react");

    assert_eq!(db::purge_all_messages(&db).await.expect("purge"), 2);

    assert!(
        db::fetch_history(&db, channel, None, 50)
            .await
            .expect("history")
            .is_empty()
    );
    assert!(
        db::get_pins_for_channel(&db, channel)
            .await
            .expect("pins")
            .is_empty()
    );
    // The full-text index follows the messages through its delete trigger, so
    // a purged message can not come back as a search hit.
    assert!(
        db::search_messages(&db, channel, "hello", 50)
            .await
            .expect("search")
            .is_empty()
    );
    // The channel itself survives a purge — only its contents are gone.
    assert!(db::get_channels(&db).await.iter().any(|c| c.id == channel));
}

#[tokio::test]
async fn reset_clears_the_structure_but_keeps_general_and_the_protected_roles() {
    let db = setup().await;
    let channel = general_id(&db).await;

    let (category, _) = db::add_category(&db, "Text", None).await.expect("category");
    db::add_channel(&db, "random", Some(category))
        .await
        .expect("channel")
        .expect("created");
    db::add_voice_channel(&db, "Lounge", "standard", Some(64_000), Some(category))
        .await
        .expect("voice channel");
    db::insert_message(&db, channel, r#"{"user":"a","text":"hello"}"#)
        .await
        .expect("insert");
    db::create_wiki_page(&db, channel, "guide", "Guide", "body", "a", 100)
        .await
        .expect("wiki page");
    let custom = db::create_role_def(&db, "Dude", None, DEFAULT_MOD, 1)
        .await
        .expect("role");

    let summary = db::reset_server(&db).await.expect("reset");
    assert_eq!(summary.messages, 1);
    assert_eq!(summary.channels, 1);
    assert_eq!(summary.voice_channels, 1);
    assert_eq!(summary.categories, 1);
    assert!(summary.roles >= 1);

    // `general` is the one channel a client can always assume exists.
    let channels = db::get_channels(&db).await;
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].name, "general");
    assert!(channels[0].category_id.is_none());
    assert!(db::get_voice_channels(&db).await.is_empty());
    assert!(db::get_categories(&db).await.is_empty());
    assert!(
        db::fetch_history(&db, channel, None, 50)
            .await
            .expect("history")
            .is_empty()
    );
    assert!(
        db::list_wiki_pages(&db, channel)
            .await
            .expect("wiki index")
            .is_empty()
    );

    // Deleting the Owner and @everyone roles would leave the administrator
    // who ran the reset unable to administer anything, so they are kept —
    // every other role, including the one created above, is gone.
    let roles = db::list_role_defs(&db).await.expect("roles");
    assert!(roles.iter().any(|role| role.is_owner));
    assert!(roles.iter().any(|role| role.is_default));
    assert!(!roles.iter().any(|role| role.id == custom));
    assert!(roles.iter().all(|role| role.is_owner || role.is_default));
}

#[tokio::test]
async fn reset_keeps_identities_bans_and_emojis() {
    let db = setup().await;

    db::add_ban(&db, "key", "spammer", "mod")
        .await
        .expect("ban");
    db::add_emoji(&db, "party", "/files/party.png", "mod")
        .await
        .expect("emoji");

    db::reset_server(&db).await.expect("reset");

    // "Start over" is about the server's structure. Losing who people are,
    // who was banned or what emojis exist is never what it is asked to mean.
    assert_eq!(db::list_bans(&db).await.expect("bans").len(), 1);
    assert_eq!(db::get_emojis(&db).await.expect("emojis").len(), 1);
}
