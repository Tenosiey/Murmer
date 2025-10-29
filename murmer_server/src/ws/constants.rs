//! Constants used in WebSocket message handling.

/// Roles that are allowed to create or delete channels when administrative controls are enabled.
pub const CHANNEL_MANAGE_ROLES: &[&str] = &["Admin", "Mod", "Owner"];

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

/// Maximum number of search results to return.
pub const MAX_SEARCH_RESULTS: i64 = 200;

/// Maximum number of messages to load in a single history request.
pub const MAX_HISTORY_LIMIT: i64 = 200;

/// Default number of messages to load when no limit is specified.
pub const DEFAULT_HISTORY_LIMIT: i64 = 50;

