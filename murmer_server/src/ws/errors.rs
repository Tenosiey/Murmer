//! WebSocket error message constants.
//!
//! These constants provide consistent error responses to clients.

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

/// Voice quality parameter is invalid.
pub const INVALID_VOICE_QUALITY: &str = r#"{"type":"error","message":"invalid-voice-quality"}"#;

/// Voice bitrate parameter is invalid.
pub const INVALID_VOICE_BITRATE: &str = r#"{"type":"error","message":"invalid-voice-bitrate"}"#;

/// Voice channel does not exist.
pub const UNKNOWN_VOICE_CHANNEL: &str = r#"{"type":"error","message":"unknown-voice-channel"}"#;

/// Failed to update voice channel configuration.
pub const VOICE_CHANNEL_UPDATE_FAILED: &str =
    r#"{"type":"error","message":"voice-channel-update-failed"}"#;
