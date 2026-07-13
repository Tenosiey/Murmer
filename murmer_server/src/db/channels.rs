//! Text channel, voice channel and category persistence operations.

use rusqlite::params;

use super::{Db, DbCall, DbError};

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

/// A category grouping text and voice channels.
#[derive(Clone)]
pub struct CategoryRecord {
    pub id: i32,
    pub name: String,
    pub position: i32,
}

/// Retrieve all categories ordered by position then id.
pub async fn get_categories(db: &Db) -> Vec<CategoryRecord> {
    db.call_db(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, name, position FROM categories ORDER BY position, id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CategoryRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    position: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
    .unwrap_or_default()
}

/// Create a new category and return its id.
pub async fn add_category(db: &Db, name: &str, position: i32) -> Result<i32, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let id = conn.query_row(
            "INSERT INTO categories (name, position) VALUES (?1, ?2) RETURNING id",
            params![name, position],
            |row| row.get(0),
        )?;
        Ok(id)
    })
    .await
}

/// Rename an existing category. Returns true if a row was updated.
pub async fn rename_category(db: &Db, id: i32, name: &str) -> Result<bool, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let count = conn.execute(
            "UPDATE categories SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(count > 0)
    })
    .await
}

/// Delete a category by id. Channels in this category have their `category_id` set to NULL
/// via the foreign key `ON DELETE SET NULL` constraint.
pub async fn remove_category(db: &Db, id: i32) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let count = conn.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
        Ok(count > 0)
    })
    .await
}

// ---------------------------------------------------------------------------
// Text channels
// ---------------------------------------------------------------------------

/// A text channel with its integer ID.
#[derive(Clone)]
pub struct ChannelRecord {
    pub id: i32,
    pub name: String,
    pub category_id: Option<i32>,
    pub description: String,
}

fn row_to_channel(row: &rusqlite::Row) -> rusqlite::Result<ChannelRecord> {
    Ok(ChannelRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        category_id: row.get(2)?,
        description: row.get(3)?,
    })
}

/// Retrieve the list of text channels with their category assignments.
pub async fn get_channels(db: &Db) -> Vec<ChannelRecord> {
    db.call_db(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, name, category_id, description FROM channels ORDER BY name")?;
        let rows = stmt
            .query_map([], row_to_channel)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
    .unwrap_or_default()
}

/// Look up a channel ID by name. Returns `None` if not found.
pub async fn get_channel_id_by_name(db: &Db, name: &str) -> Option<i32> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let id = conn
            .query_row(
                "SELECT id FROM channels WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .ok();
        Ok(id)
    })
    .await
    .ok()
    .flatten()
}

/// Look up a channel record by ID. Returns `None` if not found.
pub async fn get_channel_by_id(db: &Db, id: i32) -> Option<ChannelRecord> {
    db.call_db(move |conn| {
        let record = conn
            .query_row(
                "SELECT id, name, category_id, description FROM channels WHERE id = ?1",
                params![id],
                row_to_channel,
            )
            .ok();
        Ok(record)
    })
    .await
    .ok()
    .flatten()
}

/// Insert a new channel and return its record. Returns `None` if name already exists.
pub async fn add_channel(
    db: &Db,
    name: &str,
    category_id: Option<i32>,
) -> Result<Option<ChannelRecord>, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(
            "INSERT INTO channels (name, category_id) VALUES (?1, ?2) ON CONFLICT (name) DO NOTHING \
             RETURNING id, name, category_id, description",
        )?;
        let mut rows = stmt.query(params![name, category_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_channel(row)?)),
            None => Ok(None),
        }
    })
    .await
}

/// Update a text channel's description/topic. Returns true if a row was updated.
pub async fn set_channel_description(db: &Db, id: i32, description: &str) -> Result<bool, DbError> {
    let description = description.to_owned();
    db.call_db(move |conn| {
        let count = conn.execute(
            "UPDATE channels SET description = ?2 WHERE id = ?1",
            params![id, description],
        )?;
        Ok(count > 0)
    })
    .await
}

/// Move a text channel to a different category (or remove from category).
pub async fn move_channel(db: &Db, id: i32, category_id: Option<i32>) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let count = conn.execute(
            "UPDATE channels SET category_id = ?2 WHERE id = ?1",
            params![id, category_id],
        )?;
        Ok(count > 0)
    })
    .await
}

/// Delete a channel and all its messages by ID.
pub async fn remove_channel(db: &Db, id: i32) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute("DELETE FROM messages WHERE channel_id = ?1", params![id])?;
        conn.execute("DELETE FROM channels WHERE id = ?1", params![id])?;
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------------------
// Voice channels
// ---------------------------------------------------------------------------

/// A voice channel with its integer ID and audio configuration.
#[derive(Clone)]
pub struct VoiceChannelRecord {
    pub id: i32,
    pub name: String,
    pub quality: String,
    pub bitrate: Option<i32>,
    pub category_id: Option<i32>,
}

fn row_to_voice_channel(row: &rusqlite::Row) -> rusqlite::Result<VoiceChannelRecord> {
    Ok(VoiceChannelRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        quality: row.get(2)?,
        bitrate: row.get(3)?,
        category_id: row.get(4)?,
    })
}

/// Retrieve all voice channels ordered by name.
pub async fn get_voice_channels(db: &Db) -> Vec<VoiceChannelRecord> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, quality, bitrate, category_id FROM voice_channels ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], row_to_voice_channel)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
    .unwrap_or_default()
}

/// Look up a voice channel record by ID. Returns `None` if not found.
pub async fn get_voice_channel_by_id(db: &Db, id: i32) -> Option<VoiceChannelRecord> {
    db.call_db(move |conn| {
        let record = conn
            .query_row(
                "SELECT id, name, quality, bitrate, category_id FROM voice_channels WHERE id = ?1",
                params![id],
                row_to_voice_channel,
            )
            .ok();
        Ok(record)
    })
    .await
    .ok()
    .flatten()
}

/// Insert a new voice channel and return its record. Returns `None` if name already exists.
pub async fn add_voice_channel(
    db: &Db,
    name: &str,
    quality: &str,
    bitrate: Option<i32>,
    category_id: Option<i32>,
) -> Result<Option<VoiceChannelRecord>, DbError> {
    let name = name.to_owned();
    let quality = quality.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(
            "INSERT INTO voice_channels (name, quality, bitrate, category_id) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (name) DO NOTHING RETURNING id, name, quality, bitrate, category_id",
        )?;
        let mut rows = stmt.query(params![name, quality, bitrate, category_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_voice_channel(row)?)),
            None => Ok(None),
        }
    })
    .await
}

/// Move a voice channel to a different category (or remove from category).
pub async fn move_voice_channel(
    db: &Db,
    id: i32,
    category_id: Option<i32>,
) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let count = conn.execute(
            "UPDATE voice_channels SET category_id = ?2 WHERE id = ?1",
            params![id, category_id],
        )?;
        Ok(count > 0)
    })
    .await
}

/// Update an existing voice channel's audio configuration.
pub async fn update_voice_channel(
    db: &Db,
    id: i32,
    quality: &str,
    bitrate: Option<i32>,
) -> Result<bool, DbError> {
    let quality = quality.to_owned();
    db.call_db(move |conn| {
        let count = conn.execute(
            "UPDATE voice_channels SET quality = ?2, bitrate = ?3 WHERE id = ?1",
            params![id, quality, bitrate],
        )?;
        Ok(count > 0)
    })
    .await
}

/// Delete an existing voice channel by ID.
pub async fn remove_voice_channel(db: &Db, id: i32) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute("DELETE FROM voice_channels WHERE id = ?1", params![id])?;
        Ok(())
    })
    .await
}
