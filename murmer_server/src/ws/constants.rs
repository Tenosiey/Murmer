//! Constants used in WebSocket message handling.
//!
//! Feature authorization is decided by the permission bitmask in
//! [`crate::permissions`], not by hardcoded role-name lists.

/// Maximum number of custom emojis a server may register.
pub const MAX_CUSTOM_EMOJIS: i64 = 200;

/// Number of random bytes behind an invite code. Base64url-encoded these
/// become 16 characters, which is short enough to read out over voice and far
/// too much to guess: an invite is a credential on a password-protected
/// server, and nothing rate-limits a redemption beyond the auth limiter.
pub const INVITE_CODE_BYTES: usize = 12;

/// Maximum number of invites a server may hold at once. Revoking is a row
/// delete, so this is a cap on live invites rather than on invites ever made.
pub const MAX_INVITES: i64 = 100;

/// Largest use limit an invite may be created with. Beyond this the sensible
/// answer is an unlimited invite (`maxUses` of 0), not a bigger number.
pub const MAX_INVITE_USES: i64 = 1_000;

/// Longest lifetime an invite may be created with (30 days). An invite that
/// should outlive a month is one that never expires, and saying so is more
/// honest than a date three years out.
pub const MAX_INVITE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Maximum number of role definitions a server may hold (including built-ins).
pub const MAX_ROLES: usize = 100;

/// Maximum length in bytes for a role name.
pub const MAX_ROLE_NAME_LENGTH: usize = 32;

/// Maximum file size in bytes for a role icon image. Role icons render at
/// badge size next to a name, so they get the emoji budget, not the icon one.
pub const MAX_ROLE_ICON_BYTES: u64 = 512 * 1024;

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

/// Maximum length in characters for a user's display name. The display name
/// is cosmetic — the account name stays the identity — so it only has to fit
/// next to a message without pushing the timestamp off screen.
pub const MAX_DISPLAY_NAME_LENGTH: usize = 32;

/// Maximum length in characters for a member's per-server nickname. Same
/// budget as the display name: both end up in the same slot in the member
/// list and next to a message.
pub const MAX_NICKNAME_LENGTH: usize = 32;

/// Maximum length in characters for a user's profile "about" text.
pub const MAX_ABOUT_LENGTH: usize = 300;

/// File extensions accepted for image uploads referenced over the WebSocket
/// (custom emojis, server icon). Subset of the upload endpoint's safe-list.
pub const UPLOAD_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp"];

/// Maximum number of soundboard sounds a server may register.
pub const MAX_SOUNDBOARD_SOUNDS: i64 = 100;

/// Maximum file size in bytes for a soundboard sound. Small on purpose: these
/// clips are auto-played to everyone in a voice channel, so a long file is
/// abuse rather than a feature.
pub const MAX_SOUND_FILE_BYTES: u64 = 1024 * 1024;

/// Minimum length of a soundboard sound's display name.
pub const MIN_SOUND_NAME_LEN: usize = 2;

/// Maximum length of a soundboard sound's display name.
pub const MAX_SOUND_NAME_LEN: usize = 32;

/// File extensions accepted for soundboard uploads referenced over the
/// WebSocket. Subset of the upload endpoint's `audio` category.
pub const UPLOAD_SOUND_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "m4a", "opus"];

/// Bytes read from the head of an uploaded sound for magic-byte validation.
pub const SOUND_MAGIC_BYTES: usize = 12;

/// Minimum interval in milliseconds between two `play-sound` frames from the
/// same user. Enforced server-side: a soundboard is the most spammable thing
/// in the app and a patched client must not be able to bypass this.
pub const SOUNDBOARD_COOLDOWN_MS: u64 = 3_000;

/// Bounds accepted for the screen share bitrate cap in bits per second.
pub const MIN_SCREENSHARE_BITRATE: u64 = 100_000;
pub const MAX_SCREENSHARE_BITRATE: u64 = 100_000_000;

/// Maximum number of favorite reactions returned with a stats snapshot.
pub const MAX_FAVORITE_REACTIONS: i64 = 5;

/// Maximum number of favorite sounds returned with a stats snapshot.
pub const MAX_FAVORITE_SOUNDS: i64 = 5;

/// Upper bound accepted for reported latency/jitter values in milliseconds.
pub const MAX_REPORTED_STAT_MS: f64 = 60_000.0;

/// Bounds for a timed mute. Defined next to the rows they bound
/// (`db::moderation`) because the auto-moderation rules reach for the same
/// pair — a rule may not mute for longer than a moderator could by hand.
pub use crate::db::{MAX_MUTE_SECONDS, MIN_MUTE_SECONDS};

/// Bound for a voice channel's bitrate. Defined next to the configurable
/// defaults it limits (`db::voice_defaults`) so the two can never disagree;
/// the defaults themselves are read from that setting, not from a constant.
pub use crate::db::MAX_ALLOWED_VOICE_BITRATE;

/// Fewest breakout rooms a voice channel can be split into. One room is not
/// a split, it is the channel everybody is already in.
pub const MIN_BREAKOUT_ROOMS: usize = 2;

/// Most breakout rooms one split may open. Each room is a real voice channel
/// in everybody's sidebar for as long as the split lasts, so the cap is about
/// what stays legible there rather than about server cost.
pub const MAX_BREAKOUT_ROOMS: usize = 8;

/// Maximum number of ids accepted in a single reorder request (channels of
/// one category, or all categories).
pub const MAX_REORDER_IDS: usize = 200;

/// Allowed user status values broadcast to clients.
pub const USER_STATUSES: &[&str] = &["online", "away", "busy", "offline"];

/// Minimum duration in seconds for ephemeral messages.
pub const MIN_EPHEMERAL_SECONDS: i64 = 5;

/// Maximum duration in seconds for ephemeral messages.
pub const MAX_EPHEMERAL_SECONDS: i64 = 86_400;

/// Hard ceiling in bytes for a chat message's text content. A server may
/// configure a *lower* cap (`db::chat_settings`), never a higher one, so this
/// is defined alongside that setting.
pub use crate::db::MAX_MESSAGE_LENGTH;

/// Exact decoded length in bytes of a NaCl box/secretbox nonce, shared by
/// direct messages and encrypted channel messages.
pub const BOX_NONCE_BYTES: usize = 24;

/// Poly1305 authenticator bytes appended to every NaCl box ciphertext; a
/// ciphertext may exceed its plaintext limit by this much.
pub const BOX_OVERHEAD_BYTES: usize = 16;

/// Exact decoded length in bytes of a wrapped channel key: the 32-byte
/// symmetric channel secret plus the box authenticator. A wrap of any other
/// size is not a channel key, so the server refuses to store it.
pub const WRAPPED_CHANNEL_KEY_BYTES: usize = 32 + BOX_OVERHEAD_BYTES;

/// Maximum number of wrapped keys accepted in a single `put-channel-keys`
/// frame. One frame carries at most a full member roster.
pub const MAX_CHANNEL_KEY_ENTRIES: usize = 500;

/// Highest key epoch a channel may reach. Epochs only ever count up, one per
/// membership removal; a client that tries to run past this is malfunctioning.
pub const MAX_CHANNEL_KEY_EPOCH: i64 = 1_000_000;

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

/// Maximum number of wiki pages returned alongside the message hits of a
/// search. Pages accompany the message results rather than replacing them,
/// so this stays well below [`MAX_SEARCH_RESULTS`].
pub const MAX_WIKI_SEARCH_RESULTS: i64 = 20;

/// Maximum number of links accepted in a single wiki-resolve request.
pub const MAX_WIKI_RESOLVE_LINKS: usize = 50;
