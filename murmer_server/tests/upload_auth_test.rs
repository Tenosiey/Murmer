//! HTTP-level tests for the authentication gate on `/upload`.
//!
//! Writing a file to disk is the only thing an HTTP caller can make the server
//! spend storage on, so the endpoint accepts nothing but a fresh, single-use
//! Ed25519 proof from a key that already owns an account here — and only so
//! many of those per minute per IP.

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit},
    http::{Request, StatusCode, header},
    routing::post,
};
use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signer, SigningKey};
use murmer_server::{AppState, RateLimiter, db, upload};
use tokio::sync::{Mutex, broadcast};
use tower::ServiceExt;

const BOUNDARY: &str = "murmertestboundary";

/// A PNG header plus a byte of payload: enough to pass the magic-byte check.
const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];

/// An upload directory of this test's own, so a stored file is observable and
/// nothing leaks into the crate directory.
fn temp_upload_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("murmer-upload-{label}-{unique}"));
    std::fs::create_dir_all(&dir).expect("create upload dir");
    dir
}

async fn make_app(upload_dir: PathBuf, rate_limiter: RateLimiter) -> (Router, Arc<AppState>) {
    let database = db::init(":memory:").await.expect("in-memory db");
    let (tx, _) = broadcast::channel(64);
    let state = Arc::new(AppState {
        tx,
        channels: Arc::new(Mutex::new(HashMap::new())),
        direct: Arc::new(Mutex::new(HashMap::new())),
        db: database,
        users: Arc::new(Mutex::new(Default::default())),
        known_users: Arc::new(Mutex::new(Default::default())),
        voice_channels: Arc::new(Mutex::new(HashMap::new())),
        role_defs: Arc::new(Mutex::new(HashMap::new())),
        user_roles: Arc::new(Mutex::new(HashMap::new())),
        channel_overrides: Arc::new(Mutex::new(HashMap::new())),
        statuses: Arc::new(Mutex::new(HashMap::new())),
        user_keys: Arc::new(Mutex::new(HashMap::new())),
        mutes: Arc::new(Mutex::new(HashMap::new())),
        active_screen_shares: Arc::new(Mutex::new(HashMap::new())),
        active_webcams: Arc::new(Mutex::new(HashMap::new())),
        voice_mutes: Arc::new(Mutex::new(HashMap::new())),
        connection_stats: Arc::new(Mutex::new(HashMap::new())),
        voice_session_starts: Arc::new(Mutex::new(HashMap::new())),
        screenshare_session_starts: Arc::new(Mutex::new(HashMap::new())),
        soundboard_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        upload_dir,
        password: None,
        admin_token: None,
        rate_limiter,
        stats_enabled: std::sync::atomic::AtomicBool::new(false),
        chat_settings: Arc::new(Mutex::new(murmer_server::db::ChatSettings::default())),
        slow_mode_sends: Arc::new(Mutex::new(HashMap::new())),
        visibility_epoch: std::sync::atomic::AtomicU64::new(0),
    });
    let router = Router::new()
        .route(
            "/upload",
            post(upload::upload).layer(DefaultBodyLimit::max(upload::MAX_CONFIGURABLE_FILE_SIZE)),
        )
        .with_state(Arc::clone(&state));
    (router, state)
}

/// One `name=value` text part.
fn text_part(name: &str, value: &str) -> Vec<u8> {
    format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
        .into_bytes()
}

/// A multipart body carrying the given credential parts ahead of the file.
fn body(credentials: &[(&str, &str)], filename: &str, contents: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, value) in credentials {
        out.extend_from_slice(&text_part(name, value));
    }
    out.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(contents);
    out.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    out
}

async fn post_upload(app: &Router, body: Vec<u8>) -> StatusCode {
    let mut request = Request::builder()
        .method("POST")
        .uri("/upload")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .expect("request");
    // `oneshot` skips the connection layer that normally supplies this, and
    // the handler rate-limits per client IP.
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4000))));
    app.clone()
        .oneshot(request)
        .await
        .expect("response")
        .status()
}

/// The credentials a client sends: the key, a timestamp, and the signature
/// over `upload:<timestamp>`.
fn credentials(key: &SigningKey, timestamp: &str) -> (String, String) {
    let public = general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    let signature = general_purpose::STANDARD.encode(
        key.sign(format!("upload:{timestamp}").as_bytes())
            .to_bytes(),
    );
    (public, signature)
}

fn now_ms() -> String {
    chrono::Utc::now().timestamp_millis().to_string()
}

fn stored_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read upload dir")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into())
        .collect()
}

#[tokio::test]
async fn a_signed_upload_from_a_bound_key_is_stored() {
    let dir = temp_upload_dir("accepted");
    let (app, state) = make_app(dir.clone(), RateLimiter::new()).await;

    let key = SigningKey::from_bytes(&[7u8; 32]);
    let timestamp = now_ms();
    let (public, signature) = credentials(&key, &timestamp);
    // Presence binds the name to the key; that binding is the account the
    // upload is made under.
    db::bind_user_key(&state.db, "alice", &public)
        .await
        .expect("bind");

    let status = post_upload(
        &app,
        body(
            &[
                ("publicKey", &public),
                ("timestamp", &timestamp),
                ("signature", &signature),
            ],
            "cat.png",
            PNG,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored_files(&dir).len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn an_upload_without_credentials_writes_nothing() {
    let dir = temp_upload_dir("anonymous");
    let (app, _state) = make_app(dir.clone(), RateLimiter::new()).await;

    let status = post_upload(&app, body(&[], "cat.png", PNG)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(stored_files(&dir).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_signature_over_the_wrong_message_is_rejected() {
    let dir = temp_upload_dir("wrong-message");
    let (app, state) = make_app(dir.clone(), RateLimiter::new()).await;

    let key = SigningKey::from_bytes(&[9u8; 32]);
    let timestamp = now_ms();
    let public = general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    db::bind_user_key(&state.db, "mallory", &public)
        .await
        .expect("bind");

    // A presence proof signs the bare timestamp. Uploads sign
    // `upload:<timestamp>`, so a captured presence signature must not work
    // here.
    let presence_signature =
        general_purpose::STANDARD.encode(key.sign(timestamp.as_bytes()).to_bytes());

    let status = post_upload(
        &app,
        body(
            &[
                ("publicKey", &public),
                ("timestamp", &timestamp),
                ("signature", &presence_signature),
            ],
            "cat.png",
            PNG,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(stored_files(&dir).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_key_without_an_account_here_is_rejected() {
    let dir = temp_upload_dir("unknown-key");
    let (app, _state) = make_app(dir.clone(), RateLimiter::new()).await;

    // Valid proof, but this key has never authenticated with this server — on
    // a password-protected server it could not have.
    let key = SigningKey::from_bytes(&[11u8; 32]);
    let timestamp = now_ms();
    let (public, signature) = credentials(&key, &timestamp);

    let status = post_upload(
        &app,
        body(
            &[
                ("publicKey", &public),
                ("timestamp", &timestamp),
                ("signature", &signature),
            ],
            "cat.png",
            PNG,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(stored_files(&dir).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_replayed_proof_is_rejected() {
    let dir = temp_upload_dir("replay");
    let (app, state) = make_app(dir.clone(), RateLimiter::new()).await;

    let key = SigningKey::from_bytes(&[13u8; 32]);
    let timestamp = now_ms();
    let (public, signature) = credentials(&key, &timestamp);
    db::bind_user_key(&state.db, "bob", &public)
        .await
        .expect("bind");
    let parts = [
        ("publicKey", public.as_str()),
        ("timestamp", timestamp.as_str()),
        ("signature", signature.as_str()),
    ];

    assert_eq!(
        post_upload(&app, body(&parts, "cat.png", PNG)).await,
        StatusCode::OK
    );
    // Same timestamp, same signature: the nonce store has already spent it.
    assert_eq!(
        post_upload(&app, body(&parts, "cat.png", PNG)).await,
        StatusCode::UNAUTHORIZED
    );

    assert_eq!(stored_files(&dir).len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn uploads_are_rate_limited_per_ip() {
    let dir = temp_upload_dir("rate-limit");
    let mut limiter = RateLimiter::new();
    limiter.max_uploads_per_minute = 1;
    let (app, state) = make_app(dir.clone(), limiter).await;

    let key = SigningKey::from_bytes(&[17u8; 32]);
    let public = general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    db::bind_user_key(&state.db, "carol", &public)
        .await
        .expect("bind");

    for expected in [StatusCode::OK, StatusCode::TOO_MANY_REQUESTS] {
        // A fresh proof each time, so only the limit can reject the second one.
        let timestamp = now_ms();
        let (_, signature) = credentials(&key, &timestamp);
        let status = post_upload(
            &app,
            body(
                &[
                    ("publicKey", &public),
                    ("timestamp", &timestamp),
                    ("signature", &signature),
                ],
                "cat.png",
                PNG,
            ),
        )
        .await;
        assert_eq!(status, expected);
    }

    assert_eq!(stored_files(&dir).len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}
