//! HTTP-level tests for the authentication gate on `/upload`.
//!
//! Writing a file to disk is the only thing an HTTP caller can make the server
//! spend storage on, so the endpoint accepts nothing but the upload session of
//! a live, authenticated WebSocket connection — and only so many uploads per
//! minute per IP.

use std::{
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
use murmer_server::{AppState, RateLimiter, db, upload};
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
    let state = Arc::new(AppState {
        upload_dir,
        rate_limiter,
        ..AppState::new(database)
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

/// Register `session` the way a successful `presence` does.
async fn open_session(state: &AppState, session: &str, user: &str) {
    state.upload_sessions.lock().await.insert(
        session.to_string(),
        (user.to_string(), format!("{user}-key")),
    );
}

fn stored_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read upload dir")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into())
        .collect()
}

#[tokio::test]
async fn an_upload_with_a_live_session_is_stored() {
    let dir = temp_upload_dir("accepted");
    let (app, state) = make_app(dir.clone(), RateLimiter::new()).await;
    open_session(&state, "session-a", "alice").await;

    let status = post_upload(&app, body(&[("session", "session-a")], "cat.png", PNG)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored_files(&dir).len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_filename_with_a_dot_run_is_stored_under_a_registrable_key() {
    let dir = temp_upload_dir("dot-run");
    let (app, state) = make_app(dir.clone(), RateLimiter::new()).await;
    open_session(&state, "session-d", "dora").await;

    let status = post_upload(&app, body(&[("session", "session-d")], "wow...png", PNG)).await;

    // Every `/files/<key>` validator refuses "..", so a key stored with one
    // could be uploaded but never registered as an emoji, avatar or sound.
    assert_eq!(status, StatusCode::OK);
    let stored = stored_files(&dir);
    assert_eq!(stored.len(), 1);
    let url = format!("/files/{}", stored[0]);
    assert!(murmer_server::ws::validation::upload_key_from_url(&url).is_some());
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
async fn a_session_this_server_never_issued_is_rejected() {
    let dir = temp_upload_dir("unknown-session");
    let (app, state) = make_app(dir.clone(), RateLimiter::new()).await;
    open_session(&state, "session-m", "mallory").await;

    // A closed connection's session, or one issued by another server, is not
    // in the table.
    let status = post_upload(&app, body(&[("session", "session-x")], "cat.png", PNG)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(stored_files(&dir).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn uploads_are_rate_limited_per_ip() {
    let dir = temp_upload_dir("rate-limit");
    let mut limiter = RateLimiter::new();
    limiter.max_uploads_per_minute = 1;
    let (app, state) = make_app(dir.clone(), limiter).await;
    open_session(&state, "session-c", "carol").await;

    for expected in [StatusCode::OK, StatusCode::TOO_MANY_REQUESTS] {
        let status = post_upload(&app, body(&[("session", "session-c")], "cat.png", PNG)).await;
        assert_eq!(status, expected);
    }

    assert_eq!(stored_files(&dir).len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}
