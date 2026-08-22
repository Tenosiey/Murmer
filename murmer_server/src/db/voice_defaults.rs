//! Voice defaults: the quality preset and bitrate new voice channels start
//! with.
//!
//! Like the screen share cap these live in the generic `server_settings`
//! table. They are pure defaults: an existing channel keeps whatever it was
//! created with, and a client that names a quality/bitrate itself still wins.
//! A `None` bitrate means the channel is uncompressed ("lossless").

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};

/// `server_settings` key for the default voice quality label.
const VOICE_DEFAULT_QUALITY_KEY: &str = "voice_default_quality";

/// `server_settings` key for the default voice bitrate in bits per second.
/// Stored as `0` for "no bitrate" (lossless).
const VOICE_DEFAULT_BITRATE_KEY: &str = "voice_default_bitrate";

/// Quality label assigned to new voice channels when nothing is configured.
pub const DEFAULT_VOICE_QUALITY: &str = "standard";

/// Bitrate assigned to new voice channels when nothing is configured.
pub const DEFAULT_VOICE_BITRATE: i32 = 64_000;

/// Upper bound to reject unreasonable bitrate configuration values.
pub const MAX_ALLOWED_VOICE_BITRATE: i32 = 320_000;

/// The quality/bitrate pair new voice channels are created with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDefaults {
    pub quality: String,
    /// Bits per second, or `None` for an uncompressed channel.
    pub bitrate: Option<i32>,
}

impl Default for VoiceDefaults {
    fn default() -> Self {
        Self {
            quality: DEFAULT_VOICE_QUALITY.to_string(),
            bitrate: Some(DEFAULT_VOICE_BITRATE),
        }
    }
}

/// Load the voice defaults, falling back to the built-in ones for missing or
/// unparseable settings.
pub async fn voice_defaults(db: &Db) -> Result<VoiceDefaults, DbError> {
    db.call_db(|conn| {
        let read = |key: &str| -> rusqlite::Result<Option<String>> {
            conn.query_row(
                "SELECT value FROM server_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
        };

        let mut defaults = VoiceDefaults::default();
        if let Some(quality) = read(VOICE_DEFAULT_QUALITY_KEY)?.filter(|q| !q.trim().is_empty()) {
            defaults.quality = quality;
        }
        if let Some(bitrate) = read(VOICE_DEFAULT_BITRATE_KEY)?.and_then(|v| v.parse::<i32>().ok())
        {
            defaults.bitrate = (bitrate > 0).then(|| bitrate.min(MAX_ALLOWED_VOICE_BITRATE));
        }
        Ok(defaults)
    })
    .await
}

/// Store the voice defaults (Owner/Admin action, validated by the caller).
pub async fn set_voice_defaults(db: &Db, defaults: &VoiceDefaults) -> Result<(), DbError> {
    let quality = defaults.quality.clone();
    let bitrate = defaults.bitrate.unwrap_or(0).to_string();
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        for (key, value) in [
            (VOICE_DEFAULT_QUALITY_KEY, quality),
            (VOICE_DEFAULT_BITRATE_KEY, bitrate),
        ] {
            tx.execute(
                "INSERT INTO server_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await
}
