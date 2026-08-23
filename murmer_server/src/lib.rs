//! Shared state structures and module re-exports for the Murmer server.
//!
//! The binary crate uses these exports and the integration tests link against
//! them to exercise rate limiting and validation logic.

pub mod admin;
pub mod bot;
pub mod channel_overrides;
pub mod config;
pub mod db;
pub mod link_preview;
pub mod permissions;
pub mod profanity;
pub mod roles;
pub mod security;
pub mod upload;
pub mod ws;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tokio::sync::{Mutex, broadcast, mpsc};

pub use roles::RoleDef;

/// A serialized WebSocket frame as it travels over the broadcast channels.
///
/// [`broadcast`] hands every receiver its own clone, so the payload type
/// decides what a fan-out costs. `Utf8Bytes` is a reference-counted `Bytes`
/// behind a UTF-8 guarantee: building one from a `String` moves the
/// allocation, cloning it per receiver is a refcount bump, and
/// `Message::Text` takes it by value — so a frame sent to N clients is
/// allocated and copied exactly once instead of N times. It derefs to `str`,
/// so receivers can still inspect the JSON without converting back.
pub type Frame = axum::extract::ws::Utf8Bytes;

/// How many undelivered frames a single connection's direct mailbox holds
/// before further ones are dropped.
///
/// Point-to-point delivery is bounded rather than unbounded so one stalled
/// client cannot make the server hold frames for it without limit. Dropping
/// mirrors what the broadcast channels already do to a receiver that falls
/// behind, and the depth is far above the burst a call setup produces.
pub const DIRECT_MAILBOX_CAPACITY: usize = 256;

/// The mailboxes of every open connection, so a frame can be addressed to one
/// user instead of broadcast to everyone.
///
/// One user may hold several entries — the same account can be signed in from
/// more than one client — so connections are keyed by a unique id within the
/// user's entry, which is also what lets a disconnect remove exactly its own
/// mailbox and not a newer one belonging to the same name.
pub type DirectRegistry = HashMap<String, HashMap<u64, mpsc::Sender<Frame>>>;

/// Where the rate limiter reads "now" from.
///
/// Production uses [`Clock::system`], which is `Instant::now()` and nothing
/// else. Tests use [`Clock::manual`] plus [`Clock::advance`]: the sliding
/// window and the map sweep are both measured in minutes, and a
/// `std::time::Instant` cannot be moved by hand — so without this the sweep
/// could only be covered by a test that really sleeps for over a minute,
/// which is to say not at all.
#[derive(Clone)]
pub struct Clock {
    /// The instant a manual clock was created at and how far it has since
    /// been advanced, in nanoseconds. `None` is the real clock.
    manual: Option<(Instant, Arc<AtomicU64>)>,
}

impl Clock {
    /// The real monotonic clock.
    pub fn system() -> Self {
        Self { manual: None }
    }

    /// A clock that only moves when [`advance`](Self::advance) is called.
    ///
    /// It starts at the current instant rather than at some epoch because
    /// `Instant` has no constructor — the offset is what the test controls.
    pub fn manual() -> Self {
        Self {
            manual: Some((Instant::now(), Arc::new(AtomicU64::new(0)))),
        }
    }

    /// Read the current instant.
    pub fn now(&self) -> Instant {
        match &self.manual {
            None => Instant::now(),
            Some((base, offset)) => {
                *base + std::time::Duration::from_nanos(offset.load(Ordering::Relaxed))
            }
        }
    }

    /// Move a manual clock forward. A no-op on [`Clock::system`], which
    /// nothing but the passage of time can advance.
    pub fn advance(&self, by: std::time::Duration) {
        if let Some((_, offset)) = &self.manual {
            offset.fetch_add(by.as_nanos() as u64, Ordering::Relaxed);
        }
    }
}

/// A set of sliding windows keyed by user name, IP or nonce, plus the last
/// time the whole map was swept for entries that fell out of their window.
///
/// Keeping the sweep marker next to the data it describes means one lock
/// covers both, so the limiter never has to decide between a stale marker and
/// a second round of locking.
pub struct SlidingWindows<T> {
    pub entries: HashMap<String, T>,
    /// When [`entries`](Self::entries) was last pruned end to end.
    pub last_sweep: Instant,
}

impl<T> SlidingWindows<T> {
    /// `now` comes from the limiter's [`Clock`] rather than from
    /// `Instant::now()`, so a manual clock starts its first sweep interval at
    /// the same instant every later check is measured against.
    fn new(now: Instant) -> Self {
        Self {
            entries: HashMap::new(),
            last_sweep: now,
        }
    }
}

/// Tracks rate limiting state for authentication, messaging, uploads and
/// nonce usage.
///
/// The limits are resolved from the environment once, when the limiter
/// is built, instead of on every check: `check_message_rate_limit` runs on
/// every chat frame and `std::env::var` allocates and walks the environment
/// each time it is called.
pub struct RateLimiter {
    /// Message timestamps per user (user -> timestamps).
    pub message_times: Arc<Mutex<SlidingWindows<VecDeque<Instant>>>>,
    /// Authentication attempt timestamps per IP (ip -> timestamps).
    pub auth_attempts: Arc<Mutex<SlidingWindows<VecDeque<Instant>>>>,
    /// Upload attempt timestamps per IP (ip -> timestamps).
    pub upload_attempts: Arc<Mutex<SlidingWindows<VecDeque<Instant>>>>,
    /// Used nonces to prevent replay attacks (nonce -> first seen time).
    pub used_nonces: Arc<Mutex<SlidingWindows<Instant>>>,
    /// Messages one user may send per minute.
    pub max_messages_per_minute: usize,
    /// Authentication attempts one IP may make per minute.
    pub max_auth_attempts_per_minute: usize,
    /// Uploads one IP may make per minute.
    pub max_uploads_per_minute: usize,
    /// How long a used nonce stays remembered.
    pub nonce_expiry: std::time::Duration,
    /// Where every window and expiry check reads "now" from.
    pub clock: Clock,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::with_clock(Clock::system())
    }

    /// Build a limiter reading time from `clock`, for tests that need to
    /// fast-forward past the sliding window or the sweep interval.
    pub fn with_clock(clock: Clock) -> Self {
        let now = clock.now();
        Self {
            message_times: Arc::new(Mutex::new(SlidingWindows::new(now))),
            auth_attempts: Arc::new(Mutex::new(SlidingWindows::new(now))),
            upload_attempts: Arc::new(Mutex::new(SlidingWindows::new(now))),
            used_nonces: Arc::new(Mutex::new(SlidingWindows::new(now))),
            max_messages_per_minute: security::get_max_messages_per_minute(),
            max_auth_attempts_per_minute: security::get_max_auth_attempts_per_minute(),
            max_uploads_per_minute: security::get_max_uploads_per_minute(),
            nonce_expiry: std::time::Duration::from_secs(security::get_nonce_expiry_seconds()),
            clock,
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
    pub position: i32,
}

/// Shared application state passed to handlers.
pub struct AppState {
    pub tx: broadcast::Sender<Frame>,
    /// Per-text-channel broadcast senders, keyed by channel ID.
    pub channels: Arc<Mutex<HashMap<i32, broadcast::Sender<Frame>>>>,
    /// Mailboxes for frames addressed to a single user (see [`DirectRegistry`]).
    /// WebRTC signaling goes here: an offer, answer or ICE candidate concerns
    /// exactly two peers, so broadcasting it made every client parse and
    /// discard a frame that was never theirs.
    pub direct: Arc<Mutex<DirectRegistry>>,
    pub db: db::Db,
    pub users: Arc<Mutex<HashSet<String>>>,
    pub known_users: Arc<Mutex<HashSet<String>>>,
    /// Voice channel state, keyed by voice channel ID.
    pub voice_channels: Arc<Mutex<HashMap<i32, VoiceChannelState>>>,
    /// All role definitions, keyed by role id. Loaded at startup and mutated
    /// as roles are created/edited/deleted.
    pub role_defs: Arc<Mutex<HashMap<i64, RoleDef>>>,
    /// Roles assigned to each connected user (username → role ids). Populated
    /// at authentication from the `user_roles` table.
    pub user_roles: Arc<Mutex<HashMap<String, Vec<i64>>>>,
    /// Per-channel permission overrides, keyed by (kind, channel id). Loaded at
    /// startup and mutated as channel permissions change.
    pub channel_overrides:
        Arc<Mutex<HashMap<(channel_overrides::ChannelKind, i32), channel_overrides::OverrideSet>>>,
    pub statuses: Arc<Mutex<HashMap<String, String>>>,
    pub user_keys: Arc<Mutex<HashMap<String, String>>>,
    /// Active mutes keyed by public key; `None` means muted indefinitely.
    pub mutes: Arc<Mutex<HashMap<String, Option<chrono::DateTime<chrono::Utc>>>>>,
    /// Active screen shares per voice channel: channel_id -> set of usernames sharing.
    pub active_screen_shares: Arc<Mutex<HashMap<i32, HashSet<String>>>>,
    /// Cameras currently on per voice channel: channel_id -> set of usernames.
    /// The video itself rides the existing voice peer connections; this is only
    /// the announcement, so a client joining later learns whose camera is on.
    pub active_webcams: Arc<Mutex<HashMap<i32, HashSet<String>>>>,
    /// Voice mute state per user: username -> (microphone_muted, output_muted).
    pub voice_mutes: Arc<Mutex<HashMap<String, (bool, bool)>>>,
    /// Latest self-reported connection stats per user (in-memory only).
    pub connection_stats: Arc<Mutex<HashMap<String, ConnectionStatsEntry>>>,
    /// When each user joined a voice channel; used to accumulate lifetime
    /// voice minutes when they leave (only if stat tracking is enabled).
    pub voice_session_starts: Arc<Mutex<HashMap<String, Instant>>>,
    /// When each user started screen sharing; mirrors `voice_session_starts`.
    pub screenshare_session_starts: Arc<Mutex<HashMap<String, Instant>>>,
    /// When each user last played a soundboard sound, for the server-side
    /// playback cooldown. Entries are dropped on disconnect.
    pub soundboard_cooldowns: Arc<Mutex<HashMap<String, Instant>>>,
    pub upload_dir: PathBuf,
    pub password: Option<String>,
    pub admin_token: Option<String>,
    pub rate_limiter: RateLimiter,
    /// Mirror of the server-wide stat tracking toggle (`server_settings` key
    /// `stats_enabled`), kept in memory so the recording hooks that fire on
    /// every message can skip the database entirely while tracking is off —
    /// which is the default and the common case.
    ///
    /// This is an optimisation, never an authorization decision: the real
    /// double opt-in gate stays inside `db::record_user_stats`, in the same
    /// database call that performs the increments. A stale value here can only
    /// cost a wasted query, never record a counter the gate would refuse.
    pub stats_enabled: std::sync::atomic::AtomicBool,
    /// Cached chat policy (`server_settings`: slow mode, message length cap,
    /// profanity filter). Every chat message consults all three, so they are
    /// held in memory rather than read back per message; the handler that
    /// writes them refreshes this in the same step.
    pub chat_settings: Arc<Mutex<db::ChatSettings>>,
    /// When each user last had a message accepted, for the slow mode gate.
    /// In-memory only and pruned on disconnect: slow mode is a pacing tool,
    /// not a punishment to be remembered across sessions.
    pub slow_mode_sends: Arc<Mutex<HashMap<String, Instant>>>,
}
