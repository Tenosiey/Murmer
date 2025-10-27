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
//! - `BIND_ADDRESS`: optional socket address to bind to (defaults to `0.0.0.0:3001`).
//! - `CORS_ALLOW_ORIGINS`: comma separated list of origins allowed to access HTTP endpoints.
//!
//! Run with `cargo run` or via Docker Compose (`docker compose up --build`).
use anyhow::{Context, Result};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, StatusCode, header},
    routing::{get, post},
};
use dotenvy::dotenv;
use murmer_server::{AppState, RateLimiter, VoiceChannelState, admin, db, upload, ws};
use std::{
    collections::{HashMap, HashSet},
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use tokio::{
    net::TcpListener,
    signal,
    sync::{Mutex, broadcast},
};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use tracing::info;

#[derive(Debug, Clone)]
struct Config {
    bind_addr: SocketAddr,
    database_url: String,
    upload_dir: PathBuf,
    password: Option<String>,
    admin_token: Option<String>,
    cors_allowlist: Option<Vec<HeaderValue>>,
}

impl Config {
    fn from_env() -> Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required"))?;
        let bind_addr = env::var("BIND_ADDRESS")
            .unwrap_or_else(|_| "0.0.0.0:3001".to_string())
            .parse()
            .context("failed to parse BIND_ADDRESS as a socket address")?;
        let upload_dir = env::var("UPLOAD_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("uploads"));
        let password = env::var("SERVER_PASSWORD").ok().filter(|s| !s.is_empty());
        let admin_token = env::var("ADMIN_TOKEN").ok().filter(|s| !s.is_empty());
        let cors_allowlist = match env::var("CORS_ALLOW_ORIGINS") {
            Ok(raw) => {
                let mut origins = Vec::new();
                for origin in raw.split(',') {
                    let trimmed = origin.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    origins.push(HeaderValue::from_str(trimmed).with_context(|| {
                        format!("invalid origin '{trimmed}' in CORS_ALLOW_ORIGINS")
                    })?);
                }
                if origins.is_empty() {
                    None
                } else {
                    Some(origins)
                }
            }
            Err(_) => None,
        };

        Ok(Self {
            bind_addr,
            database_url,
            upload_dir,
            password,
            admin_token,
            cors_allowlist,
        })
    }

    fn cors_layer(&self) -> Option<CorsLayer> {
        self.cors_allowlist.as_ref().map(|origins| {
            let allowed = AllowOrigin::list(origins.clone());
            CorsLayer::new()
                .allow_origin(allowed)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        })
    }
}

fn init_tracing() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("murmer_server=info,axum=info"));
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .compact()
            .init();
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    init_tracing();

    let config = Config::from_env()?;

    let (tx, _rx) = broadcast::channel::<String>(100);

    let db_client = db::init(&config.database_url)
        .await
        .context("failed to initialise database connection")?;

    let existing_voice = db::get_voice_channels(&db_client).await;

    tokio::fs::create_dir_all(&config.upload_dir)
        .await
        .with_context(|| {
            format!(
                "failed to create uploads directory '{}'",
                config.upload_dir.display()
            )
        })?;

    let state = Arc::new(AppState {
        tx: tx.clone(),
        channels: Arc::new(Mutex::new(HashMap::new())),
        db: Arc::new(db_client),
        users: Arc::new(Mutex::new(HashSet::new())),
        known_users: Arc::new(Mutex::new(HashSet::new())),
        voice_channels: Arc::new(Mutex::new({
            let mut map = HashMap::new();
            for record in &existing_voice {
                map.insert(
                    record.name.clone(),
                    VoiceChannelState {
                        users: HashSet::new(),
                        quality: record.quality.clone(),
                        bitrate: record.bitrate,
                    },
                );
            }
            map
        })),
        roles: Arc::new(Mutex::new(HashMap::new())),
        statuses: Arc::new(Mutex::new(HashMap::new())),
        user_keys: Arc::new(Mutex::new(HashMap::new())),
        upload_dir: config.upload_dir.clone(),
        password: config.password.clone(),
        admin_token: config.admin_token.clone(),
        rate_limiter: RateLimiter::default(),
    });

    let mut router = Router::new()
        .route(
            "/",
            get(|| async { StatusCode::OK }).head(|| async { StatusCode::OK }),
        )
        .route(
            "/ws",
            get(ws::ws_handler).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/upload",
            post(upload::upload).layer(DefaultBodyLimit::max(
                upload::MAX_FILE_SIZE + (1024_usize * 1024),
            )),
        )
        .route("/role", post(admin::set_role))
        .nest_service(
            "/files",
            ServeDir::new(&config.upload_dir).append_index_html_on_directories(false),
        )
        .with_state(state);

    let security_headers = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ));

    router = router
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(security_headers);

    if let Some(cors) = config.cors_layer() {
        if let Some(origins) = &config.cors_allowlist {
            info!(?origins, "CORS enabled for configured origins");
        }
        router = router.layer(cors);
    }

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("failed to bind to {}", config.bind_addr))?;
    info!(address = %config.bind_addr, "WebSocket server listening");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server task failed")?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = signal::ctrl_c().await {
            tracing::error!(?err, "failed to listen for ctrl+c");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => sig.recv().await,
            Err(err) => {
                tracing::error!(?err, "failed to listen for terminate signal");
                None
            }
        };
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
}
