//! Profile persistence: the display name and about text stored on a user's
//! name/key binding, and the validation the `set-profile` handler applies.

use murmer_server::db;
use murmer_server::ws::validation::{validate_about, validate_display_name};

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
