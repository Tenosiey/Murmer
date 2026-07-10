use murmer_server::ws::helpers::dm_involves;
use serde_json::json;

#[test]
fn delivers_to_sender_and_recipient() {
    let frame = json!({"type": "dm", "from": "alice", "to": "bob", "text": "hi"});
    assert!(dm_involves(&frame, Some("alice")));
    assert!(dm_involves(&frame, Some("bob")));
}

#[test]
fn hides_from_other_users() {
    let frame = json!({"type": "dm", "from": "alice", "to": "bob", "text": "hi"});
    assert!(!dm_involves(&frame, Some("mallory")));
}

#[test]
fn hides_from_unauthenticated_connections() {
    let frame = json!({"type": "dm", "from": "alice", "to": "bob", "text": "hi"});
    assert!(!dm_involves(&frame, None));
}

#[test]
fn requires_exact_name_match() {
    let frame = json!({"type": "dm", "from": "alice", "to": "bob", "text": "hi"});
    assert!(!dm_involves(&frame, Some("ali")));
    assert!(!dm_involves(&frame, Some("Bob")));
}

#[test]
fn ignores_non_string_participants() {
    let frame = json!({"type": "dm", "from": 42, "to": null, "text": "hi"});
    assert!(!dm_involves(&frame, Some("42")));
}
