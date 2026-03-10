//! Shared state structures and module re-exports for the Murmer server.
//!
//! The binary crate uses these exports and the integration tests link against
//! them to exercise rate limiting and validation logic.

pub mod admin;
pub mod bot;
pub mod config;
pub mod db;
pub mod roles;
pub mod security;
pub mod upload;
pub mod ws;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use tokio::sync::{broadcast, Mutex};

pub use roles::RoleInfo;

/// Tracks rate limiting state for authentication, messaging and nonce usage.
pub struct RateLimiter {
    /// Message timestamps per user (user -> timestamps).
    pub message_times: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    /// Authentication attempt timestamps per IP (ip -> timestamps).
    pub auth_attempts: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    /// Used nonces to prevent replay attacks (nonce -> first seen time).
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

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of connected users within a voice channel.
#[derive(Clone)]
pub struct VoiceChannelState {
    pub name: String,
    pub users: HashSet<String>,
    pub quality: String,
    pub bitrate: Option<i32>,
    pub category_id: Option<i32>,
}

/// Shared application state passed to handlers.
pub struct AppState {
    pub tx: broadcast::Sender<String>,
    /// Per-text-channel broadcast senders, keyed by channel ID.
    pub channels: Arc<Mutex<HashMap<i32, broadcast::Sender<String>>>>,
    pub db: Arc<tokio_postgres::Client>,
    pub users: Arc<Mutex<HashSet<String>>>,
    pub known_users: Arc<Mutex<HashSet<String>>>,
    /// Voice channel state, keyed by voice channel ID.
    pub voice_channels: Arc<Mutex<HashMap<i32, VoiceChannelState>>>,
    pub roles: Arc<Mutex<HashMap<String, RoleInfo>>>,
    pub statuses: Arc<Mutex<HashMap<String, String>>>,
    pub user_keys: Arc<Mutex<HashMap<String, String>>>,
    /// Active screen shares per voice channel: channel_id -> set of usernames sharing.
    pub active_screen_shares: Arc<Mutex<HashMap<i32, HashSet<String>>>>,
    pub upload_dir: PathBuf,
    pub password: Option<String>,
    pub admin_token: Option<String>,
    pub rate_limiter: RateLimiter,
}
