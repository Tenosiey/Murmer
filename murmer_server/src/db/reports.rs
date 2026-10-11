//! Message reports: a member flagging a message to the moderators.
//!
//! A report stores who flagged what and why, never a copy of the message:
//! the text is read from `messages` when a moderator lists the queue. A copy
//! would outlive the message's deletion, retention and the Danger Zone
//! purge, which are exactly the moments the operator expects it gone. A
//! report whose message is gone therefore shows no text, and the moderator
//! dismisses it.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError, NOW_UTC};

/// Tables owned by this module. One report per message and reporter, so
/// flagging the same message twice is a no-op rather than a louder report.
pub fn reports_schema() -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS message_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id INTEGER NOT NULL,
    channel_id INTEGER NOT NULL,
    author TEXT NOT NULL,
    reporter TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC}),
    UNIQUE (message_id, reporter)
);
"#
    )
}

/// One open report, with the reported message's stored JSON when it still
/// exists.
#[derive(Clone, Debug)]
pub struct ReportRecord {
    pub id: i64,
    pub message_id: i64,
    pub channel_id: i32,
    pub author: String,
    pub reporter: String,
    pub reason: String,
    pub created_at: String,
    /// `None` once the message itself has been deleted.
    pub content: Option<String>,
}

/// File a report. Returns `false` when this reporter already reported the
/// message.
pub async fn add_report(
    db: &Db,
    message_id: i64,
    channel_id: i32,
    author: &str,
    reporter: &str,
    reason: &str,
) -> Result<bool, DbError> {
    let (author, reporter, reason) = (author.to_owned(), reporter.to_owned(), reason.to_owned());
    db.call_db(move |conn| {
        let inserted = conn
            .prepare_cached(
                "INSERT OR IGNORE INTO message_reports \
                 (message_id, channel_id, author, reporter, reason) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?
            .execute(params![message_id, channel_id, author, reporter, reason])?;
        Ok(inserted > 0)
    })
    .await
}

/// The oldest `limit` open reports, oldest first: a queue is worked from the
/// front.
pub async fn list_reports(db: &Db, limit: i64) -> Result<Vec<ReportRecord>, DbError> {
    db.call_db(move |conn| {
        let rows = conn
            .prepare_cached(
                "SELECT r.id, r.message_id, r.channel_id, r.author, r.reporter, r.reason, \
                 r.created_at, m.content FROM message_reports r \
                 LEFT JOIN messages m ON m.id = r.message_id ORDER BY r.id LIMIT ?1",
            )?
            .query_map(params![limit], |row| {
                Ok(ReportRecord {
                    id: row.get(0)?,
                    message_id: row.get(1)?,
                    channel_id: row.get(2)?,
                    author: row.get(3)?,
                    reporter: row.get(4)?,
                    reason: row.get(5)?,
                    created_at: row.get(6)?,
                    content: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// The channel a report belongs to, if the report exists.
pub async fn report_channel(db: &Db, id: i64) -> Result<Option<i32>, DbError> {
    db.call_db(move |conn| {
        let channel = conn
            .prepare_cached("SELECT channel_id FROM message_reports WHERE id = ?1")?
            .query_row(params![id], |row| row.get(0))
            .optional()?;
        Ok(channel)
    })
    .await
}

/// Close a report. Returns `true` if a row was removed.
pub async fn delete_report(db: &Db, id: i64) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let affected = conn
            .prepare_cached("DELETE FROM message_reports WHERE id = ?1")?
            .execute(params![id])?;
        Ok(affected > 0)
    })
    .await
}
