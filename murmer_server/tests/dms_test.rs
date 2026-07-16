use base64::{Engine as _, engine::general_purpose};
use murmer_server::db::{self, DbCall};
use murmer_server::ws::helpers::{DmPayloadError, dm_involves, validate_dm_payload};
use serde_json::json;

/// Base64 of `len` arbitrary bytes, mimicking client-encoded crypto fields.
fn b64(len: usize) -> String {
    general_purpose::STANDARD.encode(vec![0u8; len])
}

#[test]
fn accepts_well_formed_payload() {
    assert_eq!(validate_dm_payload(&b64(24), &b64(17)), Ok(()));
    // Longest allowed message: 4000 plaintext bytes + 16 authenticator bytes.
    assert_eq!(validate_dm_payload(&b64(24), &b64(4016)), Ok(()));
}

#[test]
fn rejects_invalid_base64() {
    assert_eq!(
        validate_dm_payload("not base64!", &b64(32)),
        Err(DmPayloadError::Malformed)
    );
    assert_eq!(
        validate_dm_payload(&b64(24), "not base64!"),
        Err(DmPayloadError::Malformed)
    );
}

#[test]
fn rejects_wrong_nonce_length() {
    assert_eq!(
        validate_dm_payload(&b64(23), &b64(32)),
        Err(DmPayloadError::Malformed)
    );
    assert_eq!(
        validate_dm_payload(&b64(25), &b64(32)),
        Err(DmPayloadError::Malformed)
    );
}

#[test]
fn rejects_ciphertext_without_content() {
    // 16 bytes is the bare Poly1305 authenticator (empty plaintext).
    assert_eq!(
        validate_dm_payload(&b64(24), &b64(16)),
        Err(DmPayloadError::Malformed)
    );
    assert_eq!(
        validate_dm_payload(&b64(24), &b64(0)),
        Err(DmPayloadError::Malformed)
    );
}

#[test]
fn rejects_oversized_ciphertext() {
    assert_eq!(
        validate_dm_payload(&b64(24), &b64(4017)),
        Err(DmPayloadError::TooLong)
    );
}

/// The E2EE rollout wipes pre-existing plaintext DM rows exactly once; DMs
/// stored after the marker is set survive later schema runs.
#[tokio::test]
async fn plaintext_dm_wipe_runs_only_once() {
    let conn = db::init(":memory:").await.expect("in-memory db");

    // Simulate a pre-E2EE database: drop the marker and store a plaintext DM.
    conn.call_db(|conn| {
        conn.execute("DELETE FROM server_settings WHERE key = 'dm_e2ee'", [])?;
        conn.execute(
            "INSERT INTO direct_messages (sender, recipient, content) VALUES ('a', 'b', '{}')",
            [],
        )?;
        Ok(())
    })
    .await
    .expect("seed plaintext dm");

    db::run_schema(&conn).await.expect("re-run schema");
    let history = db::fetch_dm_history(&conn, "a", "b", None, 10)
        .await
        .expect("history");
    assert!(history.is_empty(), "plaintext rows should be wiped");

    // Encrypted-era rows survive further schema runs.
    db::insert_direct_message(&conn, "a", "b", "{}")
        .await
        .expect("insert");
    db::run_schema(&conn).await.expect("re-run schema again");
    let history = db::fetch_dm_history(&conn, "a", "b", None, 10)
        .await
        .expect("history");
    assert_eq!(history.len(), 1, "post-wipe rows must not be deleted");
}

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
