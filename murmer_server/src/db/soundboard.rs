//! Soundboard sound persistence operations.
//!
//! Sounds are a server-wide shared library: one row per uploaded clip, with a
//! display name that is unique case-insensitively. Unlike emojis (keyed by
//! name) sounds carry a stable integer id, so renaming one does not break the
//! `play-sound` frames or the per-sound volume settings clients store locally.
//!
//! `play_count`/`last_played_at` are deliberately *aggregate only*: they belong
//! to the sound, never to a person, so they need no privacy gate. Per-user
//! soundboard counters live in [`super::stats`] behind the double opt-in.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError, NOW_UTC};

/// A soundboard sound registered on this server.
pub struct SoundRecord {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub uploaded_by: String,
    pub play_count: i64,
    pub created_at: String,
}

/// Outcome of a rename attempt, so the handler can report the precise error.
pub enum RenameOutcome {
    Renamed,
    NotFound,
    NameTaken,
}

/// Create the soundboard table. Called from [`super::run_schema`].
pub(super) fn soundboard_schema() -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS soundboard_sounds (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    url TEXT NOT NULL,
    uploaded_by TEXT NOT NULL DEFAULT '',
    play_count INTEGER NOT NULL DEFAULT 0,
    last_played_at TEXT,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
"#
    )
}

const SELECT_SOUND: &str =
    "SELECT id, name, url, uploaded_by, play_count, created_at FROM soundboard_sounds";

fn row_to_sound(row: &rusqlite::Row) -> rusqlite::Result<SoundRecord> {
    Ok(SoundRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        uploaded_by: row.get(3)?,
        play_count: row.get(4)?,
        created_at: row.get(5)?,
    })
}

/// List every sound ordered by name.
pub async fn get_sounds(db: &Db) -> Result<Vec<SoundRecord>, DbError> {
    db.call_db(|conn| {
        let mut stmt =
            conn.prepare_cached(&format!("{SELECT_SOUND} ORDER BY name COLLATE NOCASE"))?;
        let rows = stmt.query_map([], row_to_sound)?;
        rows.collect()
    })
    .await
}

/// Fetch a single sound by id.
pub async fn get_sound(db: &Db, id: i64) -> Result<Option<SoundRecord>, DbError> {
    db.call_db(move |conn| {
        conn.query_row(
            &format!("{SELECT_SOUND} WHERE id = ?1"),
            params![id],
            row_to_sound,
        )
        .optional()
    })
    .await
}

/// Number of sounds currently registered.
pub async fn count_sounds(db: &Db) -> Result<i64, DbError> {
    db.call_db(|conn| {
        conn.query_row("SELECT COUNT(*) FROM soundboard_sounds", [], |row| {
            row.get(0)
        })
    })
    .await
}

/// Register a sound. Returns the new id, or `None` when the name is taken.
pub async fn add_sound(
    db: &Db,
    name: &str,
    url: &str,
    uploaded_by: &str,
) -> Result<Option<i64>, DbError> {
    let name = name.to_owned();
    let url = url.to_owned();
    let uploaded_by = uploaded_by.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute(
            "INSERT OR IGNORE INTO soundboard_sounds (name, url, uploaded_by) VALUES (?1, ?2, ?3)",
            params![name, url, uploaded_by],
        )?;
        Ok(if affected > 0 {
            Some(conn.last_insert_rowid())
        } else {
            None
        })
    })
    .await
}

/// Rename a sound, keeping its id (and therefore clients' per-sound settings).
pub async fn rename_sound(db: &Db, id: i64, name: &str) -> Result<RenameOutcome, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM soundboard_sounds WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Ok(RenameOutcome::NotFound);
        }
        // A different row already holding the name (case-insensitively) is a
        // conflict; renaming a sound to its own name is a no-op success.
        let taken: i64 = conn.query_row(
            "SELECT COUNT(*) FROM soundboard_sounds WHERE name = ?1 COLLATE NOCASE AND id != ?2",
            params![name, id],
            |row| row.get(0),
        )?;
        if taken > 0 {
            return Ok(RenameOutcome::NameTaken);
        }
        conn.execute(
            "UPDATE soundboard_sounds SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(RenameOutcome::Renamed)
    })
    .await
}

/// Delete a sound by id. Returns the stored file URL when a row was removed so
/// the caller can clean up the file on disk.
pub async fn remove_sound(db: &Db, id: i64) -> Result<Option<String>, DbError> {
    db.call_db(move |conn| {
        let url: Option<String> = conn
            .query_row(
                "SELECT url FROM soundboard_sounds WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if url.is_some() {
            conn.execute("DELETE FROM soundboard_sounds WHERE id = ?1", params![id])?;
        }
        Ok(url)
    })
    .await
}

/// Bump a sound's aggregate play counter. Not attributed to any user.
pub async fn note_sound_played(db: &Db, id: i64) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute(
            &format!(
                "UPDATE soundboard_sounds
                 SET play_count = play_count + 1, last_played_at = {NOW_UTC}
                 WHERE id = ?1"
            ),
            params![id],
        )?;
        Ok(())
    })
    .await
}
