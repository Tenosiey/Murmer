//! Validation helpers for WebSocket message parameters.

use super::constants::{
    MAX_ABOUT_LENGTH, MAX_ALLOWED_VOICE_BITRATE, MAX_DISPLAY_NAME_LENGTH, MAX_EMOJI_NAME_LEN,
    MAX_ROLE_NAME_LENGTH, MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_NAME_LENGTH,
    MAX_SOUND_NAME_LEN, MAX_TOPIC_LENGTH, MAX_WELCOME_MESSAGE_LENGTH, MAX_WIKI_SLUG_LENGTH,
    MAX_WIKI_TITLE_LENGTH, MIN_EMOJI_NAME_LEN, MIN_SOUND_NAME_LEN, UPLOAD_IMAGE_EXTENSIONS,
    UPLOAD_SOUND_EXTENSIONS, USER_STATUSES,
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

/// Validate the server display name: may be empty (unset), otherwise within
/// the length limit and free of control characters.
pub fn validate_server_name(value: &str) -> bool {
    value.len() <= MAX_SERVER_NAME_LENGTH && !value.chars().any(char::is_control)
}

/// Validate the server description: may be empty (unset), within the length
/// limit; newlines are allowed, other control characters are not.
pub fn validate_server_description(value: &str) -> bool {
    value.len() <= MAX_SERVER_DESCRIPTION_LENGTH
        && !value.chars().any(|c| c.is_control() && c != '\n')
}

/// Validate the welcome message: may be empty (disabled), within the length
/// limit; newlines are allowed, other control characters are not.
pub fn validate_welcome_message(value: &str) -> bool {
    value.len() <= MAX_WELCOME_MESSAGE_LENGTH && !value.chars().any(|c| c.is_control() && c != '\n')
}

/// Validate a user's profile display name: may be empty (falling back to the
/// account name), otherwise trimmed, within the length limit and free of
/// control characters. Counted in characters rather than bytes so a name of
/// non-ASCII glyphs is not cut shorter than an ASCII one.
///
/// Deliberately *not* unique: the display name is decoration on top of the
/// account name, which stays the identity everywhere it matters (auth, roles,
/// DMs, mentions) and is shown alongside it on the profile.
pub fn validate_display_name(value: &str) -> bool {
    value.chars().count() <= MAX_DISPLAY_NAME_LENGTH
        && value == value.trim()
        && !value.chars().any(char::is_control)
}

/// Validate a user's profile "about" text: may be empty, within the length
/// limit; newlines are allowed, other control characters are not.
pub fn validate_about(value: &str) -> bool {
    value.chars().count() <= MAX_ABOUT_LENGTH && !value.chars().any(|c| c.is_control() && c != '\n')
}

/// Validate a soundboard sound's display name. Unlike emoji shortcodes these
/// are shown as plain text, so spaces and mixed case are fine; control
/// characters are not, and the name must already be trimmed.
pub fn validate_sound_name(value: &str) -> bool {
    value.len() >= MIN_SOUND_NAME_LEN
        && value.len() <= MAX_SOUND_NAME_LEN
        && value == value.trim()
        && !value.chars().any(char::is_control)
}

/// Extract the file key from an upload URL of the form `/files/<key>`,
/// accepting only the given extensions. Rejects anything that could escape the
/// upload directory: the key must be a single non-empty path segment.
fn upload_key_with_extensions<'a>(url: &'a str, allowed: &[&str]) -> Option<&'a str> {
    let key = url.strip_prefix("/files/")?;
    if key.is_empty()
        || key.contains('/')
        || key.contains('\\')
        || key.contains("..")
        || key.chars().any(char::is_control)
    {
        return None;
    }
    let (_, ext) = key.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    if !allowed.contains(&ext.as_str()) {
        return None;
    }
    Some(key)
}

/// Extract the file key from an uploaded-image URL of the form
/// `/files/<key>`, restricted to the [`UPLOAD_IMAGE_EXTENSIONS`] safe-list.
pub fn upload_key_from_url(url: &str) -> Option<&str> {
    upload_key_with_extensions(url, UPLOAD_IMAGE_EXTENSIONS)
}

/// Extract the file key from an uploaded-sound URL of the form
/// `/files/<key>`, restricted to the [`UPLOAD_SOUND_EXTENSIONS`] safe-list.
pub fn sound_key_from_url(url: &str) -> Option<&str> {
    upload_key_with_extensions(url, UPLOAD_SOUND_EXTENSIONS)
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

/// Validate a role display name: non-empty after trimming, within the length
/// limit and free of control characters. Uniqueness is enforced by the
/// database's case-insensitive UNIQUE constraint.
pub fn validate_role_name(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= MAX_ROLE_NAME_LENGTH
        && !trimmed.chars().any(char::is_control)
}

/// Validate a role color: a hex string like `#rgb`, `#rrggbb` or `#rrggbbaa`
/// (3 to 8 hex digits). Mirrors the client's `HEX_COLOR_RE`.
pub fn validate_role_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    (3..=8).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_name_limits() {
        assert!(validate_role_name("Dude"));
        assert!(validate_role_name("@everyone"));
        assert!(!validate_role_name("   "));
        assert!(!validate_role_name("bad\nname"));
        assert!(!validate_role_name(&"x".repeat(MAX_ROLE_NAME_LENGTH + 1)));
    }

    #[test]
    fn role_color_forms() {
        assert!(validate_role_color("#fff"));
        assert!(validate_role_color("#3b82f6"));
        assert!(validate_role_color("#3b82f6ff"));
        assert!(!validate_role_color("3b82f6"));
        assert!(!validate_role_color("#xyz"));
        assert!(!validate_role_color("#12"));
    }

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
    fn server_identity_text_limits() {
        assert!(validate_server_name(""));
        assert!(validate_server_name("My Murmer Server"));
        assert!(!validate_server_name("bad\nname"));
        assert!(!validate_server_name(
            &"x".repeat(MAX_SERVER_NAME_LENGTH + 1)
        ));

        assert!(validate_server_description(
            "What this server is about.\nSecond line."
        ));
        assert!(!validate_server_description("bad\u{7}description"));
        assert!(!validate_server_description(
            &"x".repeat(MAX_SERVER_DESCRIPTION_LENGTH + 1)
        ));

        assert!(validate_welcome_message("Welcome!\nEnjoy your stay."));
        assert!(!validate_welcome_message("bad\u{7}welcome"));
        assert!(!validate_welcome_message(
            &"x".repeat(MAX_WELCOME_MESSAGE_LENGTH + 1)
        ));
    }

    #[test]
    fn upload_key_extraction() {
        assert_eq!(upload_key_from_url("/files/icon.png"), Some("icon.png"));
        assert_eq!(upload_key_from_url("/files/a.b.webp"), Some("a.b.webp"));
        assert_eq!(upload_key_from_url("/files/../etc/passwd"), None);
        assert_eq!(upload_key_from_url("/files/sub/dir.png"), None);
        assert_eq!(upload_key_from_url("/files/script.svg"), None);
        assert_eq!(upload_key_from_url("/files/"), None);
        assert_eq!(upload_key_from_url("https://evil.example/x.png"), None);
    }

    #[test]
    fn sound_key_extraction() {
        assert_eq!(sound_key_from_url("/files/horn.mp3"), Some("horn.mp3"));
        assert_eq!(sound_key_from_url("/files/a.b.ogg"), Some("a.b.ogg"));
        // Images and sounds must not be interchangeable in either direction.
        assert_eq!(sound_key_from_url("/files/icon.png"), None);
        assert_eq!(upload_key_from_url("/files/horn.mp3"), None);
        assert_eq!(sound_key_from_url("/files/../etc/passwd"), None);
        assert_eq!(sound_key_from_url("/files/sub/dir.mp3"), None);
        assert_eq!(sound_key_from_url("/files/script.svg"), None);
        assert_eq!(sound_key_from_url("https://evil.example/x.mp3"), None);
    }

    #[test]
    fn sound_name_limits() {
        assert!(validate_sound_name("Air Horn"));
        assert!(validate_sound_name("wow"));
        assert!(!validate_sound_name("x"));
        assert!(!validate_sound_name(" padded "));
        assert!(!validate_sound_name("bad\u{7}name"));
        assert!(!validate_sound_name(&"x".repeat(MAX_SOUND_NAME_LEN + 1)));
    }

    #[test]
    fn wiki_title_limits() {
        assert!(validate_wiki_title("Getting Started"));
        assert!(!validate_wiki_title("   "));
        assert!(!validate_wiki_title("bad\u{7}title"));
        assert!(!validate_wiki_title(&"x".repeat(MAX_WIKI_TITLE_LENGTH + 1)));
    }
}
