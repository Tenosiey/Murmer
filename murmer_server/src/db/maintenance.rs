//! Destructive server-wide maintenance: purging all messages and resetting
//! the server's structure.
//!
//! Both are Danger Zone actions from the Server Dashboard, gated on
//! `ADMINISTRATOR` by the caller. They run in a single transaction so a
//! failure half-way leaves the server as it was, and they are deliberately
//! narrow about what they touch: identities, bans, mutes, emojis, sounds,
//! recorded stats and personal reminders survive a reset, because losing those
//! is never what "start over" is asked to mean.
//!
//! The **audit log survives too**, and that one is not a convenience: an
//! action that erased the record of itself would make the log worth nothing
//! precisely when somebody needs it. Both actions add an entry of their own —
//! see [`super::audit`].

use rusqlite::params;

use super::{Db, DbCall, DbError};
use crate::roles::EVERYONE_ROLE_NAME;

/// What a reset removed, for the log line and the confirmation shown to the
/// admin who asked for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResetSummary {
    pub messages: usize,
    pub channels: usize,
    pub voice_channels: usize,
    pub categories: usize,
    pub roles: usize,
}

/// Delete every message on the server, along with its reactions and pins.
/// The FTS index follows through the `messages_fts_delete` trigger.
/// Returns the number of messages deleted.
pub async fn purge_all_messages(db: &Db) -> Result<usize, DbError> {
    db.call_db(|conn| {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM reactions", [])?;
        tx.execute("DELETE FROM pins", [])?;
        let messages = tx.execute("DELETE FROM messages", [])?;
        tx.commit()?;
        Ok(messages)
    })
    .await
}

/// Reset the server's structure: every message, pin, reaction and wiki page,
/// every channel except `general`, every voice channel, category, per-channel
/// permission override, every custom role and every queued scheduled message.
///
/// Scheduled messages go because they are addressed to channels: those of a
/// deleted channel would cascade away anyway, and one still aimed at `general`
/// would post into a server that no longer resembles the one it was written
/// for. Reminders stay — they are personal notes, not server structure.
///
/// The built-in `@everyone` and Owner roles are kept on purpose — deleting
/// them would strip the administrator running the reset of the very
/// permission needed to rebuild the server, leaving nobody who can administer
/// it. Custom role *assignments* disappear with their roles through the
/// `user_roles` foreign key cascade.
pub async fn reset_server(db: &Db) -> Result<ResetSummary, DbError> {
    db.call_db(|conn| {
        let tx = conn.transaction()?;

        tx.execute("DELETE FROM reactions", [])?;
        tx.execute("DELETE FROM pins", [])?;
        let messages = tx.execute("DELETE FROM messages", [])?;
        // wiki_revisions cascade from wiki_pages; wiki_fts follows its trigger.
        tx.execute("DELETE FROM wiki_pages", [])?;
        tx.execute("DELETE FROM channel_overrides", [])?;
        tx.execute("DELETE FROM scheduled_messages", [])?;

        let channels = tx.execute("DELETE FROM channels WHERE name <> 'general'", [])?;
        // The surviving channel goes back to how a fresh server seeds it —
        // including unencrypted. Its overrides are gone by now, so leaving the
        // flag set would strand an encrypted channel that is no longer private
        // and whose messages were just deleted anyway.
        tx.execute(
            "UPDATE channels SET category_id = NULL, description = '', position = 0, e2ee = 0 \
             WHERE name = 'general'",
            [],
        )?;
        // Keys of the deleted channels cascade; general's are dropped here.
        tx.execute("DELETE FROM channel_keys", [])?;
        let voice_channels = tx.execute("DELETE FROM voice_channels", [])?;
        let categories = tx.execute("DELETE FROM categories", [])?;

        let roles = tx.execute(
            "DELETE FROM role_definitions WHERE is_default = 0 AND is_owner = 0 AND name <> ?1",
            params![EVERYONE_ROLE_NAME],
        )?;

        tx.commit()?;
        Ok(ResetSummary {
            messages,
            channels,
            voice_channels,
            categories,
            roles,
        })
    })
    .await
}
