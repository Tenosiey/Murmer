//! Server-wide chat policy: slow mode, the message length cap and the
//! profanity filter.
//!
//! Like the upload policy these live in the generic `server_settings`
//! key-value table, so adding them needed no schema change. Owners/Admins
//! edit them from the Server Dashboard (Moderation tab); the values are
//! cached in [`crate::AppState`] because every chat message consults them,
//! and the cache is refreshed in the same handler that writes them.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};

/// `server_settings` key for the per-user slow mode interval in seconds.
const SLOW_MODE_KEY: &str = "slow_mode_seconds";

/// `server_settings` key for the per-message character cap.
const MAX_MESSAGE_LENGTH_KEY: &str = "max_message_length";

/// `server_settings` key for the profanity filter toggle.
const PROFANITY_FILTER_KEY: &str = "profanity_filter_enabled";

/// `server_settings` key for the newline-separated filtered word list.
const PROFANITY_WORDS_KEY: &str = "profanity_words";

/// Largest slow mode interval an operator may configure (6 hours).
pub const MAX_SLOW_MODE_SECONDS: u64 = 6 * 60 * 60;

/// Hard ceiling for a chat message, in bytes. A configured cap may only
/// lower this — never raise it — so the storage cost of one message stays
/// bounded no matter what a setting says.
pub const MAX_MESSAGE_LENGTH: usize = 4000;

/// Smallest message cap an operator may configure.
pub const MIN_CONFIGURABLE_MESSAGE_LENGTH: usize = 10;

/// Maximum number of words the profanity filter may hold.
pub const MAX_PROFANITY_WORDS: usize = 200;

/// Maximum length in bytes of a single filtered word.
pub const MAX_PROFANITY_WORD_LEN: usize = 32;

/// The server-wide chat policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSettings {
    /// Seconds a member must wait between messages; 0 disables slow mode.
    pub slow_mode_seconds: u64,
    /// Largest accepted message length in bytes.
    pub max_message_length: usize,
    /// Whether filtered words are masked in new messages.
    pub profanity_filter: bool,
    /// Lowercase words the filter masks. Only disclosed to managers.
    pub profanity_words: Vec<String>,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            slow_mode_seconds: 0,
            max_message_length: MAX_MESSAGE_LENGTH,
            profanity_filter: false,
            profanity_words: Vec::new(),
        }
    }
}

/// Normalize a filtered word list: lowercased, trimmed, de-duplicated and
/// bounded in both count and length. Applied on write *and* on read, so a row
/// written by an older build can never hand the filter something unbounded.
pub fn normalize_profanity_words<I, S>(words: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut normalized: Vec<String> = Vec::new();
    for word in words {
        let word = word.as_ref().trim().to_lowercase();
        if word.is_empty()
            || word.len() > MAX_PROFANITY_WORD_LEN
            || word.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            continue;
        }
        if !normalized.iter().any(|existing| existing == &word) {
            normalized.push(word);
        }
        if normalized.len() == MAX_PROFANITY_WORDS {
            break;
        }
    }
    normalized
}

/// Clamp settings to the bounds this build accepts. The client's own bounds
/// are cosmetic; this is the one that decides.
pub fn clamp_chat_settings(settings: ChatSettings) -> ChatSettings {
    ChatSettings {
        slow_mode_seconds: settings.slow_mode_seconds.min(MAX_SLOW_MODE_SECONDS),
        max_message_length: settings
            .max_message_length
            .clamp(MIN_CONFIGURABLE_MESSAGE_LENGTH, MAX_MESSAGE_LENGTH),
        profanity_filter: settings.profanity_filter,
        profanity_words: normalize_profanity_words(settings.profanity_words),
    }
}

/// Load the chat policy, falling back to the defaults for missing or
/// unparseable settings.
pub async fn chat_settings(db: &Db) -> Result<ChatSettings, DbError> {
    db.call_db(|conn| {
        let read = |key: &str| -> rusqlite::Result<Option<String>> {
            conn.query_row(
                "SELECT value FROM server_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
        };

        let mut settings = ChatSettings::default();
        if let Some(seconds) = read(SLOW_MODE_KEY)?.and_then(|v| v.parse::<u64>().ok()) {
            settings.slow_mode_seconds = seconds;
        }
        if let Some(length) = read(MAX_MESSAGE_LENGTH_KEY)?.and_then(|v| v.parse::<usize>().ok()) {
            settings.max_message_length = length;
        }
        if let Some(raw) = read(PROFANITY_FILTER_KEY)? {
            settings.profanity_filter = raw == "1";
        }
        if let Some(raw) = read(PROFANITY_WORDS_KEY)? {
            settings.profanity_words = raw.split('\n').map(str::to_string).collect();
        }

        Ok(clamp_chat_settings(settings))
    })
    .await
}

/// Store the chat policy (Owner/Admin action, validated by the caller).
/// Returns the clamped settings that were actually written.
pub async fn set_chat_settings(db: &Db, settings: &ChatSettings) -> Result<ChatSettings, DbError> {
    let stored = clamp_chat_settings(settings.clone());
    let values = [
        (SLOW_MODE_KEY, stored.slow_mode_seconds.to_string()),
        (
            MAX_MESSAGE_LENGTH_KEY,
            stored.max_message_length.to_string(),
        ),
        (
            PROFANITY_FILTER_KEY,
            if stored.profanity_filter { "1" } else { "0" }.to_string(),
        ),
        (PROFANITY_WORDS_KEY, stored.profanity_words.join("\n")),
    ];
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        for (key, value) in values {
            tx.execute(
                "INSERT INTO server_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await?;
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_lowercased_deduped_and_bounded() {
        let words = normalize_profanity_words(["Damn", "damn", " HECK ", "", "two words", "x"]);
        assert_eq!(words, vec!["damn", "heck", "x"]);

        let many: Vec<String> = (0..MAX_PROFANITY_WORDS + 50)
            .map(|i| format!("word{i}"))
            .collect();
        assert_eq!(normalize_profanity_words(many).len(), MAX_PROFANITY_WORDS);

        assert!(normalize_profanity_words(["x".repeat(MAX_PROFANITY_WORD_LEN + 1)]).is_empty());
    }

    #[test]
    fn settings_are_clamped_to_build_bounds() {
        let clamped = clamp_chat_settings(ChatSettings {
            slow_mode_seconds: MAX_SLOW_MODE_SECONDS * 10,
            // A setting may only ever lower the hard message cap.
            max_message_length: MAX_MESSAGE_LENGTH * 2,
            profanity_filter: true,
            profanity_words: vec!["Damn".into()],
        });
        assert_eq!(clamped.slow_mode_seconds, MAX_SLOW_MODE_SECONDS);
        assert_eq!(clamped.max_message_length, MAX_MESSAGE_LENGTH);
        assert_eq!(clamped.profanity_words, vec!["damn"]);

        let tiny = clamp_chat_settings(ChatSettings {
            max_message_length: 1,
            ..ChatSettings::default()
        });
        assert_eq!(tiny.max_message_length, MIN_CONFIGURABLE_MESSAGE_LENGTH);
    }
}
