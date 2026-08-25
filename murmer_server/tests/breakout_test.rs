//! Breakout rooms are voice channels that are supposed to disappear. Two
//! things about that are invisible until they go wrong, and neither shows up
//! in a smoke test where somebody opens a split and closes it again a minute
//! later:
//!
//! - **The startup sweep.** A server that dies mid-split leaves its rooms in
//!   the table. Nobody is in them any more, nobody remembers opening them,
//!   and nothing else will ever delete them — so a sweep that stopped working
//!   looks exactly like a server whose users happen to tidy up after
//!   themselves. Their permission overrides have to go with them: those are
//!   keyed by channel id, and SQLite hands ids out again.
//! - **The generated room name.** Voice channel names are capped and rejected
//!   outright when they are too long, so a long parent name is the case where
//!   a split silently produces fewer rooms than were asked for.

use murmer_server::channel_overrides::ChannelKind;
use murmer_server::db;
use murmer_server::security::validate_channel_name;
use murmer_server::ws::validation::{breakout_room_count, breakout_room_name};

async fn setup() -> db::Db {
    db::init(":memory:").await.expect("in-memory db")
}

async fn voice_channel(db: &db::Db, name: &str, parent: Option<i32>) -> i32 {
    db::add_voice_channel(db, name, "standard", None, None, parent)
        .await
        .expect("add voice channel")
        .expect("name free")
        .id
}

async fn deny_everyone(db: &db::Db, channel_id: i32) {
    db::upsert_channel_override(
        db,
        ChannelKind::Voice,
        channel_id,
        "everyone",
        "",
        "",
        0,
        murmer_server::permissions::VIEW_CHANNELS,
    )
    .await
    .expect("store override");
}

async fn override_count(db: &db::Db, channel_id: i32) -> usize {
    db::get_channel_overrides(db, ChannelKind::Voice, channel_id)
        .await
        .expect("read overrides")
        .len()
}

#[tokio::test]
async fn a_room_records_the_channel_it_was_split_off_from() {
    let db = setup().await;
    let lounge = voice_channel(&db, "Lounge", None).await;
    let room = voice_channel(&db, "Lounge Room 1", Some(lounge)).await;

    let record = db::get_voice_channel_by_id(&db, room).await.expect("room");
    assert_eq!(record.breakout_parent, Some(lounge));

    // The list every client is built from carries the parent too, or the
    // sidebar cannot tell a room from an ordinary channel.
    let listed = db::get_voice_channels(&db).await;
    assert_eq!(
        listed
            .iter()
            .find(|c| c.id == lounge)
            .expect("lounge listed")
            .breakout_parent,
        None
    );
    assert_eq!(
        listed
            .iter()
            .find(|c| c.id == room)
            .expect("room listed")
            .breakout_parent,
        Some(lounge)
    );
}

#[tokio::test]
async fn a_restart_sweeps_breakout_rooms_and_their_overrides() {
    let db = setup().await;
    let lounge = voice_channel(&db, "Lounge", None).await;
    let room = voice_channel(&db, "Lounge Room 1", Some(lounge)).await;
    deny_everyone(&db, lounge).await;
    deny_everyone(&db, room).await;

    // `run_schema` is what a restart runs; nothing else deletes a room that
    // outlived the process that opened it.
    db::run_schema(&db).await.expect("restart");

    let listed = db::get_voice_channels(&db).await;
    assert!(listed.iter().any(|c| c.id == lounge));
    assert!(
        !listed.iter().any(|c| c.id == room),
        "a breakout room survived a restart"
    );
    // The parent is an ordinary channel and keeps everything it had; the
    // room's override must not be left behind for whoever inherits its id.
    assert_eq!(override_count(&db, lounge).await, 1);
    assert_eq!(override_count(&db, room).await, 0);

    // A second restart finds nothing to sweep and leaves the rest alone.
    db::run_schema(&db).await.expect("second restart");
    assert!(
        db::get_voice_channels(&db)
            .await
            .iter()
            .any(|c| c.id == lounge)
    );
    assert_eq!(override_count(&db, lounge).await, 1);
}

#[tokio::test]
async fn room_names_stay_within_what_a_channel_name_may_be() {
    assert_eq!(breakout_room_name("Lounge", 1), "Lounge Room 1");

    // The longest name a voice channel can have. The suffix has to fit
    // somewhere, and the only place is the parent's name.
    let long = "L".repeat(50);
    for index in 1..=8 {
        let name = breakout_room_name(&long, index);
        assert!(
            validate_channel_name(&name),
            "generated name is not a valid channel name: {name:?}"
        );
    }
    // Truncating must not leave a trailing space, which `validate_name`
    // rejects on its own.
    let padded = format!("{} x", "L".repeat(41));
    let name = breakout_room_name(&padded, 1);
    assert!(validate_channel_name(&name), "{name:?}");
    assert!(!name.contains("  "), "{name:?}");

    // Rooms of the same parent must not collide: voice channel names are
    // unique, so two rooms with one name is one room.
    let names: Vec<String> = (1..=8).map(|i| breakout_room_name(&long, i)).collect();
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "duplicate room names: {names:?}");
}

#[tokio::test]
async fn a_split_needs_at_least_two_rooms_and_stays_bounded() {
    // One room is not a split, and neither is none of them.
    assert_eq!(breakout_room_count(0), None);
    assert_eq!(breakout_room_count(1), None);
    assert_eq!(breakout_room_count(-4), None);

    assert_eq!(breakout_room_count(2), Some(2));

    // The cap is what keeps a single frame from filling the sidebar with
    // channels; a client asking for a thousand gets nothing.
    assert_eq!(breakout_room_count(1_000), None);
    assert_eq!(breakout_room_count(i64::MAX), None);
}
