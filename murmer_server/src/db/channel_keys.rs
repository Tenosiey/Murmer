//! Persistence for the wrapped keys of end-to-end encrypted channels.
//!
//! An encrypted channel has a 32-byte symmetric key that is generated on a
//! member's machine and never reaches the server in the clear. Every row here
//! holds that key sealed with `nacl.box` for exactly one recipient — the
//! X25519 key derived from their Ed25519 identity key — so the server keeps N
//! opaque blobs per epoch and can open none of them.
//!
//! Keys are versioned by `epoch`, counting up from 1. Admitting a member only
//! adds rows to the epochs they should be able to read; removing one starts a
//! fresh epoch, and *not* re-wrapping the old key is what stops the removed
//! member from reading anything sent afterwards. Old epochs are kept so
//! members who were there at the time can still open the history.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};

/// One channel key sealed for a single recipient.
pub struct WrappedChannelKey {
    pub epoch: i64,
    /// Base64 Ed25519 identity key of the member this wrap is for.
    pub recipient_key: String,
    /// Base64 Ed25519 identity key of the member who wrapped it. The recipient
    /// needs it to derive the same box shared secret.
    pub sender_key: String,
    /// Base64 24-byte box nonce.
    pub nonce: String,
    /// Base64 box ciphertext over the 32-byte channel key.
    pub wrapped_key: String,
}

/// One key wrap as submitted by a client, before it is stored.
pub struct ChannelKeyEntry {
    pub recipient_key: String,
    pub nonce: String,
    pub wrapped_key: String,
}

/// Outcome of a `put-channel-keys` write.
#[derive(Debug, PartialEq, Eq)]
pub enum ChannelKeyWrite {
    /// Rows were accepted; carries how many were actually inserted (wraps that
    /// already existed are kept, never overwritten).
    Stored(usize),
    /// The requested epoch is neither the next one nor an existing one — the
    /// client raced another member and must re-read the current state.
    EpochConflict,
    /// The author does not hold a wrap at an existing epoch they tried to
    /// extend, so they cannot be in possession of that epoch's key.
    NotAKeyHolder,
}

/// Every wrap addressed to one member in a channel, oldest epoch first. This
/// is the whole key material a client needs to read the channel's history.
pub async fn get_channel_keys_for(
    db: &Db,
    channel_id: i32,
    recipient_key: &str,
) -> Result<Vec<WrappedChannelKey>, DbError> {
    let recipient_key = recipient_key.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT epoch, recipient_key, sender_key, nonce, wrapped_key FROM channel_keys \
             WHERE channel_id = ?1 AND recipient_key = ?2 ORDER BY epoch",
        )?;
        let rows = stmt.query_map(params![channel_id, recipient_key], |row| {
            Ok(WrappedChannelKey {
                epoch: row.get(0)?,
                recipient_key: row.get(1)?,
                sender_key: row.get(2)?,
                nonce: row.get(3)?,
                wrapped_key: row.get(4)?,
            })
        })?;
        rows.collect()
    })
    .await
}

/// The channel's current key epoch, or `None` when no key exists yet.
pub async fn latest_channel_epoch(db: &Db, channel_id: i32) -> Result<Option<i64>, DbError> {
    db.call_db(move |conn| {
        conn.query_row(
            "SELECT MAX(epoch) FROM channel_keys WHERE channel_id = ?1",
            params![channel_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map(Option::flatten)
    })
    .await
}

/// Identity keys that hold a wrap at one epoch. Clients compare this against
/// the channel's member roster to decide whether to hand out the current key
/// or to rotate to a new epoch.
pub async fn channel_epoch_recipients(
    db: &Db,
    channel_id: i32,
    epoch: i64,
) -> Result<Vec<String>, DbError> {
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT recipient_key FROM channel_keys WHERE channel_id = ?1 AND epoch = ?2",
        )?;
        let rows = stmt.query_map(params![channel_id, epoch], |row| row.get(0))?;
        rows.collect()
    })
    .await
}

/// Store wraps for one epoch of a channel key.
///
/// Two writes are legal and nothing else: opening the *next* epoch (which any
/// member may do, since rotating is how a removal takes effect), and adding
/// recipients to an epoch the author demonstrably holds — proven by their own
/// wrap already being stored there. Existing rows are never overwritten, so a
/// member cannot replace another member's wrap with one sealed to a key they
/// control; the first wrap for a recipient at an epoch is final.
pub async fn insert_channel_keys(
    db: &Db,
    channel_id: i32,
    epoch: i64,
    author_key: &str,
    entries: Vec<ChannelKeyEntry>,
) -> Result<ChannelKeyWrite, DbError> {
    let author_key = author_key.to_owned();
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let latest: Option<i64> = tx.query_row(
            "SELECT MAX(epoch) FROM channel_keys WHERE channel_id = ?1",
            params![channel_id],
            |row| row.get(0),
        )?;

        let opening_new = match latest {
            None => epoch == 1,
            Some(current) => epoch == current + 1,
        };
        if !opening_new {
            let extends_existing = latest.is_some_and(|current| epoch >= 1 && epoch <= current);
            if !extends_existing {
                return Ok(ChannelKeyWrite::EpochConflict);
            }
            let holds: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM channel_keys \
                 WHERE channel_id = ?1 AND epoch = ?2 AND recipient_key = ?3)",
                params![channel_id, epoch, author_key],
                |row| row.get(0),
            )?;
            if !holds {
                return Ok(ChannelKeyWrite::NotAKeyHolder);
            }
        }

        let mut stored = 0usize;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO channel_keys \
                 (channel_id, epoch, recipient_key, sender_key, nonce, wrapped_key) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for entry in &entries {
                stored += stmt.execute(params![
                    channel_id,
                    epoch,
                    entry.recipient_key,
                    author_key,
                    entry.nonce,
                    entry.wrapped_key
                ])?;
            }
        }
        tx.commit()?;
        Ok(ChannelKeyWrite::Stored(stored))
    })
    .await
}

/// Drop every key of a channel. Called when the channel is deleted and when
/// encryption is switched off, so a disabled channel keeps no key material.
pub async fn delete_channel_keys(db: &Db, channel_id: i32) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute(
            "DELETE FROM channel_keys WHERE channel_id = ?1",
            params![channel_id],
        )?;
        Ok(())
    })
    .await
}
