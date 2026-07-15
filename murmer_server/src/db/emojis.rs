//! Custom server emoji persistence operations.

use rusqlite::params;

use super::{Db, DbCall, DbError};

/// A custom emoji registered on this server.
pub struct EmojiRecord {
    pub name: String,
    pub url: String,
    pub uploaded_by: String,
    pub created_at: String,
}

/// List all custom emojis ordered by name.
pub async fn get_emojis(db: &Db) -> Result<Vec<EmojiRecord>, DbError> {
    db.call_db(|conn| {
        let mut stmt =
            conn.prepare("SELECT name, url, uploaded_by, created_at FROM emojis ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(EmojiRecord {
                name: row.get(0)?,
                url: row.get(1)?,
                uploaded_by: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect()
    })
    .await
}

/// Number of custom emojis currently registered.
pub async fn count_emojis(db: &Db) -> Result<i64, DbError> {
    db.call_db(|conn| conn.query_row("SELECT COUNT(*) FROM emojis", [], |row| row.get(0)))
        .await
}

/// Whether a custom emoji with the given name exists.
pub async fn emoji_exists(db: &Db, name: &str) -> Result<bool, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM emojis WHERE name = ?1",
            params![name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
    })
    .await
}

/// Register a custom emoji. Returns `false` when the name is already taken.
pub async fn add_emoji(db: &Db, name: &str, url: &str, uploaded_by: &str) -> Result<bool, DbError> {
    let name = name.to_owned();
    let url = url.to_owned();
    let uploaded_by = uploaded_by.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute(
            "INSERT OR IGNORE INTO emojis (name, url, uploaded_by) VALUES (?1, ?2, ?3)",
            params![name, url, uploaded_by],
        )?;
        Ok(affected > 0)
    })
    .await
}

/// Delete a custom emoji by name. Returns the stored file URL when a row was
/// removed so the caller can clean up the file on disk.
pub async fn remove_emoji(db: &Db, name: &str) -> Result<Option<String>, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let url: Option<String> = conn
            .query_row(
                "SELECT url FROM emojis WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .ok();
        if url.is_some() {
            conn.execute("DELETE FROM emojis WHERE name = ?1", params![name])?;
        }
        Ok(url)
    })
    .await
}
