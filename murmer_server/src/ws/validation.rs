//! Validation helpers for WebSocket message parameters.

use super::constants::{
    MAX_ALLOWED_VOICE_BITRATE, MAX_EMOJI_NAME_LEN, MAX_TOPIC_LENGTH, MIN_EMOJI_NAME_LEN,
    USER_STATUSES,
};

/// Normalize a user status string to a valid status value.
///
/// Returns `None` if the input doesn't match any valid status.
pub fn normalize_status(value: &str) -> Option<&'static str> {
    USER_STATUSES
        .iter()
        .copied()
        .find(|status| status.eq_ignore_ascii_case(value))
}

/// Validate that a voice quality string is acceptable.
///
/// Quality strings must be non-empty, at most 32 characters,
/// and contain only alphanumeric characters, dashes, underscores, or spaces.
pub fn validate_voice_quality(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 32
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ')
}

/// Validate a channel topic/description.
///
/// Topics may be empty (clearing the topic) but must stay within the length
/// limit and must not contain control characters.
pub fn validate_channel_topic(value: &str) -> bool {
    value.len() <= MAX_TOPIC_LENGTH && !value.chars().any(char::is_control)
}

/// Validate a custom emoji name: lowercase alphanumerics and underscores,
/// 2 to 32 characters (`^[a-z0-9_]{2,32}$` without a regex dependency).
pub fn validate_emoji_name(value: &str) -> bool {
    value.len() >= MIN_EMOJI_NAME_LEN
        && value.len() <= MAX_EMOJI_NAME_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether a reaction key is a custom emoji shortcode of the form `:name:`.
pub fn is_emoji_shortcode(value: &str) -> bool {
    value
        .strip_prefix(':')
        .and_then(|rest| rest.strip_suffix(':'))
        .is_some_and(validate_emoji_name)
}

/// Validate and convert a bitrate value to i32.
///
/// Returns `None` if the value is out of range or cannot be converted.
pub fn validate_bitrate(value: i64) -> Option<i32> {
    if value <= 0 || value > MAX_ALLOWED_VOICE_BITRATE as i64 {
        return None;
    }
    i32::try_from(value).ok()
}
