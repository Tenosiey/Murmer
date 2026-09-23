//! Murmer WebSocket server: provides text and voice chat over WebSocket with SQLite persistence.
//!
//! - `/ws`: WebSocket endpoint for chat and voice events.
//! - `/upload`: HTTP endpoint for uploading files (requires a signed proof
//!   from a key with an account on this server; see `upload.rs`).
//! - `/link-preview`: HTTP endpoint returning OpenGraph metadata for a URL.
//! - `/role`: HTTP endpoint for managing user roles (requires `ADMIN_TOKEN`).
//!
//! Configuration via environment variables:
//! - `DATABASE_PATH`: path to the SQLite database file (default: `murmer.db`).
//! - `UPLOAD_DIR`: directory for storing uploads (default: `uploads`).
//! - `SERVER_PASSWORD`: optional password for client authentication.
//! - `ADMIN_TOKEN`: token for admin role management.
//! - `BIND_ADDRESS`: optional socket address to bind to (defaults to `0.0.0.0:3001`).
//! - `CORS_ALLOW_ORIGINS`: comma separated list of origins allowed to access HTTP endpoints.
//! - `WEB_CLIENT_DIR`: directory with the built web client (`murmer_client/build`).
//!   When set, the client is served at `/`, on the same origin as `/ws` and
//!   `/upload` -- which is what lets a browser use it with CORS disabled.
//! - `STUN_SERVERS`: comma separated STUN URLs handed to clients for WebRTC
//!   (default: Google's public server; empty for none).
//! - `MAX_MESSAGES_PER_MINUTE`, `MAX_AUTH_ATTEMPTS_PER_MINUTE`,
//!   `MAX_UPLOADS_PER_MINUTE`, `NONCE_EXPIRY_SECONDS`: rate limiting overrides.
//!
//! Run with `cargo run` or via Docker Compose (`docker compose up --build`).
use anyhow::{Context, Result};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, StatusCode, header},
    routing::{get, post},
};
use dotenvy::dotenv;
use murmer_server::{
    AppState, VoiceChannelState, admin, automod, bot, config::Config, db, link_preview, upload,
    web_client, ws,
};
use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{Arc, OnceLock},
};
use tokio::{net::TcpListener, signal, sync::Mutex};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer, set_header::SetResponseHeaderLayer, trace::TraceLayer,
};
use tracing::info;

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

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "set-role" {
        return cli_set_role(&args[2..]).await;
    }
    if args.len() >= 2 && args[1] == "unbind-name" {
        return cli_unbind_name(&args[2..]).await;
    }

    init_tracing();
    // Uptime is what the dashboard's cumulative counters are divided by, so
    // it starts here rather than at whatever first touches one.
    murmer_server::metrics::mark_start();

    let config = Config::from_env()?;

    let db_client = db::init(&config.database_path)
        .await
        .context("failed to initialise database connection")?;

    let existing_voice = db::get_voice_channels(&db_client).await;

    // The in-memory mirrors below are what every permission, mute and
    // moderation check reads. Starting with an empty one after a failed load
    // would fail open until the next restart — no overrides makes every
    // private channel public, no mutes un-mutes everyone — so a load error
    // stops the server instead.
    let existing_mutes = db::get_all_mutes(&db_client)
        .await
        .context("failed to load mutes")?;
    let existing_role_defs = db::list_role_defs(&db_client)
        .await
        .context("failed to load role definitions")?;
    let existing_overrides = db::load_all_overrides(&db_client)
        .await
        .context("failed to load channel permission overrides")?;
    let chat_settings = db::chat_settings(&db_client)
        .await
        .context("failed to load chat settings")?;
    let automod_rules = db::automod_rules(&db_client)
        .await
        .context("failed to load auto-moderation rules")?;

    // Off is the safe default here: `db::record_user_stats` still enforces
    // the real gate, so this cache can only skip work, never record more.
    let stats_enabled = db::stats_server_enabled(&db_client).await.unwrap_or(false);

    tokio::fs::create_dir_all(&config.upload_dir)
        .await
        .with_context(|| {
            format!(
                "failed to create uploads directory '{}'",
                config.upload_dir.display()
            )
        })?;

    let voice_channels = existing_voice
        .iter()
        .map(|record| {
            let info = VoiceChannelState {
                name: record.name.clone(),
                users: HashSet::new(),
                quality: record.quality.clone(),
                bitrate: record.bitrate,
                category_id: record.category_id,
                position: record.position,
                breakout_parent: record.breakout_parent,
            };
            (record.id, info)
        })
        .collect();
    let state = Arc::new(AppState {
        voice_channels: Mutex::new(voice_channels),
        role_defs: Mutex::new(
            existing_role_defs
                .into_iter()
                .map(|def| (def.id, def))
                .collect(),
        ),
        channel_overrides: Mutex::new(existing_overrides),
        mutes: Mutex::new(existing_mutes.into_iter().collect()),
        upload_dir: config.upload_dir.clone(),
        password: config.password.clone(),
        admin_token: config.admin_token.clone(),
        stun_servers: config.stun_servers.clone(),
        stats_enabled: std::sync::atomic::AtomicBool::new(stats_enabled),
        chat_settings: Mutex::new(chat_settings),
        automod: Mutex::new(automod::RuleSet::compile(automod_rules)),
        ..AppState::new(db_client)
    });

    // Ephemeral deletion timers only live in memory; re-arm any that were
    // lost to a restart (expired ones are deleted immediately).
    ws::helpers::resume_ephemeral_deletions(&state).await;

    // A scheduled message left claimed by a process that stopped mid-delivery
    // may or may not have been posted, so it is reported to its author as a
    // failure rather than retried. Must run before the scheduler starts.
    ws::recover_claimed_scheduled_messages(&state).await;
    ws::spawn_scheduler(Arc::clone(&state));

    let mut router = Router::new()
        .route(
            "/ws",
            get(ws::ws_handler).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/upload",
            // The body limit is the hard ceiling for what an operator may
            // configure; the handler streams the body and aborts as soon as
            // the configured (usually much smaller) limit is passed.
            post(upload::upload).layer(DefaultBodyLimit::max(
                upload::MAX_CONFIGURABLE_FILE_SIZE + (1024_usize * 1024),
            )),
        )
        .route("/link-preview", get(link_preview::link_preview))
        .route("/role", post(admin::set_role))
        .merge(bot::routes::router())
        .nest_service("/files", upload::files_router(&config.upload_dir))
        .with_state(state);

    // Everything the API does not claim: the web client when one is
    // configured, otherwise the bare liveness response the health check and
    // the client's reachability probe expect from `/`.
    router = match &config.web_client_dir {
        Some(dir) => {
            info!(path = %dir.display(), "serving web client");
            router.fallback_service(web_client::router(dir))
        }
        None => router.route(
            "/",
            get(|| async { StatusCode::OK }).head(|| async { StatusCode::OK }),
        ),
    };

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
        if let Some(origins) = config.cors_origins() {
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

/// CLI subcommand: assign a role directly in the database.
///
/// Usage: `murmer_server set-role <public_key> <role> [color]`
///
/// Designed to be run from inside the Docker container so the server
/// operator can bootstrap the initial Owner without needing the client.
async fn cli_set_role(args: &[String]) -> Result<()> {
    if args.len() < 2 {
        eprintln!("Usage: murmer_server set-role <public_key> <role> [color]");
        eprintln!();
        eprintln!("Roles: Owner, Admin, Mod (or any custom name)");
        eprintln!("Color: optional hex color, e.g. #3b82f6");
        std::process::exit(1);
    }

    let key = &args[0];
    let role = &args[1];
    let color = args
        .get(2)
        .cloned()
        .or_else(|| murmer_server::roles::default_color(role));

    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "murmer.db".to_string());
    let client = db::init(&db_path)
        .await
        .context("failed to connect to database")?;

    let def = db::assign_named_role(&client, key, role, color.as_deref())
        .await
        .context("failed to set role in database")?;

    println!("Role '{}' assigned to key {key}", def.name);
    if let Some(c) = &def.color {
        println!("Color: {c}");
    }
    Ok(())
}

/// CLI subcommand: release the persisted binding between a user name and its
/// public key.
///
/// Usage: `murmer_server unbind-name <user_name>`
///
/// A name is claimed permanently by the first key that authenticates with
/// it; run this when a user lost their keypair and needs to reclaim their
/// name with a new one.
async fn cli_unbind_name(args: &[String]) -> Result<()> {
    let Some(name) = args.first() else {
        eprintln!("Usage: murmer_server unbind-name <user_name>");
        std::process::exit(1);
    };

    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "murmer.db".to_string());
    let client = db::init(&db_path)
        .await
        .context("failed to connect to database")?;

    if db::unbind_user_name(&client, name)
        .await
        .context("failed to remove name binding")?
    {
        println!("Name '{name}' released; the next key to authenticate with it claims it.");
    } else {
        println!("No binding found for name '{name}'.");
    }
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
