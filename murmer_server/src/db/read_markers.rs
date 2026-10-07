//! Per-user read markers: the newest message each account has seen, per
//! channel and per direct-message conversation.
//!
//! They live on the server rather than in the client's storage because one
//! account is often signed in from several clients at once — the desktop app
//! and a browser — and each would otherwise keep treating what the other
//! already read as unread. A marker only ever moves forward: two clients
//! racing to report different positions settle on the newer one, whichever
//! write lands last.
//!
//! A marker is the account's own pointer and reveals nothing, so it is not
//! checked against the message it names. Message ids are AUTOINCREMENT and
//! never reused, so a marker left behind by a deleted message or a purge
//! cannot hide a later message.

use rusqlite::{OptionalExtension, params};
use std::collections::HashMap;

use super::{Db, DbCall, DbError};

/// Create the marker tables, seeding the direct-message markers of a
/// database that predates them.
///
/// The client works out a channel's unread messages itself; a DM's unread
/// count comes from here, from every message past the marker. Without the
/// seed, the first start after this shipped would report every DM anybody
/// ever received as unread. Whether the seed is owed is decided by whether
/// the table existed before this run, and both happen in one transaction, as
/// for `messages_fts`.
pub(super) fn read_markers_schema(conn: &mut rusqlite::Connection) -> rusqlite::Result<()> {
    let existed = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'dm_read_markers'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    let tx = conn.transaction()?;
    tx.execute_batch(
        r#"CREATE TABLE IF NOT EXISTS channel_read_markers (
    user_name TEXT NOT NULL,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    message_id INTEGER NOT NULL,
    PRIMARY KEY (user_name, channel_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS dm_read_markers (
    user_name TEXT NOT NULL,
    peer TEXT NOT NULL,
    message_id INTEGER NOT NULL,
    PRIMARY KEY (user_name, peer)
) WITHOUT ROWID;
-- The unread count reads a user's inbox by recipient, which the
-- participants index (sender first) cannot serve.
CREATE INDEX IF NOT EXISTS idx_direct_messages_recipient
    ON direct_messages (recipient, sender, id);"#,
    )?;
    if !existed {
        tx.execute_batch(
            "INSERT INTO dm_read_markers (user_name, peer, message_id)
             SELECT recipient, sender, MAX(id) FROM direct_messages GROUP BY recipient, sender;",
        )?;
    }
    tx.commit()
}

/// Advance `user`'s marker in a channel. Returns whether it moved; a
/// position at or behind the stored one is not an error, just stale, and
/// neither is a channel deleted under a client that has not heard yet.
pub async fn mark_channel_read(
    db: &Db,
    user: &str,
    channel_id: i32,
    message_id: i64,
) -> Result<bool, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let changed = conn
            .prepare_cached(
                "INSERT INTO channel_read_markers (user_name, channel_id, message_id)
                 SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM channels WHERE id = ?2)
                 ON CONFLICT (user_name, channel_id) DO UPDATE SET message_id = excluded.message_id
                 WHERE excluded.message_id > channel_read_markers.message_id",
            )?
            .execute(params![user, channel_id, message_id])?;
        Ok(changed > 0)
    })
    .await
}

/// Advance `user`'s marker in the conversation with `peer`. Returns whether
/// it moved.
pub async fn mark_dm_read(
    db: &Db,
    user: &str,
    peer: &str,
    message_id: i64,
) -> Result<bool, DbError> {
    let user = user.to_owned();
    let peer = peer.to_owned();
    db.call_db(move |conn| {
        let changed = conn
            .prepare_cached(
                "INSERT INTO dm_read_markers (user_name, peer, message_id)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (user_name, peer) DO UPDATE SET message_id = excluded.message_id
                 WHERE excluded.message_id > dm_read_markers.message_id",
            )?
            .execute(params![user, peer, message_id])?;
        Ok(changed > 0)
    })
    .await
}

/// `user`'s channel markers: channel id -> newest message read.
pub async fn channel_read_markers(db: &Db, user: &str) -> Result<HashMap<i32, i64>, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        conn.prepare_cached(
            "SELECT channel_id, message_id FROM channel_read_markers WHERE user_name = ?1",
        )?
        .query_map([user], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect()
    })
    .await
}

/// Where `user` stands in each direct conversation: peer -> (newest message
/// read, messages from that peer past it). Lists every peer with a marker or
/// an unread message.
pub async fn dm_read_state(db: &Db, user: &str) -> Result<HashMap<String, (i64, i64)>, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let mut state: HashMap<String, (i64, i64)> = conn
            .prepare_cached("SELECT peer, message_id FROM dm_read_markers WHERE user_name = ?1")?
            .query_map([&user], |row| Ok((row.get(0)?, (row.get(1)?, 0))))?
            .collect::<rusqlite::Result<_>>()?;
        let unread = conn
            .prepare_cached(
                "SELECT d.sender, COUNT(*) FROM direct_messages d
                 LEFT JOIN dm_read_markers m ON m.user_name = d.recipient AND m.peer = d.sender
                 WHERE d.recipient = ?1 AND d.id > COALESCE(m.message_id, 0)
                 GROUP BY d.sender",
            )?
            .query_map([&user], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (peer, count) in unread {
            state.entry(peer).or_insert((0, 0)).1 = count;
        }
        Ok(state)
    })
    .await
}
