//! Text channel, voice channel and category persistence operations.
//!
//! Channels and categories carry a `position` column for the custom sort
//! order. New rows append at the end of their scope (the category for
//! channels, the whole list for categories); reordering rewrites the
//! positions of one scope in a single transaction.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};

/// Outcome of a channel rename. Channel names are unique, so a rename can fail
/// either because the target row is gone or because another channel already
/// owns the requested name.
pub enum RenameResult<T> {
    /// The channel was renamed; carries its updated record.
    Renamed(T),
    /// No channel exists with the given id.
    NotFound,
    /// Another channel already uses the requested name.
    NameTaken,
}

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

/// Create a new category and return its id and position. Without an explicit
/// position the category is appended after the existing ones.
pub async fn add_category(
    db: &Db,
    name: &str,
    position: Option<i32>,
) -> Result<(i32, i32), DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let row = conn.query_row(
            "INSERT INTO categories (name, position) VALUES (?1, \
                COALESCE(?2, (SELECT COALESCE(MAX(position) + 1, 0) FROM categories))) \
             RETURNING id, position",
            params![name, position],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(row)
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

/// Rewrite category positions to match the given id order. Rolls back and
/// returns false if any id does not exist.
pub async fn reorder_categories(db: &Db, ids: Vec<i32>) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE categories SET position = ?1 WHERE id = ?2")?;
            for (index, id) in ids.iter().enumerate() {
                if stmt.execute(params![index as i32, id])? == 0 {
                    return Ok(false);
                }
            }
        }
        tx.commit()?;
        Ok(true)
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
    pub position: i32,
}

fn row_to_channel(row: &rusqlite::Row) -> rusqlite::Result<ChannelRecord> {
    Ok(ChannelRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        category_id: row.get(2)?,
        description: row.get(3)?,
        position: row.get(4)?,
    })
}

/// Retrieve the list of text channels with their category assignments,
/// ordered by their custom position (per category) then name.
pub async fn get_channels(db: &Db) -> Vec<ChannelRecord> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, category_id, description, position FROM channels \
             ORDER BY position, name",
        )?;
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
                "SELECT id, name, category_id, description, position FROM channels WHERE id = ?1",
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

/// Insert a new channel at the end of its category and return its record.
/// Returns `None` if the name already exists.
pub async fn add_channel(
    db: &Db,
    name: &str,
    category_id: Option<i32>,
) -> Result<Option<ChannelRecord>, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(
            "INSERT INTO channels (name, category_id, position) VALUES (?1, ?2, \
                (SELECT COALESCE(MAX(position) + 1, 0) FROM channels WHERE category_id IS ?2)) \
             ON CONFLICT (name) DO NOTHING \
             RETURNING id, name, category_id, description, position",
        )?;
        let mut rows = stmt.query(params![name, category_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_channel(row)?)),
            None => Ok(None),
        }
    })
    .await
}

/// Rename a text channel. Names are unique, so this reports [`RenameResult`]
/// variants for the taken-name and unknown-channel cases separately. The check
/// and update run on the single writer connection, so no other rename can slip
/// in between them.
pub async fn rename_channel(
    db: &Db,
    id: i32,
    name: &str,
) -> Result<RenameResult<ChannelRecord>, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let taken: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM channels WHERE name = ?2 AND id <> ?1)",
            params![id, name],
            |row| row.get(0),
        )?;
        if taken {
            return Ok(RenameResult::NameTaken);
        }
        let mut stmt = conn.prepare(
            "UPDATE channels SET name = ?2 WHERE id = ?1 \
             RETURNING id, name, category_id, description, position",
        )?;
        let mut rows = stmt.query(params![id, name])?;
        match rows.next()? {
            Some(row) => Ok(RenameResult::Renamed(row_to_channel(row)?)),
            None => Ok(RenameResult::NotFound),
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

/// Move a text channel to the end of a different category (or out of any
/// category). Returns the channel's new position, or `None` if it does not exist.
pub async fn move_channel(
    db: &Db,
    id: i32,
    category_id: Option<i32>,
) -> Result<Option<i32>, DbError> {
    db.call_db(move |conn| {
        let position = conn
            .query_row(
                "UPDATE channels SET category_id = ?2, position = \
                    (SELECT COALESCE(MAX(position) + 1, 0) FROM channels \
                     WHERE category_id IS ?2 AND id <> ?1) \
                 WHERE id = ?1 RETURNING position",
                params![id, category_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(position)
    })
    .await
}

/// Rewrite the category assignment and positions of text or voice channels to
/// match the given id order. All listed channels end up in `category_id` at
/// their list index. Rolls back and returns false if any id does not exist.
pub async fn reorder_channels(
    db: &Db,
    category_id: Option<i32>,
    ids: Vec<i32>,
    voice: bool,
) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let table = if voice { "voice_channels" } else { "channels" };
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(&format!(
                "UPDATE {table} SET category_id = ?1, position = ?2 WHERE id = ?3"
            ))?;
            for (index, id) in ids.iter().enumerate() {
                if stmt.execute(params![category_id, index as i32, id])? == 0 {
                    return Ok(false);
                }
            }
        }
        tx.commit()?;
        Ok(true)
    })
    .await
}

/// Delete a channel together with its messages and wiki pages by ID.
/// There is no FK cascade from `channels`, so dependent rows are removed
/// explicitly; the transaction keeps a partial delete from surviving a crash.
pub async fn remove_channel(db: &Db, id: i32) -> Result<(), DbError> {
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        // Wiki revisions cascade from wiki_pages; FTS rows go via triggers.
        tx.execute("DELETE FROM wiki_pages WHERE channel_id = ?1", params![id])?;
        tx.execute("DELETE FROM messages WHERE channel_id = ?1", params![id])?;
        tx.execute("DELETE FROM channels WHERE id = ?1", params![id])?;
        tx.commit()?;
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
    pub position: i32,
}

fn row_to_voice_channel(row: &rusqlite::Row) -> rusqlite::Result<VoiceChannelRecord> {
    Ok(VoiceChannelRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        quality: row.get(2)?,
        bitrate: row.get(3)?,
        category_id: row.get(4)?,
        position: row.get(5)?,
    })
}

/// Retrieve all voice channels ordered by their custom position then name.
pub async fn get_voice_channels(db: &Db) -> Vec<VoiceChannelRecord> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, quality, bitrate, category_id, position FROM voice_channels \
             ORDER BY position, name",
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
                "SELECT id, name, quality, bitrate, category_id, position FROM voice_channels \
                 WHERE id = ?1",
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

/// Insert a new voice channel at the end of its category and return its
/// record. Returns `None` if the name already exists.
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
            "INSERT INTO voice_channels (name, quality, bitrate, category_id, position) \
             VALUES (?1, ?2, ?3, ?4, \
                (SELECT COALESCE(MAX(position) + 1, 0) FROM voice_channels WHERE category_id IS ?4)) \
             ON CONFLICT (name) DO NOTHING \
             RETURNING id, name, quality, bitrate, category_id, position",
        )?;
        let mut rows = stmt.query(params![name, quality, bitrate, category_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_voice_channel(row)?)),
            None => Ok(None),
        }
    })
    .await
}

/// Move a voice channel to the end of a different category (or out of any
/// category). Returns the channel's new position, or `None` if it does not exist.
pub async fn move_voice_channel(
    db: &Db,
    id: i32,
    category_id: Option<i32>,
) -> Result<Option<i32>, DbError> {
    db.call_db(move |conn| {
        let position = conn
            .query_row(
                "UPDATE voice_channels SET category_id = ?2, position = \
                    (SELECT COALESCE(MAX(position) + 1, 0) FROM voice_channels \
                     WHERE category_id IS ?2 AND id <> ?1) \
                 WHERE id = ?1 RETURNING position",
                params![id, category_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(position)
    })
    .await
}

/// Rename a voice channel. Mirrors [`rename_channel`]; voice channel names are
/// likewise unique.
pub async fn rename_voice_channel(
    db: &Db,
    id: i32,
    name: &str,
) -> Result<RenameResult<VoiceChannelRecord>, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        let taken: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM voice_channels WHERE name = ?2 AND id <> ?1)",
            params![id, name],
            |row| row.get(0),
        )?;
        if taken {
            return Ok(RenameResult::NameTaken);
        }
        let mut stmt = conn.prepare(
            "UPDATE voice_channels SET name = ?2 WHERE id = ?1 \
             RETURNING id, name, quality, bitrate, category_id, position",
        )?;
        let mut rows = stmt.query(params![id, name])?;
        match rows.next()? {
            Some(row) => Ok(RenameResult::Renamed(row_to_voice_channel(row)?)),
            None => Ok(RenameResult::NotFound),
        }
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
