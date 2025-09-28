//! Murmer WebSocket server: provides text and voice chat over WebSocket with Postgres persistence.
//!
//! - `/ws`: WebSocket endpoint for chat and voice events.
//! - `/upload`: HTTP endpoint for uploading files.
//! - `/role`: HTTP endpoint for managing user roles (requires `ADMIN_TOKEN`).
//!
//! Configuration via environment variables:
//! - `DATABASE_URL`: Postgres connection string (required).
//! - `UPLOAD_DIR`: directory for storing uploads (default: `uploads`).
//! - `SERVER_PASSWORD`: optional password for client authentication.
//! - `ADMIN_TOKEN`: token for admin role management.
//!
//! Run with `cargo run` or via Docker Compose (`docker compose up --build`).
mod admin;
mod db;
mod roles;
mod security;
mod upload;
mod ws;

use axum::{
    Router,
    routing::{get, post},
};
use roles::RoleInfo;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, broadcast};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

/// Rate limiting state for tracking user actions
pub struct RateLimiter {
    /// Track message timestamps per user (user -> timestamps)
    pub message_times: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    /// Track authentication attempt timestamps per IP (ip -> timestamps)
    pub auth_attempts: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    /// Track used nonces to prevent replay attacks (nonce -> expiry_time)
    pub used_nonces: Arc<Mutex<HashMap<String, Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            message_times: Arc::new(Mutex::new(HashMap::new())),
            auth_attempts: Arc::new(Mutex::new(HashMap::new())),
            used_nonces: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Shared application state passed to handlers.
/// - `tx`: broadcast channel for global events (online users, voice updates).
/// - `channels`: per-text-channel broadcast senders.
/// - `db`: PostgreSQL client for persisting chat history.
/// - `users`: set of currently connected chat users.
/// - `known_users`: set of all users that have ever joined while the server is running.
/// - `voice_channels`: map of voice channel names to active users.
/// - `upload_dir`: directory where uploaded files are stored.
/// - `rate_limiter`: rate limiting and replay protection.
pub struct AppState {
    pub tx: broadcast::Sender<String>,
    pub channels: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
    pub db: Arc<tokio_postgres::Client>,
    pub users: Arc<Mutex<HashSet<String>>>,
    pub known_users: Arc<Mutex<HashSet<String>>>,
    pub voice_channels: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    pub roles: Arc<Mutex<HashMap<String, RoleInfo>>>,
    pub statuses: Arc<Mutex<HashMap<String, String>>>,
    pub user_keys: Arc<Mutex<HashMap<String, String>>>,
    pub upload_dir: PathBuf,
    pub password: Option<String>,
    pub admin_token: Option<String>,
    pub rate_limiter: RateLimiter,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    // The server requires a PostgreSQL database connection string via
    // `DATABASE_URL`. Uploaded files are stored under `UPLOAD_DIR`
    // (defaults to "uploads" if unset).
    let (tx, _rx) = broadcast::channel::<String>(100);

    let db_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable is required")
        .expect("DATABASE_URL not set");
    let db_client = db::init(&db_url)
        .await
        .expect("Failed to initialize database connection");

    let existing_voice = db::get_voice_channels(&db_client).await;

    let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| "uploads".to_string());
    if let Err(e) = tokio::fs::create_dir_all(&upload_dir).await {
        tracing::error!("Failed to create uploads directory '{}': {}", upload_dir, e);
        std::process::exit(1);
    }

    let password = env::var("SERVER_PASSWORD").ok();
    let admin_token = env::var("ADMIN_TOKEN").ok();

    let state = Arc::new(AppState {
        tx: tx.clone(),
        channels: Arc::new(Mutex::new(HashMap::new())),
        db: Arc::new(db_client),
        users: Arc::new(Mutex::new(HashSet::new())),
        known_users: Arc::new(Mutex::new(HashSet::new())),
        voice_channels: Arc::new(Mutex::new({
            let mut map = HashMap::new();
            for name in &existing_voice {
                map.insert(name.clone(), HashSet::new());
            }
            map
        })),
        roles: Arc::new(Mutex::new(HashMap::new())),
        statuses: Arc::new(Mutex::new(HashMap::new())),
        user_keys: Arc::new(Mutex::new(HashMap::new())),
        upload_dir: PathBuf::from(upload_dir.clone()),
        password,
        admin_token,
        rate_limiter: RateLimiter::new(),
    });

    use axum::http::StatusCode;

    let mut app = Router::new()
        .route(
            "/",
            get(|| async { "OK" }).head(|| async { StatusCode::OK }),
        )
        .route("/ws", get(ws::ws_handler))
        .route("/upload", post(upload::upload))
        .route("/role", post(admin::set_role))
        .nest_service("/files", ServeDir::new(upload_dir))
        .with_state(state);

    // Add CORS if ENABLE_CORS environment variable is set (for development)
    if env::var("ENABLE_CORS").is_ok() {
        info!("CORS enabled via ENABLE_CORS environment variable");
        app = app.layer(CorsLayer::permissive());
    } else {
        info!("CORS disabled - production mode");
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    info!("WebSocket server running on {addr}");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
