//! Profile persistence: the display name, nickname and about text stored on a
//! user's name/key binding, and the validation the `set-profile` and
//! `set-nickname` handlers apply.

use murmer_server::db;
use murmer_server::ws::validation::{validate_about, validate_display_name, validate_nickname};

#[tokio::test]
async fn profile_fields_update_independently() {
    let db = db::init(":memory:").await.expect("in-memory db");
    db::bind_user_key(&db, "alice", "key-a")
        .await
        .expect("bind");

    let profile = db::get_user_profile(&db, "alice")
        .await
        .expect("query")
        .expect("binding exists");
    assert_eq!(profile.display_name, "");
    assert_eq!(profile.about, "");
    assert!(!profile.created_at.is_empty(), "member since is recorded");

    // Writing one field must not clobber the other.
    assert!(
        db::set_user_profile(&db, "alice", Some("Alice A."), None)
            .await
            .expect("set display name")
    );
    assert!(
        db::set_user_profile(&db, "alice", None, Some("Builds things."))
            .await
            .expect("set about")
    );
    let profile = db::get_user_profile(&db, "alice")
        .await
        .expect("query")
        .expect("binding exists");
    assert_eq!(profile.display_name, "Alice A.");
    assert_eq!(profile.about, "Builds things.");

    // An empty string clears a field, again without touching the other.
    assert!(
        db::set_user_profile(&db, "alice", Some(""), None)
            .await
            .expect("clear display name")
    );
    let profile = db::get_user_profile(&db, "alice")
        .await
        .expect("query")
        .expect("binding exists");
    assert_eq!(profile.display_name, "");
    assert_eq!(profile.about, "Builds things.");

    // Users without a binding (bots) carry no profile.
    assert!(
        !db::set_user_profile(&db, "nobody", Some("Ghost"), None)
            .await
            .expect("set profile for unknown user")
    );
    assert!(
        db::get_user_profile(&db, "nobody")
            .await
            .expect("query")
            .is_none()
    );
}

#[tokio::test]
async fn snapshot_lists_every_binding() {
    let db = db::init(":memory:").await.expect("in-memory db");
    db::bind_user_key(&db, "alice", "key-a")
        .await
        .expect("bind");
    db::bind_user_key(&db, "bob", "key-b").await.expect("bind");
    db::set_user_profile(&db, "bob", Some("Bobby"), None)
        .await
        .expect("set display name");

    let profiles = db::get_all_profiles(&db).await.expect("snapshot");
    assert_eq!(profiles.len(), 2);
    let bob = profiles
        .iter()
        .find(|p| p.user_name == "bob")
        .expect("bob listed");
    assert_eq!(bob.display_name, "Bobby");
    // Users who never edited anything are included too — the snapshot also
    // carries "member since".
    let alice = profiles
        .iter()
        .find(|p| p.user_name == "alice")
        .expect("alice listed");
    assert_eq!(alice.display_name, "");
    assert!(!alice.created_at.is_empty());
}

#[test]
fn display_name_and_about_limits() {
    assert!(validate_display_name(""), "empty clears the display name");
    assert!(validate_display_name("Alice A."));
    assert!(validate_display_name("Ünïcödé ✨"));
    assert!(!validate_display_name(" padded "), "must arrive trimmed");
    assert!(!validate_display_name("line\nbreak"));
    assert!(!validate_display_name(&"x".repeat(33)));
    // Counted in characters, not bytes: 32 emoji are still 32 characters.
    assert!(validate_display_name(&"✨".repeat(32)));

    assert!(validate_about(""));
    assert!(validate_about("Two\nlines are fine."));
    assert!(!validate_about("tab\tseparated"));
    assert!(!validate_about(&"x".repeat(301)));
}

#[tokio::test]
async fn nickname_is_written_separately_from_the_profile() {
    let db = db::init(":memory:").await.expect("in-memory db");
    db::bind_user_key(&db, "alice", "key-a")
        .await
        .expect("bind");
    db::set_user_profile(&db, "alice", Some("Alice A."), Some("Builds things."))
        .await
        .expect("set profile");

    // A moderator setting the nickname must not be able to touch anything the
    // user owns, which is why it is its own statement.
    assert!(
        db::set_user_nickname(&db, "alice", "Sparky")
            .await
            .expect("set nickname")
    );
    let profile = db::get_user_profile(&db, "alice")
        .await
        .expect("query")
        .expect("binding exists");
    assert_eq!(profile.nickname, "Sparky");
    assert_eq!(profile.display_name, "Alice A.");
    assert_eq!(profile.about, "Builds things.");

    // Editing the profile leaves the nickname alone in the same way.
    db::set_user_profile(&db, "alice", Some("Alice B."), None)
        .await
        .expect("set display name");
    let profile = db::get_user_profile(&db, "alice")
        .await
        .expect("query")
        .expect("binding exists");
    assert_eq!(profile.nickname, "Sparky");
    assert_eq!(profile.display_name, "Alice B.");

    // An empty string clears it, falling back to the display name.
    assert!(
        db::set_user_nickname(&db, "alice", "")
            .await
            .expect("clear nickname")
    );
    let profile = db::get_user_profile(&db, "alice")
        .await
        .expect("query")
        .expect("binding exists");
    assert_eq!(profile.nickname, "");

    // A name that never connected has no binding row to label.
    assert!(
        !db::set_user_nickname(&db, "nobody", "Ghost")
            .await
            .expect("set nickname for unknown user")
    );
}

#[tokio::test]
async fn snapshot_carries_the_nickname() {
    let db = db::init(":memory:").await.expect("in-memory db");
    db::bind_user_key(&db, "bob", "key-b").await.expect("bind");
    db::set_user_nickname(&db, "bob", "Bobby")
        .await
        .expect("set nickname");

    let profiles = db::get_all_profiles(&db).await.expect("snapshot");
    let bob = profiles
        .iter()
        .find(|p| p.user_name == "bob")
        .expect("bob listed");
    assert_eq!(bob.nickname, "Bobby");
}

#[test]
fn nickname_limits_match_the_display_name() {
    // Same slot in the UI, so the same rules; kept as its own validator
    // because the two limits are free to diverge later.
    assert!(validate_nickname(""), "empty clears the nickname");
    assert!(validate_nickname("Sparky"));
    assert!(!validate_nickname(" padded "), "must arrive trimmed");
    assert!(!validate_nickname("line\nbreak"));
    assert!(!validate_nickname(&"x".repeat(33)));
    assert!(validate_nickname(&"y".repeat(32)));
}

#[test]
fn moderating_roles_gain_the_nickname_flag_by_default() {
    use murmer_server::permissions::{
        DEFAULT_ADMIN, DEFAULT_EVERYONE, DEFAULT_MOD, MANAGE_NICKNAMES, mask_allows,
    };
    assert!(mask_allows(DEFAULT_MOD, MANAGE_NICKNAMES));
    assert!(mask_allows(DEFAULT_ADMIN, MANAGE_NICKNAMES));
    // Relabelling other people is never part of the baseline.
    assert!(!mask_allows(DEFAULT_EVERYONE, MANAGE_NICKNAMES));
}
