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
mod upload;
mod ws;

use axum::{
    Router,
    routing::{get, post},
};
use roles::RoleInfo;
use std::{
    collections::{HashMap, HashSet},
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, broadcast};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

/// Shared application state passed to handlers.
/// - `tx`: broadcast channel for global events (online users, voice updates).
/// - `channels`: per-text-channel broadcast senders.
/// - `db`: PostgreSQL client for persisting chat history.
/// - `users`: set of currently connected chat users.
/// - `known_users`: set of all users that have ever joined while the server is running.
/// - `voice_users`: set of users active in voice chat.
/// - `upload_dir`: directory where uploaded files are stored.
pub struct AppState {
    pub tx: broadcast::Sender<String>,
    pub channels: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
    pub db: Arc<tokio_postgres::Client>,
    pub users: Arc<Mutex<HashSet<String>>>,
    pub known_users: Arc<Mutex<HashSet<String>>>,
    pub voice_users: Arc<Mutex<HashSet<String>>>,
    pub roles: Arc<Mutex<HashMap<String, RoleInfo>>>,
    pub user_keys: Arc<Mutex<HashMap<String, String>>>,
    pub upload_dir: PathBuf,
    pub password: Option<String>,
    pub admin_token: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    // The server requires a PostgreSQL database connection string via
    // `DATABASE_URL`. Uploaded files are stored under `UPLOAD_DIR`
    // (defaults to "uploads" if unset).
    let (tx, _rx) = broadcast::channel::<String>(100);

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let db_client = db::init(&db_url).await;

    let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| "uploads".to_string());
    if let Err(e) = tokio::fs::create_dir_all(&upload_dir).await {
        panic!("create uploads dir: {e}");
    }

    let password = env::var("SERVER_PASSWORD").ok();
    let admin_token = env::var("ADMIN_TOKEN").ok();

    let state = Arc::new(AppState {
        tx: tx.clone(),
        channels: Arc::new(Mutex::new(HashMap::new())),
        db: Arc::new(db_client),
        users: Arc::new(Mutex::new(HashSet::new())),
        known_users: Arc::new(Mutex::new(HashSet::new())),
        voice_users: Arc::new(Mutex::new(HashSet::new())),
        roles: Arc::new(Mutex::new(HashMap::new())),
        user_keys: Arc::new(Mutex::new(HashMap::new())),
        upload_dir: PathBuf::from(upload_dir.clone()),
        password,
        admin_token,
    });

    let cors = CorsLayer::permissive();

    use axum::http::StatusCode;

    let app = Router::new()
        .route(
            "/",
            get(|| async { "OK" }).head(|| async { StatusCode::OK }),
        )
        .route("/ws", get(ws::ws_handler))
        .route("/upload", post(upload::upload))
        .route("/role", post(admin::set_role))
        .nest_service("/files", ServeDir::new(upload_dir))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    info!("WebSocket server running on {addr}");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
