//! Lifetime user statistics with double opt-in privacy gating.
//!
//! Murmer is privacy focused, so activity counters are only ever written when
//! BOTH switches are on:
//! - the server-wide toggle (`server_settings` key `stats_enabled`, controlled
//!   by Owners/Admins), and
//! - the individual user's own opt-in (`user_stats_opt_in`).
//!
//! The gate is enforced inside [`record_user_stats`] in the same database
//! call that performs the increments, so no handler can accidentally bypass
//! it. Counters are lifetime totals: removing a reaction or deleting a
//! message later does not subtract from them. Users can purge their own
//! stats at any time via [`purge_user_stats`].

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError, NOW_UTC};

/// A single counter in the `user_stats` table.
///
/// The enum-to-column mapping keeps the SQL built from increments free of
/// any user-supplied strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stat {
    MessagesSent,
    MessageChars,
    MessageBytes,
    /// Tracked as a running maximum, not a sum.
    LongestMessageChars,
    ImagesSent,
    GifsSent,
    FilesSent,
    UploadBytes,
    LinksShared,
    RepliesSent,
    MentionsSent,
    DmsSent,
    ReactionsGiven,
    ReactionsReceived,
    MessagesEdited,
    MessagesDeleted,
    PinsAdded,
    VoiceSeconds,
    VoiceSessions,
    ScreenshareSeconds,
}

impl Stat {
    /// Column name in `user_stats`.
    fn column(self) -> &'static str {
        match self {
            Stat::MessagesSent => "messages_sent",
            Stat::MessageChars => "message_chars",
            Stat::MessageBytes => "message_bytes",
            Stat::LongestMessageChars => "longest_message_chars",
            Stat::ImagesSent => "images_sent",
            Stat::GifsSent => "gifs_sent",
            Stat::FilesSent => "files_sent",
            Stat::UploadBytes => "upload_bytes",
            Stat::LinksShared => "links_shared",
            Stat::RepliesSent => "replies_sent",
            Stat::MentionsSent => "mentions_sent",
            Stat::DmsSent => "dms_sent",
            Stat::ReactionsGiven => "reactions_given",
            Stat::ReactionsReceived => "reactions_received",
            Stat::MessagesEdited => "messages_edited",
            Stat::MessagesDeleted => "messages_deleted",
            Stat::PinsAdded => "pins_added",
            Stat::VoiceSeconds => "voice_seconds",
            Stat::VoiceSessions => "voice_sessions",
            Stat::ScreenshareSeconds => "screenshare_seconds",
        }
    }
}

/// Snapshot of one user's lifetime counters.
#[derive(Clone, Debug, Default)]
pub struct UserStatsRecord {
    pub messages_sent: i64,
    pub message_chars: i64,
    pub message_bytes: i64,
    pub longest_message_chars: i64,
    pub images_sent: i64,
    pub gifs_sent: i64,
    pub files_sent: i64,
    pub upload_bytes: i64,
    pub links_shared: i64,
    pub replies_sent: i64,
    pub mentions_sent: i64,
    pub dms_sent: i64,
    pub reactions_given: i64,
    pub reactions_received: i64,
    pub messages_edited: i64,
    pub messages_deleted: i64,
    pub pins_added: i64,
    pub voice_seconds: i64,
    pub voice_sessions: i64,
    pub screenshare_seconds: i64,
    pub created_at: Option<String>,
}

/// Create the stats tables. Called from [`super::run_schema`].
pub(super) fn stats_schema() -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS server_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS user_stats_opt_in (
    user_name TEXT PRIMARY KEY,
    opted_in INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
CREATE TABLE IF NOT EXISTS user_stats (
    user_name TEXT PRIMARY KEY,
    messages_sent INTEGER NOT NULL DEFAULT 0,
    message_chars INTEGER NOT NULL DEFAULT 0,
    message_bytes INTEGER NOT NULL DEFAULT 0,
    longest_message_chars INTEGER NOT NULL DEFAULT 0,
    images_sent INTEGER NOT NULL DEFAULT 0,
    gifs_sent INTEGER NOT NULL DEFAULT 0,
    files_sent INTEGER NOT NULL DEFAULT 0,
    upload_bytes INTEGER NOT NULL DEFAULT 0,
    links_shared INTEGER NOT NULL DEFAULT 0,
    replies_sent INTEGER NOT NULL DEFAULT 0,
    mentions_sent INTEGER NOT NULL DEFAULT 0,
    dms_sent INTEGER NOT NULL DEFAULT 0,
    reactions_given INTEGER NOT NULL DEFAULT 0,
    reactions_received INTEGER NOT NULL DEFAULT 0,
    messages_edited INTEGER NOT NULL DEFAULT 0,
    messages_deleted INTEGER NOT NULL DEFAULT 0,
    pins_added INTEGER NOT NULL DEFAULT 0,
    voice_seconds INTEGER NOT NULL DEFAULT 0,
    voice_sessions INTEGER NOT NULL DEFAULT 0,
    screenshare_seconds INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
CREATE TABLE IF NOT EXISTS user_reaction_stats (
    user_name TEXT NOT NULL,
    emoji TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_name, emoji)
);
"#
    )
}

/// Whether the connection-level double opt-in gate is open for `user`:
/// server-wide toggle on AND the user opted in.
fn tracking_enabled(conn: &rusqlite::Connection, user: &str) -> rusqlite::Result<bool> {
    let server_enabled: Option<String> = conn
        .query_row(
            "SELECT value FROM server_settings WHERE key = 'stats_enabled'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    // Privacy default: tracking is OFF until an Owner/Admin explicitly
    // enables it for the server.
    if server_enabled.as_deref() != Some("1") {
        return Ok(false);
    }
    let opted_in: Option<i64> = conn
        .query_row(
            "SELECT opted_in FROM user_stats_opt_in WHERE user_name = ?1",
            params![user],
            |row| row.get(0),
        )
        .optional()?;
    Ok(opted_in == Some(1))
}

/// Whether the server-wide stats toggle is enabled.
pub async fn stats_server_enabled(db: &Db) -> Result<bool, DbError> {
    db.call_db(|conn| {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM server_settings WHERE key = 'stats_enabled'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.as_deref() == Some("1"))
    })
    .await
}

/// Set the server-wide stats toggle (Owner/Admin action, checked by caller).
pub async fn set_stats_server_enabled(db: &Db, enabled: bool) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute(
            "INSERT INTO server_settings (key, value) VALUES ('stats_enabled', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![if enabled { "1" } else { "0" }],
        )?;
        Ok(())
    })
    .await
}

/// Whether `user` has opted in to stat tracking.
pub async fn stats_opt_in(db: &Db, user: &str) -> Result<bool, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let opted_in: Option<i64> = conn
            .query_row(
                "SELECT opted_in FROM user_stats_opt_in WHERE user_name = ?1",
                params![user],
                |row| row.get(0),
            )
            .optional()?;
        Ok(opted_in == Some(1))
    })
    .await
}

/// Store a user's own opt-in decision.
pub async fn set_stats_opt_in(db: &Db, user: &str, enabled: bool) -> Result<(), DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            &format!(
                "INSERT INTO user_stats_opt_in (user_name, opted_in, updated_at)
                 VALUES (?1, ?2, {NOW_UTC})
                 ON CONFLICT(user_name) DO UPDATE
                 SET opted_in = excluded.opted_in, updated_at = excluded.updated_at"
            ),
            params![user, enabled as i64],
        )?;
        Ok(())
    })
    .await
}

/// Delete every stored counter for `user`. The opt-in row is kept so the
/// user's tracking preference survives the purge.
pub async fn purge_user_stats(db: &Db, user: &str) -> Result<(), DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        conn.execute("DELETE FROM user_stats WHERE user_name = ?1", params![user])?;
        conn.execute(
            "DELETE FROM user_reaction_stats WHERE user_name = ?1",
            params![user],
        )?;
        Ok(())
    })
    .await
}

/// Apply counter increments for `user`, plus optional per-emoji reaction
/// counts. Silently does nothing (returning `Ok(false)`) unless both the
/// server toggle and the user's opt-in are enabled — the double opt-in gate
/// lives here so every recording path goes through it.
pub async fn record_user_stats(
    db: &Db,
    user: &str,
    deltas: Vec<(Stat, i64)>,
    reaction_emoji: Option<String>,
) -> Result<bool, DbError> {
    if deltas.is_empty() && reaction_emoji.is_none() {
        return Ok(true);
    }
    let user = user.to_owned();
    db.call_db(move |conn| {
        if !tracking_enabled(conn, &user)? {
            return Ok(false);
        }

        conn.execute(
            "INSERT OR IGNORE INTO user_stats (user_name) VALUES (?1)",
            params![user],
        )?;
        for (stat, amount) in &deltas {
            let column = stat.column();
            // LongestMessageChars keeps a running maximum instead of summing.
            let sql = if *stat == Stat::LongestMessageChars {
                format!("UPDATE user_stats SET {column} = MAX({column}, ?1) WHERE user_name = ?2")
            } else {
                format!("UPDATE user_stats SET {column} = {column} + ?1 WHERE user_name = ?2")
            };
            conn.execute(&sql, params![amount, user])?;
        }
        if let Some(emoji) = &reaction_emoji {
            conn.execute(
                "INSERT INTO user_reaction_stats (user_name, emoji, count) VALUES (?1, ?2, 1)
                 ON CONFLICT(user_name, emoji) DO UPDATE SET count = count + 1",
                params![user, emoji],
            )?;
        }
        Ok(true)
    })
    .await
}

/// Fetch one user's lifetime counters; `None` when nothing was recorded yet.
pub async fn get_user_stats(db: &Db, user: &str) -> Result<Option<UserStatsRecord>, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        conn.query_row(
            "SELECT messages_sent, message_chars, message_bytes, longest_message_chars,
                    images_sent, gifs_sent, files_sent, upload_bytes, links_shared,
                    replies_sent, mentions_sent, dms_sent, reactions_given,
                    reactions_received, messages_edited, messages_deleted, pins_added,
                    voice_seconds, voice_sessions, screenshare_seconds, created_at
             FROM user_stats WHERE user_name = ?1",
            params![user],
            |row| {
                Ok(UserStatsRecord {
                    messages_sent: row.get(0)?,
                    message_chars: row.get(1)?,
                    message_bytes: row.get(2)?,
                    longest_message_chars: row.get(3)?,
                    images_sent: row.get(4)?,
                    gifs_sent: row.get(5)?,
                    files_sent: row.get(6)?,
                    upload_bytes: row.get(7)?,
                    links_shared: row.get(8)?,
                    replies_sent: row.get(9)?,
                    mentions_sent: row.get(10)?,
                    dms_sent: row.get(11)?,
                    reactions_given: row.get(12)?,
                    reactions_received: row.get(13)?,
                    messages_edited: row.get(14)?,
                    messages_deleted: row.get(15)?,
                    pins_added: row.get(16)?,
                    voice_seconds: row.get(17)?,
                    voice_sessions: row.get(18)?,
                    screenshare_seconds: row.get(19)?,
                    created_at: row.get(20)?,
                })
            },
        )
        .optional()
    })
    .await
}

/// A user's most-used reaction emojis, most used first.
pub async fn get_favorite_reactions(
    db: &Db,
    user: &str,
    limit: i64,
) -> Result<Vec<(String, i64)>, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT emoji, count FROM user_reaction_stats
             WHERE user_name = ?1 ORDER BY count DESC, emoji ASC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![user, limit], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}
