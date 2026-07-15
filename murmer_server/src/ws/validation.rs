//! Validation helpers for WebSocket message parameters.

use super::constants::{
    MAX_ALLOWED_VOICE_BITRATE, MAX_EMOJI_NAME_LEN, MAX_TOPIC_LENGTH, MAX_WIKI_SLUG_LENGTH,
    MAX_WIKI_TITLE_LENGTH, MIN_EMOJI_NAME_LEN, USER_STATUSES,
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

/// Validate a wiki page slug: lowercase alphanumerics and single dashes,
/// no leading/trailing/double dash, at most [`MAX_WIKI_SLUG_LENGTH`] bytes.
/// Keeping slugs lowercase-canonical makes the per-channel UNIQUE constraint
/// case-consistent and keeps `[[links]]` unambiguous.
pub fn validate_wiki_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_WIKI_SLUG_LENGTH
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Validate a wiki page title: non-empty after trimming, within the length
/// limit and free of control characters.
pub fn validate_wiki_title(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_WIKI_TITLE_LENGTH
        && !trimmed.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_slug_accepts_canonical_forms() {
        assert!(validate_wiki_slug("getting-started"));
        assert!(validate_wiki_slug("a"));
        assert!(validate_wiki_slug("page-2"));
    }

    #[test]
    fn wiki_slug_rejects_invalid_forms() {
        assert!(!validate_wiki_slug(""));
        assert!(!validate_wiki_slug("-leading"));
        assert!(!validate_wiki_slug("trailing-"));
        assert!(!validate_wiki_slug("double--dash"));
        assert!(!validate_wiki_slug("Upper"));
        assert!(!validate_wiki_slug("under_score"));
        assert!(!validate_wiki_slug("spa ce"));
        assert!(!validate_wiki_slug(&"a".repeat(MAX_WIKI_SLUG_LENGTH + 1)));
    }

    #[test]
    fn wiki_title_limits() {
        assert!(validate_wiki_title("Getting Started"));
        assert!(!validate_wiki_title("   "));
        assert!(!validate_wiki_title("bad\u{7}title"));
        assert!(!validate_wiki_title(&"x".repeat(MAX_WIKI_TITLE_LENGTH + 1)));
    }
}
