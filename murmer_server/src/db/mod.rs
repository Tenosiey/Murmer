//! Database connection, schema initialization and domain-specific queries.
//!
//! Channels are identified by an auto-incrementing integer `id`. The `name`
//! column remains for display purposes. Messages reference channels via
//! `channel_id`.
//!
//! Submodules group queries by domain:
//! - [`channels`] – text channels, voice channels and categories
//! - [`messages`] – message CRUD and history retrieval
//! - [`reactions`] – emoji reaction operations
//! - [`roles`] – user role persistence

mod channels;
mod messages;
mod reactions;
mod roles;

pub use channels::*;
pub use messages::*;
pub use reactions::*;
pub use roles::*;

use tokio_postgres::{Client, NoTls};
use tracing::{error, warn};

/// Initialize a PostgreSQL [`Client`] and ensure the schema exists.
/// The connection is retried for up to 30 seconds if the database is not ready.
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

    client
        .execute(
            "INSERT INTO channels (name) VALUES ('general') ON CONFLICT (name) DO NOTHING",
            &[],
        )
        .await
        .ok();

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

    client
        .batch_execute("DROP INDEX IF EXISTS idx_messages_channel;")
        .await
        .ok();

    Ok(client)
}
