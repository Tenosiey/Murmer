//! Screen share settings: the server-wide bitrate cap.
//!
//! The cap lives in the generic `server_settings` key-value table (shared
//! with the stats toggle and server identity), so no schema changes are
//! needed. Screen share video travels peer-to-peer, so the cap is a policy
//! knob for member bandwidth set by Owners/Admins — the server itself never
//! carries the stream. Absent or `0` means "no cap".

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};

/// `server_settings` key for the screen share bitrate cap in bits per second.
const SCREENSHARE_MAX_BITRATE_KEY: &str = "screenshare_max_bitrate";

/// The configured screen share bitrate cap in bits per second, or `None`
/// when no cap is set.
pub async fn screenshare_max_bitrate(db: &Db) -> Result<Option<u64>, DbError> {
    db.call_db(|conn| {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM server_settings WHERE key = ?1",
                params![SCREENSHARE_MAX_BITRATE_KEY],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.and_then(|v| v.parse::<u64>().ok()).filter(|&v| v > 0))
    })
    .await
}

/// Set the screen share bitrate cap (Owner/Admin action, checked by caller).
/// `None` removes the cap.
pub async fn set_screenshare_max_bitrate(db: &Db, bitrate: Option<u64>) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute(
            "INSERT INTO server_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![
                SCREENSHARE_MAX_BITRATE_KEY,
                bitrate.unwrap_or(0).to_string()
            ],
        )?;
        Ok(())
    })
    .await
}
