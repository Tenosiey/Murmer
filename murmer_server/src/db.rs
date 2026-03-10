//! Database connection helpers and functions for loading chat history.
//!
//! Channels are identified by an auto-incrementing integer `id`. The `name`
//! column remains for display purposes. Messages reference channels via
//! `channel_id`.

use std::collections::HashMap;

use tokio_postgres::{Client, NoTls};
use tracing::{error, warn};

fn escape_like_pattern(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Initialize a PostgreSQL [`Client`] and ensure the schema exists.
/// The connection is retried for a few seconds if the database is not ready.
#[tracing::instrument(skip(db_url))]
pub async fn init(db_url: &str) -> Result<Client, tokio_postgres::Error> {
    let (client, connection) = {
        let mut attempts = 0u8;
        loop {
            match tokio_postgres::connect(db_url, NoTls).await {
                Ok(result) => break result,
                Err(e) if attempts < 30 => {
                    attempts += 1;
                    warn!("database not ready ({e}), retrying...");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                Err(e) => return Err(e),
            }
        }
    };

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::error!("db connection error: {e}");
        }
    });

    client
        .batch_execute(
            r#"CREATE TABLE IF NOT EXISTS reactions (
    message_id INTEGER NOT NULL,
    user_name TEXT NOT NULL,
    emoji TEXT NOT NULL,
    PRIMARY KEY (message_id, user_name, emoji)
);
CREATE TABLE IF NOT EXISTS roles (
    public_key TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    color TEXT
);
CREATE TABLE IF NOT EXISTS categories (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS bots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    owner_key TEXT NOT NULL DEFAULT '',
    permissions INTEGER NOT NULL DEFAULT 0,
    description TEXT NOT NULL DEFAULT '',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#,
        )
        .await
        .map_err(|e| {
            error!("Failed to initialize database schema: {}", e);
            e
        })?;

    // Ensure channels table has id-based schema
    client
        .batch_execute(
            r#"
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'channels') THEN
        CREATE TABLE channels (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL
        );
    ELSIF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'channels' AND column_name = 'id') THEN
        ALTER TABLE channels ADD COLUMN id SERIAL;
        ALTER TABLE channels DROP CONSTRAINT IF EXISTS channels_pkey;
        ALTER TABLE channels ADD PRIMARY KEY (id);
        ALTER TABLE channels ADD CONSTRAINT channels_name_unique UNIQUE (name);
    END IF;
END $$;
"#,
        )
        .await
        .map_err(|e| {
            error!("Failed to migrate channels table: {}", e);
            e
        })?;

    // Ensure voice_channels table has id-based schema
    client
        .batch_execute(
            r#"
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'voice_channels') THEN
        CREATE TABLE voice_channels (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            quality TEXT NOT NULL DEFAULT 'standard',
            bitrate INTEGER,
            category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL
        );
    ELSIF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'voice_channels' AND column_name = 'id') THEN
        ALTER TABLE voice_channels ADD COLUMN id SERIAL;
        ALTER TABLE voice_channels DROP CONSTRAINT IF EXISTS voice_channels_pkey;
        ALTER TABLE voice_channels ADD PRIMARY KEY (id);
        ALTER TABLE voice_channels ADD CONSTRAINT voice_channels_name_unique UNIQUE (name);
    END IF;
END $$;
"#,
        )
        .await
        .map_err(|e| {
            error!("Failed to migrate voice_channels table: {}", e);
            e
        })?;

    // Ensure messages table uses channel_id instead of channel
    client
        .batch_execute(
            r#"
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'messages') THEN
        CREATE TABLE messages (
            id SERIAL PRIMARY KEY,
            channel_id INTEGER NOT NULL REFERENCES channels(id),
            content TEXT NOT NULL
        );
    ELSIF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'messages' AND column_name = 'channel') THEN
        ALTER TABLE messages ADD COLUMN IF NOT EXISTS channel_id INTEGER;
        UPDATE messages SET channel_id = c.id FROM channels c WHERE messages.channel = c.name AND messages.channel_id IS NULL;
        DELETE FROM messages WHERE channel_id IS NULL;
        ALTER TABLE messages ALTER COLUMN channel_id SET NOT NULL;
        ALTER TABLE messages DROP COLUMN channel;
        ALTER TABLE messages ADD CONSTRAINT messages_channel_id_fk FOREIGN KEY (channel_id) REFERENCES channels(id);
    END IF;
END $$;
"#,
        )
        .await
        .map_err(|e| {
            error!("Failed to migrate messages table: {}", e);
            e
        })?;

    // Seed the default channel
    client
        .execute(
            "INSERT INTO channels (name) VALUES ('general') ON CONFLICT (name) DO NOTHING",
            &[],
        )
        .await
        .ok();

    // Legacy migration columns
    client
        .batch_execute(
            "ALTER TABLE roles ADD COLUMN IF NOT EXISTS color TEXT;\n\
             ALTER TABLE voice_channels ADD COLUMN IF NOT EXISTS quality TEXT NOT NULL DEFAULT 'standard';\n\
             ALTER TABLE voice_channels ADD COLUMN IF NOT EXISTS bitrate INTEGER;\n\
             UPDATE voice_channels SET quality = 'standard' WHERE quality IS NULL OR trim(quality) = '';\n\
             UPDATE voice_channels SET bitrate = 64000 WHERE bitrate IS NULL;\n\
             ALTER TABLE channels ADD COLUMN IF NOT EXISTS category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL;\n\
             ALTER TABLE voice_channels ADD COLUMN IF NOT EXISTS category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL;",
        )
        .await
        .ok();

    if let Err(e) = client
        .batch_execute("CREATE EXTENSION IF NOT EXISTS pg_trgm;")
        .await
    {
        warn!("Failed to enable pg_trgm extension: {e}");
    }

    if let Err(e) = client
        .batch_execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_channel_id ON messages (channel_id);",
        )
        .await
    {
        warn!("Failed to ensure messages channel_id index: {e}");
    }

    if let Err(e) = client
        .batch_execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_content_trgm ON messages USING GIN (content gin_trgm_ops);",
        )
        .await
    {
        warn!("Failed to ensure trigram index for message content: {e}");
    }

    // Drop legacy index if it exists
    client
        .batch_execute("DROP INDEX IF EXISTS idx_messages_channel;")
        .await
        .ok();

    Ok(client)
}

use axum::extract::ws::{Message, WebSocket};
use futures::SinkExt;
use serde_json::Value;

/// Fetch a slice of messages from the database for a channel by ID.
pub async fn fetch_history(
    db: &Client,
    channel_id: i32,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let rows = if let Some(id) = before {
        let id32 = match i32::try_from(id) {
            Ok(val) => val,
            Err(_) => {
                error!("Message ID too large for database query: {}", id);
                return Ok(Vec::new());
            }
        };
        db.query(
            "SELECT id::bigint, content FROM messages WHERE channel_id = $1 AND id < $2 ORDER BY id DESC LIMIT $3",
            &[&channel_id, &id32, &limit],
        )
        .await?
    } else {
        db.query(
            "SELECT id::bigint, content FROM messages WHERE channel_id = $1 ORDER BY id DESC LIMIT $2",
            &[&channel_id, &limit],
        )
        .await?
    };
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<_, i64>(0), row.get(1)))
        .collect())
}

/// Send a slice of messages over the WebSocket as a `history` payload.
pub async fn send_history(
    db: &Client,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    channel_id: i32,
    before: Option<i64>,
    limit: i64,
) {
    match fetch_history(db, channel_id, before, limit).await {
        Ok(rows) => {
            let mut ids32 = Vec::new();
            for (id, _) in &rows {
                match i32::try_from(*id) {
                    Ok(val) => ids32.push(val),
                    Err(_) => error!("Message ID too large for reaction lookup: {}", id),
                }
            }

            let reaction_map = if ids32.is_empty() {
                HashMap::new()
            } else {
                match get_reactions_for_messages(db, &ids32).await {
                    Ok(map) => map,
                    Err(e) => {
                        error!("db reaction load error: {e}");
                        HashMap::new()
                    }
                }
            };

            let mut msgs = Vec::new();
            for (id, content) in rows.into_iter().rev() {
                if let Ok(mut val) = serde_json::from_str::<Value>(&content) {
                    val["id"] = Value::from(id);
                    #[allow(clippy::collapsible_if)]
                    if let Ok(id32) = i32::try_from(id) {
                        if let Some(reactions) = reaction_map.get(&id32) {
                            if let Ok(value) = serde_json::to_value(reactions) {
                                val["reactions"] = value;
                            }
                        }
                    }
                    msgs.push(val);
                }
            }
            if !msgs.is_empty() {
                let payload = serde_json::json!({"type": "history", "messages": msgs});
                let _ = sender.send(Message::Text(payload.to_string())).await;
            }
        }
        Err(e) => error!("db history error: {e}"),
    }
}

/// Search for messages containing a query string within a channel.
pub async fn search_messages(
    db: &Client,
    channel_id: i32,
    query: &str,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let pattern = format!("%{}%", escape_like_pattern(query));
    let rows = db
        .query(
            "SELECT id::bigint, content FROM messages WHERE channel_id = $1 AND content ILIKE $2 ESCAPE '\\\\' ORDER BY id DESC LIMIT $3",
            &[&channel_id, &pattern, &limit],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<_, i64>(0), row.get(1)))
        .collect())
}

/// Retrieve reactions for a set of message IDs, grouped by emoji.
pub async fn get_reactions_for_messages(
    db: &Client,
    ids: &[i32],
) -> Result<HashMap<i32, HashMap<String, Vec<String>>>, tokio_postgres::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = db
        .query(
            "SELECT message_id, emoji, user_name FROM reactions WHERE message_id = ANY($1)",
            &[&ids],
        )
        .await?;

    let mut map: HashMap<i32, HashMap<String, Vec<String>>> = HashMap::new();
    for row in rows {
        let message_id: i32 = row.get(0);
        let emoji: String = row.get(1);
        let user: String = row.get(2);
        let emoji_map = map.entry(message_id).or_default();
        let users = emoji_map.entry(emoji).or_default();
        users.push(user);
    }

    for emoji_map in map.values_mut() {
        for users in emoji_map.values_mut() {
            users.sort();
            users.dedup();
        }
    }

    Ok(map)
}

/// Retrieve all reactions for a single message ID, grouped by emoji.
pub async fn get_reaction_summary(
    db: &Client,
    message_id: i32,
) -> Result<HashMap<String, Vec<String>>, tokio_postgres::Error> {
    let mut map = get_reactions_for_messages(db, &[message_id]).await?;
    Ok(map.remove(&message_id).unwrap_or_default())
}

/// Add a reaction to a message. Duplicate reactions by the same user are ignored.
pub async fn add_reaction(
    db: &Client,
    message_id: i32,
    user: &str,
    emoji: &str,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO reactions (message_id, user_name, emoji) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        &[&message_id, &user, &emoji],
    )
    .await
    .map(|_| ())
}

/// Remove a reaction from a message.
pub async fn remove_reaction(
    db: &Client,
    message_id: i32,
    user: &str,
    emoji: &str,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "DELETE FROM reactions WHERE message_id = $1 AND user_name = $2 AND emoji = $3",
        &[&message_id, &user, &emoji],
    )
    .await
    .map(|_| ())
}

/// Return the channel ID a message belongs to, if it exists.
pub async fn get_message_channel_id(
    db: &Client,
    message_id: i32,
) -> Result<Option<i32>, tokio_postgres::Error> {
    db.query_opt(
        "SELECT channel_id FROM messages WHERE id = $1",
        &[&message_id],
    )
    .await
    .map(|row| row.map(|r| r.get(0)))
}

/// Metadata for a stored message.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub channel_id: i32,
    pub content: Value,
}

/// Fetch a message record including its channel_id and JSON payload.
pub async fn get_message_record(
    db: &Client,
    message_id: i32,
) -> Result<Option<MessageRecord>, tokio_postgres::Error> {
    match db
        .query_opt(
            "SELECT channel_id, content FROM messages WHERE id = $1",
            &[&message_id],
        )
        .await?
    {
        Some(row) => {
            let channel_id: i32 = row.get(0);
            let raw_content: String = row.get(1);
            let content = match serde_json::from_str::<Value>(&raw_content) {
                Ok(value) => value,
                Err(error) => {
                    error!("Failed to parse stored message JSON (id {message_id}): {error}");
                    Value::Null
                }
            };
            Ok(Some(MessageRecord {
                channel_id,
                content,
            }))
        }
        None => Ok(None),
    }
}

/// Delete a message by ID. Returns `true` if a row was removed.
pub async fn delete_message(db: &Client, message_id: i32) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM messages WHERE id = $1", &[&message_id])
        .await
        .map(|affected| affected > 0)
}

/// Get the role for a user by public key, if any.
pub async fn get_role(db: &Client, key: &str) -> Option<(String, Option<String>)> {
    db.query_opt(
        "SELECT role, color FROM roles WHERE public_key = $1",
        &[&key],
    )
    .await
    .ok()
    .flatten()
    .map(|row| (row.get(0), row.get(1)))
}

/// Insert or update a user's role.
pub async fn set_role(
    db: &Client,
    key: &str,
    role: &str,
    color: Option<&str>,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO roles (public_key, role, color) VALUES ($1, $2, $3) \
        ON CONFLICT (public_key) DO UPDATE SET role = EXCLUDED.role, color = EXCLUDED.color",
        &[&key, &role, &color],
    )
    .await
    .map(|_| ())
}

/// Remove a user's role by public key.
pub async fn remove_role(db: &Client, key: &str) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM roles WHERE public_key = $1", &[&key])
        .await
        .map(|affected| affected > 0)
}

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

/// Delete a category by id. Channels in this category have their category_id set to NULL.
pub async fn remove_category(db: &Client, id: i32) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM categories WHERE id = $1", &[&id])
        .await
        .map(|count| count > 0)
}

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

/// Insert a new channel and return its record. Returns None if name already exists.
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

/// Move a text channel to a different category (or none).
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

/// Delete an existing channel by ID.
pub async fn remove_channel(db: &Client, id: i32) -> Result<(), tokio_postgres::Error> {
    db.execute("DELETE FROM messages WHERE channel_id = $1", &[&id])
        .await?;
    db.execute("DELETE FROM channels WHERE id = $1", &[&id])
        .await
        .map(|_| ())
}

/// A voice channel with its integer ID.
#[derive(Clone)]
pub struct VoiceChannelRecord {
    pub id: i32,
    pub name: String,
    pub quality: String,
    pub bitrate: Option<i32>,
    pub category_id: Option<i32>,
}

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

/// Insert a new voice channel and return its record.
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

/// Move a voice channel to a different category (or none).
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

/// Update an existing voice channel configuration.
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
