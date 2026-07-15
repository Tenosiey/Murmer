use murmer_server::db;
use murmer_server::ws::validation::{is_emoji_shortcode, validate_emoji_name};

#[test]
fn accepts_valid_emoji_names() {
    assert!(validate_emoji_name("party_parrot"));
    assert!(validate_emoji_name("ok"));
    assert!(validate_emoji_name("a1"));
    assert!(validate_emoji_name("emoji_with_32_characters_in_it_x"));
}

#[test]
fn rejects_invalid_emoji_names() {
    assert!(!validate_emoji_name(""));
    assert!(!validate_emoji_name("a"));
    assert!(!validate_emoji_name("UPPER"));
    assert!(!validate_emoji_name("has space"));
    assert!(!validate_emoji_name("dash-ed"));
    assert!(!validate_emoji_name(":colons:"));
    assert!(!validate_emoji_name("ünïcode"));
    assert!(!validate_emoji_name(&"x".repeat(33)));
}

#[test]
fn detects_emoji_shortcodes() {
    assert!(is_emoji_shortcode(":party_parrot:"));
    assert!(is_emoji_shortcode(":ok:"));
    assert!(!is_emoji_shortcode("party_parrot"));
    assert!(!is_emoji_shortcode(":party parrot:"));
    assert!(!is_emoji_shortcode("::"));
    assert!(!is_emoji_shortcode(":a:"));
    assert!(!is_emoji_shortcode("👍"));
}

#[tokio::test]
async fn emoji_round_trip() {
    let db = db::init(":memory:").await.expect("in-memory db");

    assert_eq!(db::count_emojis(&db).await.expect("count"), 0);
    assert!(!db::emoji_exists(&db, "party_parrot").await.expect("exists"));

    assert!(
        db::add_emoji(&db, "party_parrot", "/files/1-parrot.gif", "alice")
            .await
            .expect("add")
    );
    assert!(db::emoji_exists(&db, "party_parrot").await.expect("exists"));
    assert_eq!(db::count_emojis(&db).await.expect("count"), 1);

    // Duplicate names are rejected without clobbering the original.
    assert!(
        !db::add_emoji(&db, "party_parrot", "/files/2-other.png", "bob")
            .await
            .expect("duplicate add")
    );
    let emojis = db::get_emojis(&db).await.expect("list");
    assert_eq!(emojis.len(), 1);
    assert_eq!(emojis[0].name, "party_parrot");
    assert_eq!(emojis[0].url, "/files/1-parrot.gif");
    assert_eq!(emojis[0].uploaded_by, "alice");

    // Removal returns the stored URL for file cleanup, once.
    assert_eq!(
        db::remove_emoji(&db, "party_parrot").await.expect("remove"),
        Some("/files/1-parrot.gif".to_string())
    );
    assert_eq!(
        db::remove_emoji(&db, "party_parrot")
            .await
            .expect("second remove"),
        None
    );
    assert_eq!(db::count_emojis(&db).await.expect("count"), 0);
}

#[tokio::test]
async fn emojis_are_listed_in_name_order() {
    let db = db::init(":memory:").await.expect("in-memory db");
    for name in ["zebra", "apple", "mango"] {
        assert!(db::add_emoji(&db, name, "/files/1-a.png", "alice")
            .await
            .expect("add"));
    }
    let names: Vec<String> = db::get_emojis(&db)
        .await
        .expect("list")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["apple", "mango", "zebra"]);
}
