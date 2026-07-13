//! Constants used in WebSocket message handling.

/// Roles that are allowed to create or delete channels when administrative controls are enabled.
pub const CHANNEL_MANAGE_ROLES: &[&str] = &["Admin", "Mod", "Owner"];

/// Roles that are allowed to assign or remove roles from other users.
pub const ROLE_MANAGE_ROLES: &[&str] = &["Owner"];

/// Roles that are allowed to query server details such as the running version.
pub const SERVER_INFO_ROLES: &[&str] = &["Owner", "Admin"];

/// Roles that are allowed to view other users' self-reported connection stats.
pub const CONNECTION_STATS_ROLES: &[&str] = &["Owner", "Admin"];

/// Upper bound accepted for reported latency/jitter values in milliseconds.
pub const MAX_REPORTED_STAT_MS: f64 = 60_000.0;

/// Maximum duration in seconds for a timed mute (30 days).
pub const MAX_MUTE_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Minimum duration in seconds for a timed mute.
pub const MIN_MUTE_SECONDS: i64 = 10;

/// Default quality label assigned to new voice channels.
pub const DEFAULT_VOICE_QUALITY: &str = "standard";

/// Default bitrate (in bits per second) assigned to new voice channels.
pub const DEFAULT_VOICE_BITRATE: i32 = 64_000;

/// Upper bound to reject unreasonable bitrate configuration values.
pub const MAX_ALLOWED_VOICE_BITRATE: i32 = 320_000;

/// Allowed user status values broadcast to clients.
pub const USER_STATUSES: &[&str] = &["online", "away", "busy", "offline"];

/// Minimum duration in seconds for ephemeral messages.
pub const MIN_EPHEMERAL_SECONDS: i64 = 5;

/// Maximum duration in seconds for ephemeral messages.
pub const MAX_EPHEMERAL_SECONDS: i64 = 86_400;

/// Maximum length in bytes for a chat message's text content.
pub const MAX_MESSAGE_LENGTH: usize = 4000;

/// Maximum length in bytes for a channel topic/description.
pub const MAX_TOPIC_LENGTH: usize = 256;

/// Maximum number of search results to return.
pub const MAX_SEARCH_RESULTS: i64 = 200;

/// Maximum number of messages to load in a single history request.
pub const MAX_HISTORY_LIMIT: i64 = 200;

/// Maximum number of characters preserved in a reply's quoted snippet.
pub const MAX_REPLY_PREVIEW_CHARS: usize = 200;

/// Maximum number of messages returned for a single thread.
pub const MAX_THREAD_MESSAGES: i64 = 200;

/// Minimum interval between typing broadcasts from a single connection.
pub const TYPING_BROADCAST_INTERVAL_MS: u64 = 1_000;

/// Maximum number of pinned messages per channel.
pub const MAX_PINS_PER_CHANNEL: i64 = 25;

/// Default number of messages to load when no limit is specified.
pub const DEFAULT_HISTORY_LIMIT: i64 = 50;
