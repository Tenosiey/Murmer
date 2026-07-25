//! Integration tests for the soundboard: name/URL validation, the audio
//! magic-byte gate, the sound library round trip, the per-user stat behind the
//! double opt-in, and the one-shot permission migration for pre-soundboard
//! databases.

use murmer_server::db;
use murmer_server::db::DbCall;
use murmer_server::permissions::{
    self, DEFAULT_EVERYONE, MANAGE_EMOJIS, MANAGE_SOUNDS, USE_SOUNDBOARD,
};
use murmer_server::upload::detect_audio_type;
use murmer_server::ws::validation::{sound_key_from_url, validate_sound_name};

#[test]
fn accepts_valid_sound_names() {
    assert!(validate_sound_name("Air Horn"));
    assert!(validate_sound_name("wow"));
    assert!(validate_sound_name("Ünïcode is fine"));
    assert!(validate_sound_name(&"x".repeat(32)));
}

#[test]
fn rejects_invalid_sound_names() {
    assert!(!validate_sound_name(""));
    assert!(!validate_sound_name("x"));
    // Untrimmed names are rejected so " a" and "a " cannot both exist.
    assert!(!validate_sound_name(" padded "));
    assert!(!validate_sound_name("new\nline"));
    assert!(!validate_sound_name("bell\u{7}"));
    assert!(!validate_sound_name(&"x".repeat(33)));
}

#[test]
fn sound_urls_stay_inside_the_upload_directory() {
    assert_eq!(sound_key_from_url("/files/horn.mp3"), Some("horn.mp3"));
    assert_eq!(
        sound_key_from_url("/files/1700-a.b.ogg"),
        Some("1700-a.b.ogg")
    );
    assert_eq!(sound_key_from_url("/files/../../etc/passwd"), None);
    assert_eq!(sound_key_from_url("/files/nested/horn.mp3"), None);
    assert_eq!(sound_key_from_url("/files/horn.mp3\u{0}"), None);
    assert_eq!(sound_key_from_url("https://evil.example/horn.mp3"), None);
    // Only the sound safe-list; an image or a script must never resolve here.
    assert_eq!(sound_key_from_url("/files/icon.png"), None);
    assert_eq!(sound_key_from_url("/files/payload.svg"), None);
    assert_eq!(sound_key_from_url("/files/payload.html"), None);
}

#[test]
fn audio_magic_bytes_are_required() {
    assert_eq!(
        detect_audio_type(b"ID3\x04\x00\x00\x00"),
        Some("audio/mpeg")
    );
    assert_eq!(
        detect_audio_type(&[0xFF, 0xFB, 0x90, 0x00]),
        Some("audio/mpeg")
    );
    assert_eq!(
        detect_audio_type(b"RIFF\x00\x00\x00\x00WAVE"),
        Some("audio/wav")
    );
    assert_eq!(
        detect_audio_type(b"OggS\x00\x02\x00\x00"),
        Some("audio/ogg")
    );
    assert_eq!(
        detect_audio_type(b"\x00\x00\x00\x20ftypM4A "),
        Some("audio/mp4")
    );

    // A renamed non-audio file must not pass, however friendly its extension.
    assert_eq!(detect_audio_type(b"<html><script>"), None);
    assert_eq!(detect_audio_type(b"\x89PNG\r\n\x1a\n"), None);
    assert_eq!(detect_audio_type(b""), None);
    assert_eq!(detect_audio_type(b"MZ"), None);
}

#[tokio::test]
async fn sound_round_trip() {
    let db = db::init(":memory:").await.expect("in-memory db");

    assert_eq!(db::count_sounds(&db).await.expect("count"), 0);

    let id = db::add_sound(&db, "Air Horn", "/files/1-horn.mp3", "alice")
        .await
        .expect("add")
        .expect("new id");
    assert_eq!(db::count_sounds(&db).await.expect("count"), 1);

    // Names are unique case-insensitively and a clash leaves the original be.
    assert!(
        db::add_sound(&db, "AIR HORN", "/files/2-other.mp3", "bob")
            .await
            .expect("duplicate add")
            .is_none()
    );
    assert_eq!(db::count_sounds(&db).await.expect("count"), 1);

    let sound = db::get_sound(&db, id).await.expect("get").expect("present");
    assert_eq!(sound.name, "Air Horn");
    assert_eq!(sound.url, "/files/1-horn.mp3");
    assert_eq!(sound.uploaded_by, "alice");
    assert_eq!(sound.play_count, 0);

    // Playing bumps the unattributed aggregate counter.
    db::note_sound_played(&db, id).await.expect("note play");
    db::note_sound_played(&db, id).await.expect("note play");
    let sound = db::get_sound(&db, id).await.expect("get").expect("present");
    assert_eq!(sound.play_count, 2);

    // Removal returns the URL for file cleanup, exactly once.
    assert_eq!(
        db::remove_sound(&db, id).await.expect("remove"),
        Some("/files/1-horn.mp3".to_string())
    );
    assert_eq!(
        db::remove_sound(&db, id).await.expect("second remove"),
        None
    );
    assert_eq!(db::count_sounds(&db).await.expect("count"), 0);
}

#[tokio::test]
async fn renaming_keeps_the_id_and_rejects_clashes() {
    let db = db::init(":memory:").await.expect("in-memory db");
    let horn = db::add_sound(&db, "Air Horn", "/files/1-horn.mp3", "alice")
        .await
        .expect("add")
        .expect("id");
    let wow = db::add_sound(&db, "Wow", "/files/2-wow.mp3", "alice")
        .await
        .expect("add")
        .expect("id");

    // The id is what clients key their per-sound volume on, so it must survive.
    assert!(matches!(
        db::rename_sound(&db, horn, "Klaxon").await.expect("rename"),
        db::RenameOutcome::Renamed
    ));
    let sound = db::get_sound(&db, horn)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(sound.id, horn);
    assert_eq!(sound.name, "Klaxon");

    assert!(matches!(
        db::rename_sound(&db, horn, "wow").await.expect("rename"),
        db::RenameOutcome::NameTaken
    ));
    assert!(matches!(
        db::rename_sound(&db, 9999, "Anything")
            .await
            .expect("rename"),
        db::RenameOutcome::NotFound
    ));

    // The clash left both rows untouched.
    assert_eq!(
        db::get_sound(&db, wow)
            .await
            .expect("get")
            .expect("present")
            .name,
        "Wow"
    );
}

#[tokio::test]
async fn sound_stats_need_the_double_opt_in() {
    let db = db::init(":memory:").await.expect("in-memory db");

    // Neither switch: nothing is recorded.
    db::record_sound_played(&db, "alice", "Air Horn")
        .await
        .expect("record");
    assert!(
        db::get_user_stats(&db, "alice")
            .await
            .expect("stats")
            .is_none()
    );

    // Server toggle alone is not enough.
    db::set_stats_server_enabled(&db, true)
        .await
        .expect("toggle");
    db::record_sound_played(&db, "alice", "Air Horn")
        .await
        .expect("record");
    assert!(
        db::get_user_stats(&db, "alice")
            .await
            .expect("stats")
            .is_none()
    );

    // Both switches on: the counter and the per-sound tally are written.
    db::set_stats_opt_in(&db, "alice", true)
        .await
        .expect("opt in");
    db::record_sound_played(&db, "alice", "Air Horn")
        .await
        .expect("record");
    db::record_sound_played(&db, "alice", "Air Horn")
        .await
        .expect("record");
    db::record_sound_played(&db, "alice", "Wow")
        .await
        .expect("record");

    let stats = db::get_user_stats(&db, "alice")
        .await
        .expect("stats")
        .expect("recorded");
    assert_eq!(stats.sounds_played, 3);

    let favorites = db::get_favorite_sounds(&db, "alice", 5)
        .await
        .expect("favorites");
    assert_eq!(favorites, vec![("Air Horn".into(), 2), ("Wow".into(), 1)]);

    // Purging clears the tally along with every other counter.
    db::purge_user_stats(&db, "alice").await.expect("purge");
    assert!(
        db::get_favorite_sounds(&db, "alice", 5)
            .await
            .expect("favorites")
            .is_empty()
    );
}

#[test]
fn soundboard_flags_are_seeded_into_the_builtin_roles() {
    // Playing is a baseline capability; managing sits with the other asset
    // management permissions.
    assert!(permissions::mask_allows(DEFAULT_EVERYONE, USE_SOUNDBOARD));
    assert!(!permissions::mask_allows(DEFAULT_EVERYONE, MANAGE_SOUNDS));
    assert!(permissions::mask_allows(
        permissions::DEFAULT_MOD,
        MANAGE_SOUNDS
    ));
    assert!(permissions::is_valid_mask(USE_SOUNDBOARD | MANAGE_SOUNDS));
}

#[tokio::test]
async fn pre_soundboard_databases_are_granted_the_new_flags() {
    let db = db::init(":memory:").await.expect("in-memory db");

    // Simulate a database created before the soundboard: clear both bits
    // everywhere and drop the migration marker so it runs again.
    let cleared = !(USE_SOUNDBOARD | MANAGE_SOUNDS) as i64;
    db.call_db(move |conn| {
        conn.execute(
            "UPDATE role_definitions SET permissions = permissions & ?1",
            [cleared],
        )?;
        conn.execute(
            "DELETE FROM server_settings WHERE key = 'soundboard_perms'",
            [],
        )?;
        Ok(())
    })
    .await
    .expect("age the database");

    let everyone_before = role_permissions(&db, "@everyone").await;
    assert!(everyone_before & USE_SOUNDBOARD == 0);

    db::run_schema(&db).await.expect("re-run schema");

    // @everyone can play; roles that already curate emojis can manage sounds.
    assert!(role_permissions(&db, "@everyone").await & USE_SOUNDBOARD != 0);
    let mod_perms = role_permissions(&db, "Mod").await;
    assert!(mod_perms & MANAGE_EMOJIS != 0);
    assert!(mod_perms & MANAGE_SOUNDS != 0);
    // A role without emoji management does not silently gain sound management.
    assert!(role_permissions(&db, "@everyone").await & MANAGE_SOUNDS == 0);

    // The marker makes it one-shot: a deliberate revocation must stick.
    let everyone_id = role_id(&db, "@everyone").await;
    db::update_role_def(
        &db,
        everyone_id,
        "@everyone",
        None,
        DEFAULT_EVERYONE & !USE_SOUNDBOARD,
    )
    .await
    .expect("revoke");
    db::run_schema(&db).await.expect("re-run schema again");
    assert!(role_permissions(&db, "@everyone").await & USE_SOUNDBOARD == 0);
}

async fn role_id(db: &db::Db, name: &str) -> i64 {
    db::get_role_def_by_name(db, name)
        .await
        .expect("lookup")
        .expect("role exists")
        .id
}

async fn role_permissions(db: &db::Db, name: &str) -> u64 {
    db::get_role_def_by_name(db, name)
        .await
        .expect("lookup")
        .expect("role exists")
        .permissions
}
