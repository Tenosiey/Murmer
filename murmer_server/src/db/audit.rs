//! The audit log: an append-only record of who moderated whom and who
//! changed the server's shape.
//!
//! Every one of these actions used to leave a trace in `tracing` and nowhere
//! else, which put the only record on the operator's terminal — unreachable
//! for the people who hold the Server Dashboard and are the ones being asked
//! "who banned them?". A row here is that answer.
//!
//! Three properties are load-bearing:
//!
//! - **Only what happened is recorded.** Entries are written *after* the
//!   action succeeded, never on a refusal, so the log cannot be padded by
//!   spamming frames the server rejects anyway.
//! - **A failed write never fails the action.** The action already happened;
//!   losing its row is bad, but undoing a ban because the log is full would
//!   be worse. Callers go through [`crate::ws::helpers::record_audit`], which
//!   logs the failure and moves on.
//! - **The log outlives a reset.** `db::reset_server` deliberately does not
//!   touch this table, for the same reason it keeps bans and identities: the
//!   record of a reset is worth most immediately after one.
//!
//! Retention is a hard row cap ([`MAX_AUDIT_ENTRIES`]) trimmed on insert, so
//! a busy server's log cannot grow without bound on a self-hosted disk.

use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{Db, DbCall, DbError, NOW_UTC};

/// Wire names of the recorded actions. They travel to the client in the
/// `audit-log` frame, which renders a label per name and falls back to the
/// raw name for one it does not know — so an older client meeting a newer
/// server shows a less pretty row rather than a wrong one.
///
/// Mirrored in `murmer_client/src/lib/chat/audit.ts` and guarded by
/// `murmer_client/test/server-mirror.test.ts`.
pub mod actions {
    /// A member was disconnected (`kick-user`).
    pub const KICK: &str = "kick";
    /// A member was banned (`ban-user`).
    pub const BAN: &str = "ban";
    /// A ban was lifted (`unban-user`).
    pub const UNBAN: &str = "unban";
    /// A member was muted (`mute-user`).
    pub const MUTE: &str = "mute";
    /// A mute was lifted (`unmute-user`).
    pub const UNMUTE: &str = "unmute";
    /// A role was created (`create-role`).
    pub const ROLE_CREATE: &str = "role-create";
    /// A role's name, color, icon or permissions changed (`update-role`).
    pub const ROLE_UPDATE: &str = "role-update";
    /// A role was deleted (`delete-role`).
    pub const ROLE_DELETE: &str = "role-delete";
    /// The role hierarchy was reordered (`reorder-roles`).
    pub const ROLE_REORDER: &str = "role-reorder";
    /// A member's set of roles was replaced (`set-user-roles`).
    pub const USER_ROLES: &str = "user-roles";
    /// A role was added to a key through the `/role` HTTP endpoint.
    pub const ADMIN_ROLE_GRANT: &str = "admin-role-grant";
    /// A per-channel permission override was written (`set-channel-override`).
    pub const OVERRIDE_SET: &str = "override-set";
    /// A per-channel permission override was removed.
    pub const OVERRIDE_REMOVE: &str = "override-remove";
    /// Every message on the server was deleted (`purge-all-messages`).
    pub const PURGE_MESSAGES: &str = "purge-messages";
    /// The server's structure was reset (`reset-server`).
    pub const SERVER_RESET: &str = "server-reset";
}

/// Actor recorded for a change made through the `/role` HTTP endpoint, which
/// authenticates with `ADMIN_TOKEN` rather than as an account. The
/// parentheses make it unforgeable: `validate_user_name` allows only
/// alphanumerics, dashes, underscores and spaces, so no account can be named
/// this.
pub const ACTOR_ADMIN_TOKEN: &str = "(admin token)";

/// Rows kept in the log. Oldest entries beyond this are trimmed on insert:
/// the alternative — an unbounded table on somebody's home server — trades a
/// record nobody reads for disk nobody has.
pub const MAX_AUDIT_ENTRIES: i64 = 5_000;

/// Entries answered to one `get-audit-log` request. The dashboard shows a
/// scrollable list rather than pages, so this is the whole answer.
pub const AUDIT_PAGE_SIZE: i64 = 200;

pub(super) fn audit_schema() -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action TEXT NOT NULL,
    actor TEXT NOT NULL,
    target TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
"#
    )
}

/// One recorded action, newest first when listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub id: i64,
    /// One of the names in [`actions`].
    pub action: String,
    /// Account name of whoever acted, or [`ACTOR_ADMIN_TOKEN`].
    pub actor: String,
    /// What was acted on — an account name, a role name, a channel name — or
    /// empty for a server-wide action.
    pub target: String,
    /// Short human-readable summary of the change; empty when the action and
    /// target say everything.
    pub detail: String,
    pub created_at: DateTime<Utc>,
}

/// Append one entry and trim the log back to [`MAX_AUDIT_ENTRIES`].
///
/// The trim is a rowid range delete rather than an `ORDER BY … LIMIT`
/// subquery, so it costs the rows it removes and not a scan of the table.
pub async fn record_audit_entry(
    db: &Db,
    action: &str,
    actor: &str,
    target: &str,
    detail: &str,
) -> Result<(), DbError> {
    let action = action.to_owned();
    let actor = actor.to_owned();
    let target = target.to_owned();
    let detail = detail.to_owned();
    db.call_db(move |conn| {
        conn.prepare_cached(
            "INSERT INTO audit_log (action, actor, target, detail) VALUES (?1, ?2, ?3, ?4)",
        )?
        .execute(params![action, actor, target, detail])?;
        conn.prepare_cached(
            "DELETE FROM audit_log WHERE id <= (SELECT MAX(id) FROM audit_log) - ?1",
        )?
        .execute(params![MAX_AUDIT_ENTRIES])?;
        Ok(())
    })
    .await
}

/// The newest [`AUDIT_PAGE_SIZE`] entries, newest first.
///
/// Caller-gated on `VIEW_AUDIT_LOG`: the rows name who moderated whom, which
/// is not something an ordinary member is told.
pub async fn list_audit_entries(db: &Db) -> Result<Vec<AuditEntry>, DbError> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, action, actor, target, detail, created_at FROM audit_log \
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![AUDIT_PAGE_SIZE], |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    actor: row.get(2)?,
                    target: row.get(3)?,
                    detail: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}
