//! WebSocket error message constants.
//!
//! Pre-serialized `{"type":"error","message":"<code>"}` frames, sent to a
//! single client via [`crate::ws::helpers::send_error`]. The client maps each
//! code to user-facing text in `murmer_client/src/lib/errors.ts` — keep the
//! two files in sync when adding codes.

/// Client attempted to send a message without authenticating first.
pub const UNAUTHENTICATED: &str = r#"{"type":"error","message":"unauthenticated"}"#;

/// The provided server password did not match.
pub const INVALID_PASSWORD: &str = r#"{"type":"error","message":"invalid-password"}"#;

/// Authentication rate limit exceeded.
pub const AUTH_RATE_LIMIT: &str = r#"{"type":"error","message":"auth-rate-limit"}"#;

/// Timestamp is outside the acceptable window.
pub const INVALID_TIMESTAMP: &str = r#"{"type":"error","message":"invalid-timestamp"}"#;

/// Nonce has already been used (replay attack detected).
pub const REPLAY_ATTACK: &str = r#"{"type":"error","message":"replay-attack"}"#;

/// Signature verification failed.
pub const INVALID_SIGNATURE: &str = r#"{"type":"error","message":"invalid-signature"}"#;

/// Signature format is invalid.
pub const INVALID_SIGNATURE_FORMAT: &str =
    r#"{"type":"error","message":"invalid-signature-format"}"#;

/// Public key format is invalid.
pub const INVALID_PUBLIC_KEY: &str = r#"{"type":"error","message":"invalid-public-key"}"#;

/// Public key has incorrect length.
pub const INVALID_KEY_LENGTH: &str = r#"{"type":"error","message":"invalid-key-length"}"#;

/// Base64 encoding is invalid.
pub const INVALID_ENCODING: &str = r#"{"type":"error","message":"invalid-encoding"}"#;

/// Username validation failed.
pub const INVALID_USERNAME: &str = r#"{"type":"error","message":"invalid-username"}"#;

/// Username is bound to a different public key for this server session.
pub const USERNAME_TAKEN: &str = r#"{"type":"error","message":"username-taken"}"#;

/// Channel name validation failed.
pub const INVALID_CHANNEL_NAME: &str = r#"{"type":"error","message":"invalid-channel-name"}"#;

/// User lacks permission to manage channels.
pub const CHANNEL_PERMISSION_DENIED: &str =
    r#"{"type":"error","message":"channel-permission-denied"}"#;

/// Failed to create channel in database.
pub const CHANNEL_CREATION_FAILED: &str = r#"{"type":"error","message":"channel-creation-failed"}"#;

/// Failed to delete channel from database.
pub const CHANNEL_DELETION_FAILED: &str = r#"{"type":"error","message":"channel-deletion-failed"}"#;

/// Cannot delete the general channel.
pub const CANNOT_DELETE_GENERAL: &str = r#"{"type":"error","message":"cannot-delete-general"}"#;

/// Message rate limit exceeded.
pub const MESSAGE_RATE_LIMIT: &str = r#"{"type":"error","message":"message-rate-limit"}"#;

/// Message content exceeds the maximum allowed length.
pub const MESSAGE_TOO_LONG: &str = r#"{"type":"error","message":"message-too-long"}"#;

/// Voice quality parameter is invalid.
pub const INVALID_VOICE_QUALITY: &str = r#"{"type":"error","message":"invalid-voice-quality"}"#;

/// Voice bitrate parameter is invalid.
pub const INVALID_VOICE_BITRATE: &str = r#"{"type":"error","message":"invalid-voice-bitrate"}"#;

/// Voice channel does not exist.
pub const UNKNOWN_VOICE_CHANNEL: &str = r#"{"type":"error","message":"unknown-voice-channel"}"#;

/// Failed to update voice channel configuration.
pub const VOICE_CHANNEL_UPDATE_FAILED: &str =
    r#"{"type":"error","message":"voice-channel-update-failed"}"#;

/// User lacks permission to manage roles.
pub const ROLE_PERMISSION_DENIED: &str = r#"{"type":"error","message":"role-permission-denied"}"#;

/// Target user for role change is not connected.
pub const ROLE_TARGET_NOT_FOUND: &str = r#"{"type":"error","message":"role-target-not-found"}"#;

/// Failed to update role in database.
pub const ROLE_UPDATE_FAILED: &str = r#"{"type":"error","message":"role-update-failed"}"#;

/// Category name validation failed.
pub const INVALID_CATEGORY_NAME: &str = r#"{"type":"error","message":"invalid-category-name"}"#;

/// Failed to create category in database.
pub const CATEGORY_CREATION_FAILED: &str =
    r#"{"type":"error","message":"category-creation-failed"}"#;

/// Failed to rename category in database.
pub const CATEGORY_RENAME_FAILED: &str = r#"{"type":"error","message":"category-rename-failed"}"#;

/// Failed to delete category from database.
pub const CATEGORY_DELETION_FAILED: &str =
    r#"{"type":"error","message":"category-deletion-failed"}"#;

/// The referenced category does not exist.
pub const UNKNOWN_CATEGORY: &str = r#"{"type":"error","message":"unknown-category"}"#;

/// Failed to move channel to category.
pub const CHANNEL_MOVE_FAILED: &str = r#"{"type":"error","message":"channel-move-failed"}"#;

/// Channel topic validation failed.
pub const INVALID_CHANNEL_TOPIC: &str = r#"{"type":"error","message":"invalid-channel-topic"}"#;

/// The referenced channel does not exist.
pub const UNKNOWN_CHANNEL: &str = r#"{"type":"error","message":"unknown-channel"}"#;

/// Failed to update the channel topic in the database.
pub const TOPIC_UPDATE_FAILED: &str = r#"{"type":"error","message":"topic-update-failed"}"#;

/// User lacks permission for moderation actions (kick, ban, mute).
pub const MODERATION_PERMISSION_DENIED: &str =
    r#"{"type":"error","message":"moderation-permission-denied"}"#;

/// Moderation target could not be resolved to a connected user.
pub const MODERATION_TARGET_NOT_FOUND: &str =
    r#"{"type":"error","message":"moderation-target-not-found"}"#;

/// Moderation target outranks or equals the requester and is protected.
pub const MODERATION_TARGET_PROTECTED: &str =
    r#"{"type":"error","message":"moderation-target-protected"}"#;

/// Moderation actions cannot target the requester themselves.
pub const CANNOT_MODERATE_SELF: &str = r#"{"type":"error","message":"cannot-moderate-self"}"#;

/// Failed to persist a moderation action.
pub const MODERATION_FAILED: &str = r#"{"type":"error","message":"moderation-failed"}"#;

/// Connection rejected because the user is banned.
pub const BANNED: &str = r#"{"type":"error","message":"banned"}"#;

/// Direct message target is not a known user on this server.
pub const DM_TARGET_NOT_FOUND: &str = r#"{"type":"error","message":"dm-target-not-found"}"#;

/// Direct messages cannot be sent to oneself.
pub const CANNOT_DM_SELF: &str = r#"{"type":"error","message":"cannot-dm-self"}"#;

/// Failed to persist a direct message.
pub const DM_SEND_FAILED: &str = r#"{"type":"error","message":"dm-send-failed"}"#;

/// Failed to load a direct message conversation.
pub const DM_HISTORY_FAILED: &str = r#"{"type":"error","message":"dm-history-failed"}"#;

/// Pin target does not exist (unknown or deleted message).
pub const PIN_TARGET_NOT_FOUND: &str = r#"{"type":"error","message":"pin-target-not-found"}"#;

/// The channel already carries the maximum number of pins.
pub const PIN_LIMIT_REACHED: &str = r#"{"type":"error","message":"pin-limit-reached"}"#;

/// Failed to persist a pin change.
pub const PIN_FAILED: &str = r#"{"type":"error","message":"pin-failed"}"#;

/// Request requires an authenticated user name (presence not yet processed).
pub const NOT_AUTHENTICATED: &str = r#"{"type":"error","message":"not-authenticated"}"#;

/// Status update carried a missing or unknown status value.
pub const INVALID_STATUS: &str = r#"{"type":"error","message":"invalid-status"}"#;

/// Sender is muted and may not send messages.
pub const MUTED: &str = r#"{"type":"error","message":"muted"}"#;

/// Message ID is missing, malformed or out of range.
pub const INVALID_MESSAGE_ID: &str = r#"{"type":"error","message":"invalid-message-id"}"#;

/// The referenced message does not exist.
pub const MESSAGE_NOT_FOUND: &str = r#"{"type":"error","message":"message-not-found"}"#;

/// The referenced message belongs to a different channel.
pub const MESSAGE_WRONG_CHANNEL: &str = r#"{"type":"error","message":"message-wrong-channel"}"#;

/// Requester may not delete or edit this message.
pub const MESSAGE_PERMISSION_DENIED: &str =
    r#"{"type":"error","message":"message-permission-denied"}"#;

/// Failed to delete a message.
pub const MESSAGE_DELETE_FAILED: &str = r#"{"type":"error","message":"message-delete-failed"}"#;

/// Failed to edit a message.
pub const MESSAGE_EDIT_FAILED: &str = r#"{"type":"error","message":"message-edit-failed"}"#;

/// Replacement message text is missing or invalid.
pub const INVALID_MESSAGE_TEXT: &str = r#"{"type":"error","message":"invalid-message-text"}"#;

/// Reaction action was neither `add` nor `remove`.
pub const INVALID_REACTION_ACTION: &str = r#"{"type":"error","message":"invalid-reaction-action"}"#;

/// Reaction emoji is missing or malformed.
pub const INVALID_EMOJI: &str = r#"{"type":"error","message":"invalid-emoji"}"#;

/// Failed to persist or load reactions.
pub const REACTION_FAILED: &str = r#"{"type":"error","message":"reaction-failed"}"#;

/// The message a reply targets no longer exists.
pub const REPLY_TARGET_NOT_FOUND: &str = r#"{"type":"error","message":"reply-target-not-found"}"#;

/// Failed to load a thread.
pub const THREAD_LOAD_FAILED: &str = r#"{"type":"error","message":"thread-load-failed"}"#;

/// User lacks permission to manage custom server emojis.
pub const EMOJI_PERMISSION_DENIED: &str = r#"{"type":"error","message":"emoji-permission-denied"}"#;

/// Custom emoji name failed validation.
pub const INVALID_EMOJI_NAME: &str = r#"{"type":"error","message":"invalid-emoji-name"}"#;

/// Custom emoji URL does not point at a valid uploaded image.
pub const INVALID_EMOJI_URL: &str = r#"{"type":"error","message":"invalid-emoji-url"}"#;

/// A custom emoji with this name already exists.
pub const EMOJI_NAME_TAKEN: &str = r#"{"type":"error","message":"emoji-name-taken"}"#;

/// The server has reached its custom emoji limit.
pub const EMOJI_LIMIT_REACHED: &str = r#"{"type":"error","message":"emoji-limit-reached"}"#;

/// Failed to persist a custom emoji change.
pub const EMOJI_UPDATE_FAILED: &str = r#"{"type":"error","message":"emoji-update-failed"}"#;

/// The referenced custom emoji does not exist.
pub const EMOJI_NOT_FOUND: &str = r#"{"type":"error","message":"emoji-not-found"}"#;

/// Bot presence frame did not include a token.
pub const MISSING_BOT_TOKEN: &str = r#"{"type":"error","message":"missing-bot-token"}"#;

/// Bot token is unknown or the bot is deactivated.
pub const INVALID_BOT_TOKEN: &str = r#"{"type":"error","message":"invalid-bot-token"}"#;
