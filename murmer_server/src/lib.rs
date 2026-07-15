//! Shared state structures and module re-exports for the Murmer server.
//!
//! The binary crate uses these exports and the integration tests link against
//! them to exercise rate limiting and validation logic.

pub mod admin;
pub mod bot;
pub mod config;
pub mod db;
pub mod link_preview;
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

/// A user's latest self-reported connection quality numbers (see the
/// `connection-stats` WebSocket message). Held in memory only and removed on
/// disconnect — never persisted — because the data is only useful for live
/// troubleshooting and must not become a long-term activity record.
#[derive(Clone)]
pub struct ConnectionStatsEntry {
    /// WebSocket round-trip time to this server in milliseconds.
    pub ping_ms: Option<f64>,
    /// Worst peer-to-peer voice round-trip time in milliseconds.
    pub voice_rtt_ms: Option<f64>,
    /// Worst voice jitter in milliseconds.
    pub voice_jitter_ms: Option<f64>,
    /// Worst voice packet loss in percent (0–100).
    pub voice_loss_percent: Option<f64>,
    pub updated_at: Instant,
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
    pub db: db::Db,
    pub users: Arc<Mutex<HashSet<String>>>,
    pub known_users: Arc<Mutex<HashSet<String>>>,
    /// Voice channel state, keyed by voice channel ID.
    pub voice_channels: Arc<Mutex<HashMap<i32, VoiceChannelState>>>,
    pub roles: Arc<Mutex<HashMap<String, RoleInfo>>>,
    pub statuses: Arc<Mutex<HashMap<String, String>>>,
    pub user_keys: Arc<Mutex<HashMap<String, String>>>,
    /// Active mutes keyed by public key; `None` means muted indefinitely.
    pub mutes: Arc<Mutex<HashMap<String, Option<chrono::DateTime<chrono::Utc>>>>>,
    /// Active screen shares per voice channel: channel_id -> set of usernames sharing.
    pub active_screen_shares: Arc<Mutex<HashMap<i32, HashSet<String>>>>,
    /// Voice mute state per user: username -> (microphone_muted, output_muted).
    pub voice_mutes: Arc<Mutex<HashMap<String, (bool, bool)>>>,
    /// Latest self-reported connection stats per user (in-memory only).
    pub connection_stats: Arc<Mutex<HashMap<String, ConnectionStatsEntry>>>,
    /// When each user joined a voice channel; used to accumulate lifetime
    /// voice minutes when they leave (only if stat tracking is enabled).
    pub voice_session_starts: Arc<Mutex<HashMap<String, Instant>>>,
    /// When each user started screen sharing; mirrors `voice_session_starts`.
    pub screenshare_session_starts: Arc<Mutex<HashMap<String, Instant>>>,
    pub upload_dir: PathBuf,
    pub password: Option<String>,
    pub admin_token: Option<String>,
    pub rate_limiter: RateLimiter,
}
