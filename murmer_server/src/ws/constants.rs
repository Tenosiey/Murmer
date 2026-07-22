//! Constants used in WebSocket message handling.
//!
//! Feature authorization is decided by the permission bitmask in
//! [`crate::permissions`], not by hardcoded role-name lists.

/// Maximum number of custom emojis a server may register.
pub const MAX_CUSTOM_EMOJIS: i64 = 200;

/// Maximum number of role definitions a server may hold (including built-ins).
pub const MAX_ROLES: usize = 100;

/// Maximum length in bytes for a role name.
pub const MAX_ROLE_NAME_LENGTH: usize = 32;

/// Maximum file size in bytes for a custom emoji image.
pub const MAX_EMOJI_FILE_BYTES: u64 = 512 * 1024;

/// Minimum length of a custom emoji name.
pub const MIN_EMOJI_NAME_LEN: usize = 2;

/// Maximum length of a custom emoji name.
pub const MAX_EMOJI_NAME_LEN: usize = 32;

/// Maximum length in bytes for the server display name.
pub const MAX_SERVER_NAME_LENGTH: usize = 64;

/// Maximum length in bytes for the server description.
pub const MAX_SERVER_DESCRIPTION_LENGTH: usize = 300;

/// Maximum length in bytes for the welcome message.
pub const MAX_WELCOME_MESSAGE_LENGTH: usize = 500;

/// Maximum file size in bytes for the server icon image.
pub const MAX_SERVER_ICON_BYTES: u64 = 1024 * 1024;

/// Maximum file size in bytes for a user avatar image.
pub const MAX_AVATAR_BYTES: u64 = 1024 * 1024;

/// File extensions accepted for image uploads referenced over the WebSocket
/// (custom emojis, server icon). Subset of the upload endpoint's safe-list.
pub const UPLOAD_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp"];

/// Bounds accepted for the screen share bitrate cap in bits per second.
pub const MIN_SCREENSHARE_BITRATE: u64 = 100_000;
pub const MAX_SCREENSHARE_BITRATE: u64 = 100_000_000;

/// Maximum number of favorite reactions returned with a stats snapshot.
pub const MAX_FAVORITE_REACTIONS: i64 = 5;

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

/// Maximum number of ids accepted in a single reorder request (channels of
/// one category, or all categories).
pub const MAX_REORDER_IDS: usize = 200;

/// Allowed user status values broadcast to clients.
pub const USER_STATUSES: &[&str] = &["online", "away", "busy", "offline"];

/// Minimum duration in seconds for ephemeral messages.
pub const MIN_EPHEMERAL_SECONDS: i64 = 5;

/// Maximum duration in seconds for ephemeral messages.
pub const MAX_EPHEMERAL_SECONDS: i64 = 86_400;

/// Maximum length in bytes for a chat message's text content.
pub const MAX_MESSAGE_LENGTH: usize = 4000;

/// Exact decoded length in bytes of a direct message's NaCl box nonce.
pub const DM_NONCE_BYTES: usize = 24;

/// Poly1305 authenticator bytes appended to every NaCl box ciphertext; a
/// direct message ciphertext may exceed [`MAX_MESSAGE_LENGTH`] by this much.
pub const DM_CIPHERTEXT_OVERHEAD_BYTES: usize = 16;

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

/// Maximum length in bytes for a wiki page slug.
pub const MAX_WIKI_SLUG_LENGTH: usize = 64;

/// Maximum length in characters for a wiki page title.
pub const MAX_WIKI_TITLE_LENGTH: usize = 100;

/// Maximum length in bytes for a wiki page's Markdown body.
pub const MAX_WIKI_BODY_BYTES: usize = 100_000;

/// Maximum number of wiki pages per channel.
pub const MAX_WIKI_PAGES_PER_CHANNEL: i64 = 100;

/// Number of revisions kept per wiki page; older ones are pruned on save.
pub const MAX_WIKI_REVISIONS_KEPT: i64 = 50;

/// Maximum number of links accepted in a single wiki-resolve request.
pub const MAX_WIKI_RESOLVE_LINKS: usize = 50;
