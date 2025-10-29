//! Validation helpers for WebSocket message parameters.

use super::constants::{MAX_ALLOWED_VOICE_BITRATE, USER_STATUSES};

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

/// Validate and convert a bitrate value to i32.
///
/// Returns `None` if the value is out of range or cannot be converted.
pub fn validate_bitrate(value: i64) -> Option<i32> {
    if value <= 0 || value > MAX_ALLOWED_VOICE_BITRATE as i64 {
        return None;
    }
    i32::try_from(value).ok()
}

