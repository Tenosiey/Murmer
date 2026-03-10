//! Text channel, voice channel and category persistence operations.

use tokio_postgres::Client;

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
pub async fn get_categories(db: &Client) -> Vec<CategoryRecord> {
    match db
        .query(
            "SELECT id, name, position FROM categories ORDER BY position, id",
            &[],
        )
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|row| CategoryRecord {
                id: row.get(0),
                name: row.get(1),
                position: row.get(2),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Create a new category and return its id.
pub async fn add_category(
    db: &Client,
    name: &str,
    position: i32,
) -> Result<i32, tokio_postgres::Error> {
    let row = db
        .query_one(
            "INSERT INTO categories (name, position) VALUES ($1, $2) RETURNING id",
            &[&name, &position],
        )
        .await?;
    Ok(row.get(0))
}

/// Rename an existing category. Returns true if a row was updated.
pub async fn rename_category(
    db: &Client,
    id: i32,
    name: &str,
) -> Result<bool, tokio_postgres::Error> {
    db.execute(
        "UPDATE categories SET name = $2 WHERE id = $1",
        &[&id, &name],
    )
    .await
    .map(|count| count > 0)
}

/// Delete a category by id. Channels in this category have their `category_id` set to NULL
/// via the foreign key `ON DELETE SET NULL` constraint.
pub async fn remove_category(db: &Client, id: i32) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM categories WHERE id = $1", &[&id])
        .await
        .map(|count| count > 0)
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
}

/// Retrieve the list of text channels with their category assignments.
pub async fn get_channels(db: &Client) -> Vec<ChannelRecord> {
    match db
        .query(
            "SELECT id, name, category_id FROM channels ORDER BY name",
            &[],
        )
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|row| ChannelRecord {
                id: row.get(0),
                name: row.get(1),
                category_id: row.get(2),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Look up a channel ID by name. Returns `None` if not found.
pub async fn get_channel_id_by_name(db: &Client, name: &str) -> Option<i32> {
    db.query_opt("SELECT id FROM channels WHERE name = $1", &[&name])
        .await
        .ok()
        .flatten()
        .map(|row| row.get(0))
}

/// Look up a channel record by ID. Returns `None` if not found.
pub async fn get_channel_by_id(db: &Client, id: i32) -> Option<ChannelRecord> {
    db.query_opt(
        "SELECT id, name, category_id FROM channels WHERE id = $1",
        &[&id],
    )
    .await
    .ok()
    .flatten()
    .map(|row| ChannelRecord {
        id: row.get(0),
        name: row.get(1),
        category_id: row.get(2),
    })
}

/// Insert a new channel and return its record. Returns `None` if name already exists.
pub async fn add_channel(
    db: &Client,
    name: &str,
    category_id: Option<i32>,
) -> Result<Option<ChannelRecord>, tokio_postgres::Error> {
    match db
        .query_opt(
            "INSERT INTO channels (name, category_id) VALUES ($1, $2) ON CONFLICT (name) DO NOTHING RETURNING id, name, category_id",
            &[&name, &category_id],
        )
        .await?
    {
        Some(row) => Ok(Some(ChannelRecord {
            id: row.get(0),
            name: row.get(1),
            category_id: row.get(2),
        })),
        None => Ok(None),
    }
}

/// Move a text channel to a different category (or remove from category).
pub async fn move_channel(
    db: &Client,
    id: i32,
    category_id: Option<i32>,
) -> Result<bool, tokio_postgres::Error> {
    db.execute(
        "UPDATE channels SET category_id = $2 WHERE id = $1",
        &[&id, &category_id],
    )
    .await
    .map(|count| count > 0)
}

/// Delete a channel and all its messages by ID.
pub async fn remove_channel(db: &Client, id: i32) -> Result<(), tokio_postgres::Error> {
    db.execute("DELETE FROM messages WHERE channel_id = $1", &[&id])
        .await?;
    db.execute("DELETE FROM channels WHERE id = $1", &[&id])
        .await
        .map(|_| ())
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

/// Retrieve all voice channels ordered by name.
pub async fn get_voice_channels(db: &Client) -> Vec<VoiceChannelRecord> {
    match db
        .query(
            "SELECT id, name, quality, bitrate, category_id FROM voice_channels ORDER BY name",
            &[],
        )
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|row| VoiceChannelRecord {
                id: row.get(0),
                name: row.get(1),
                quality: row.get(2),
                bitrate: row.get(3),
                category_id: row.get(4),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Look up a voice channel record by ID. Returns `None` if not found.
pub async fn get_voice_channel_by_id(db: &Client, id: i32) -> Option<VoiceChannelRecord> {
    db.query_opt(
        "SELECT id, name, quality, bitrate, category_id FROM voice_channels WHERE id = $1",
        &[&id],
    )
    .await
    .ok()
    .flatten()
    .map(|row| VoiceChannelRecord {
        id: row.get(0),
        name: row.get(1),
        quality: row.get(2),
        bitrate: row.get(3),
        category_id: row.get(4),
    })
}

/// Insert a new voice channel and return its record. Returns `None` if name already exists.
pub async fn add_voice_channel(
    db: &Client,
    name: &str,
    quality: &str,
    bitrate: Option<i32>,
    category_id: Option<i32>,
) -> Result<Option<VoiceChannelRecord>, tokio_postgres::Error> {
    match db
        .query_opt(
            "INSERT INTO voice_channels (name, quality, bitrate, category_id) VALUES ($1, $2, $3, $4) \
                ON CONFLICT (name) DO NOTHING RETURNING id, name, quality, bitrate, category_id",
            &[&name, &quality, &bitrate, &category_id],
        )
        .await?
    {
        Some(row) => Ok(Some(VoiceChannelRecord {
            id: row.get(0),
            name: row.get(1),
            quality: row.get(2),
            bitrate: row.get(3),
            category_id: row.get(4),
        })),
        None => Ok(None),
    }
}

/// Move a voice channel to a different category (or remove from category).
pub async fn move_voice_channel(
    db: &Client,
    id: i32,
    category_id: Option<i32>,
) -> Result<bool, tokio_postgres::Error> {
    db.execute(
        "UPDATE voice_channels SET category_id = $2 WHERE id = $1",
        &[&id, &category_id],
    )
    .await
    .map(|count| count > 0)
}

/// Update an existing voice channel's audio configuration.
pub async fn update_voice_channel(
    db: &Client,
    id: i32,
    quality: &str,
    bitrate: Option<i32>,
) -> Result<bool, tokio_postgres::Error> {
    db.execute(
        "UPDATE voice_channels SET quality = $2, bitrate = $3 WHERE id = $1",
        &[&id, &quality, &bitrate],
    )
    .await
    .map(|count| count > 0)
}

/// Delete an existing voice channel by ID.
pub async fn remove_voice_channel(db: &Client, id: i32) -> Result<(), tokio_postgres::Error> {
    db.execute("DELETE FROM voice_channels WHERE id = $1", &[&id])
        .await
        .map(|_| ())
}
