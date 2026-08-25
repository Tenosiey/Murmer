//! Reminders and scheduled messages: the two things this server holds on a
//! user's behalf and acts on later, without them being connected.
//!
//! Both tables are queues the scheduler drains (see
//! [`crate::ws::handlers`]), and the difference between them is what a lost
//! row costs. A reminder that never fires is a broken promise the user can
//! see; a scheduled message that fires *twice* posts words to a channel that
//! were only written once. That asymmetry decides the two claim shapes below:
//!
//! - A reminder is claimed by stamping `fired_at` and **kept** until its owner
//!   dismisses it, so one that came due while they were offline is still
//!   waiting when they reconnect.
//! - A scheduled message is claimed by stamping `claimed_at`, which takes it
//!   out of reach of every later tick *before* anything is published. Only a
//!   delivery that actually happened deletes the row. A process that dies
//!   between the two leaves a claimed row behind, which
//!   [`fail_claimed_scheduled_messages`] turns into a visible failure at the
//!   next startup rather than into a silent second post.

use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{Db, DbCall, DbError, NOW_UTC};

/// A pending or failed scheduled message, as its author sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledMessage {
    pub id: i64,
    pub channel_id: i32,
    /// The finished `chat` frame, screened and sealed when it was composed.
    pub body: String,
    pub scheduled_for: String,
    pub created_at: String,
    /// `None` while the message is still waiting; an error code once a
    /// delivery attempt was refused.
    pub failed_reason: Option<String>,
}

/// A scheduled message claimed by the scheduler and owed a delivery attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedScheduledMessage {
    pub id: i64,
    pub user_name: String,
    pub channel_id: i32,
    pub body: String,
}

/// A reminder as its owner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    pub id: i64,
    pub text: String,
    pub remind_at: String,
    /// Channel the reminder points back at, when it was set from a message.
    pub channel_id: Option<i32>,
    pub message_id: Option<i64>,
    pub created_at: String,
    /// When it came due, or `None` while it is still in the future.
    pub fired_at: Option<String>,
}

/// A reminder the scheduler has just marked due, addressed to `user_name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueReminder {
    pub id: i64,
    pub user_name: String,
    pub text: String,
    pub remind_at: String,
    pub channel_id: Option<i32>,
    pub message_id: Option<i64>,
}

pub(super) fn scheduled_schema() -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS scheduled_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_name TEXT NOT NULL,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    scheduled_for TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC}),
    claimed_at TEXT,
    failed_reason TEXT
);
-- The claim queries are the only ones that run on a timer; the per-user
-- lists are bounded by the per-user caps and stay small.
CREATE INDEX IF NOT EXISTS idx_scheduled_messages_due
    ON scheduled_messages (scheduled_for);
CREATE INDEX IF NOT EXISTS idx_scheduled_messages_user
    ON scheduled_messages (user_name);
CREATE TABLE IF NOT EXISTS reminders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_name TEXT NOT NULL,
    text TEXT NOT NULL,
    remind_at TEXT NOT NULL,
    channel_id INTEGER,
    message_id INTEGER,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC}),
    fired_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_reminders_due ON reminders (remind_at);
CREATE INDEX IF NOT EXISTS idx_reminders_user ON reminders (user_name);
"#
    )
}

/// Store a scheduled message for `user`. Returns `None` when the user is
/// already holding `max_pending` rows — failed ones included, because they
/// still occupy a slot until their author clears them.
pub async fn insert_scheduled_message(
    db: &Db,
    user: &str,
    channel_id: i32,
    body: &str,
    scheduled_for: DateTime<Utc>,
    max_pending: i64,
) -> Result<Option<i64>, DbError> {
    let user = user.to_owned();
    let body = body.to_owned();
    let scheduled_for = scheduled_for.to_rfc3339();
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM scheduled_messages WHERE user_name = ?1",
            params![user],
            |row| row.get(0),
        )?;
        if count >= max_pending {
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO scheduled_messages (user_name, channel_id, body, scheduled_for) \
             VALUES (?1, ?2, ?3, ?4)",
            params![user, channel_id, body, scheduled_for],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Some(id))
    })
    .await
}

/// Every scheduled message `user` still owns, soonest first.
pub async fn get_scheduled_messages(db: &Db, user: &str) -> Result<Vec<ScheduledMessage>, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, channel_id, body, scheduled_for, created_at, failed_reason \
             FROM scheduled_messages WHERE user_name = ?1 ORDER BY scheduled_for ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![user], |row| {
            Ok(ScheduledMessage {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                body: row.get(2)?,
                scheduled_for: row.get(3)?,
                created_at: row.get(4)?,
                failed_reason: row.get(5)?,
            })
        })?;
        rows.collect()
    })
    .await
}

/// Drop one of `user`'s scheduled messages. Ownership is part of the
/// statement rather than a check before it, so there is no window in which the
/// row could change between the two.
///
/// A row the scheduler has already claimed is deliberately still cancellable:
/// the claim only means "a delivery attempt is in flight", so cancelling races
/// that attempt at worst — never a message that is already posted.
pub async fn cancel_scheduled_message(db: &Db, user: &str, id: i64) -> Result<bool, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let removed = conn.execute(
            "DELETE FROM scheduled_messages WHERE id = ?1 AND user_name = ?2",
            params![id, user],
        )?;
        Ok(removed > 0)
    })
    .await
}

/// Take ownership of every scheduled message that is due, in one statement.
///
/// Stamping `claimed_at` is what makes delivery at-most-once: every later tick
/// skips a claimed row, and nothing is published until the caller holds the
/// row. The caller owes each claimed row either a delete or a
/// [`fail_scheduled_message`].
pub async fn claim_due_scheduled_messages(
    db: &Db,
    now: DateTime<Utc>,
) -> Result<Vec<ClaimedScheduledMessage>, DbError> {
    let now = now.to_rfc3339();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "UPDATE scheduled_messages SET claimed_at = ?1 \
             WHERE claimed_at IS NULL AND failed_reason IS NULL AND scheduled_for <= ?1 \
             RETURNING id, user_name, channel_id, body",
        )?;
        let rows = stmt.query_map(params![now], |row| {
            Ok(ClaimedScheduledMessage {
                id: row.get(0)?,
                user_name: row.get(1)?,
                channel_id: row.get(2)?,
                body: row.get(3)?,
            })
        })?;
        rows.collect()
    })
    .await
}

/// Remove a scheduled message that was delivered.
pub async fn delete_scheduled_message(db: &Db, id: i64) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute("DELETE FROM scheduled_messages WHERE id = ?1", params![id])?;
        Ok(())
    })
    .await
}

/// Record that a claimed message could not be delivered, keeping the row so
/// its author finds out. The claim is released at the same time, but
/// `failed_reason` keeps the claim query from picking it up again — a refused
/// message is something to be told about, not something to retry every ten
/// seconds until the permission comes back.
pub async fn fail_scheduled_message(db: &Db, id: i64, reason: &str) -> Result<(), DbError> {
    let reason = reason.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "UPDATE scheduled_messages SET claimed_at = NULL, failed_reason = ?2 WHERE id = ?1",
            params![id, reason],
        )?;
        Ok(())
    })
    .await
}

/// Turn rows left claimed by a process that died mid-delivery into visible
/// failures. Runs once at startup, before the scheduler starts ticking.
///
/// A claimed row cannot simply be re-queued: the previous process may have
/// posted the message before it went down, and re-queueing would post it a
/// second time. Handing the author a failure they can act on is the honest
/// answer. Returns how many rows were marked.
pub async fn fail_claimed_scheduled_messages(db: &Db, reason: &str) -> Result<usize, DbError> {
    let reason = reason.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "UPDATE scheduled_messages SET claimed_at = NULL, failed_reason = ?1 \
             WHERE claimed_at IS NOT NULL AND failed_reason IS NULL",
            params![reason],
        )
    })
    .await
}

/// Store a reminder for `user`. Returns `None` at the per-user cap.
pub async fn insert_reminder(
    db: &Db,
    user: &str,
    text: &str,
    remind_at: DateTime<Utc>,
    target: Option<(i32, i64)>,
    max_pending: i64,
) -> Result<Option<i64>, DbError> {
    let user = user.to_owned();
    let text = text.to_owned();
    let remind_at = remind_at.to_rfc3339();
    let channel_id = target.map(|(channel, _)| channel);
    let message_id = target.map(|(_, message)| message);
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM reminders WHERE user_name = ?1",
            params![user],
            |row| row.get(0),
        )?;
        if count >= max_pending {
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO reminders (user_name, text, remind_at, channel_id, message_id) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user, text, remind_at, channel_id, message_id],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Some(id))
    })
    .await
}

/// Every reminder `user` still owns — due ones included, since a reminder
/// stays in the list until it is dismissed.
pub async fn get_reminders(db: &Db, user: &str) -> Result<Vec<Reminder>, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, text, remind_at, channel_id, message_id, created_at, fired_at \
             FROM reminders WHERE user_name = ?1 ORDER BY remind_at ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![user], |row| {
            Ok(Reminder {
                id: row.get(0)?,
                text: row.get(1)?,
                remind_at: row.get(2)?,
                channel_id: row.get(3)?,
                message_id: row.get(4)?,
                created_at: row.get(5)?,
                fired_at: row.get(6)?,
            })
        })?;
        rows.collect()
    })
    .await
}

/// Dismiss one of `user`'s reminders, due or not. Ownership is in the
/// statement for the same reason as in [`cancel_scheduled_message`].
pub async fn cancel_reminder(db: &Db, user: &str, id: i64) -> Result<bool, DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        let removed = conn.execute(
            "DELETE FROM reminders WHERE id = ?1 AND user_name = ?2",
            params![id, user],
        )?;
        Ok(removed > 0)
    })
    .await
}

/// Mark every reminder that has come due and return them, so each owner can be
/// told once. The row survives the claim: an owner who was offline finds it
/// waiting, already marked due, on their next connection.
pub async fn claim_due_reminders(db: &Db, now: DateTime<Utc>) -> Result<Vec<DueReminder>, DbError> {
    let now = now.to_rfc3339();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "UPDATE reminders SET fired_at = ?1 \
             WHERE fired_at IS NULL AND remind_at <= ?1 \
             RETURNING id, user_name, text, remind_at, channel_id, message_id",
        )?;
        let rows = stmt.query_map(params![now], |row| {
            Ok(DueReminder {
                id: row.get(0)?,
                user_name: row.get(1)?,
                text: row.get(2)?,
                remind_at: row.get(3)?,
                channel_id: row.get(4)?,
                message_id: row.get(5)?,
            })
        })?;
        rows.collect()
    })
    .await
}
