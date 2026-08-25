//! Server-issued invite codes and the memberships they grant.
//!
//! On a password-protected server an invite is the alternative to handing the
//! `SERVER_PASSWORD` around. A code carries an optional expiry and an
//! optional use limit, and revoking it is a row delete — so a link that leaks
//! before it is spent can be withdrawn without changing the password for
//! everybody, which is the whole reason this exists.
//!
//! Redemption is what makes a reconnect free: the redeeming public key is
//! recorded in `invite_members`, and presence admits a key with such a row
//! before it ever looks at the invite. Membership therefore outlives the
//! invite that granted it — revoking a code stops *future* joins, it does not
//! evict the people who already joined through it. Removing one of those is a
//! ban, which is bound to the same public key.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError, NOW_UTC};

/// Tables owned by this module. Kept out of `run_schema`'s big batch the way
/// the stats, wiki and soundboard schemas are.
pub fn invites_schema() -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS invites (
    code TEXT PRIMARY KEY,
    created_by TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC}),
    expires_at TEXT,
    max_uses INTEGER NOT NULL DEFAULT 0,
    uses INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS invite_members (
    public_key TEXT PRIMARY KEY,
    invite_code TEXT NOT NULL,
    joined_at TEXT NOT NULL DEFAULT ({NOW_UTC})
);
"#
    )
}

/// One invite as shown to the moderators who may manage them.
#[derive(Clone, Debug)]
pub struct InviteRecord {
    pub code: String,
    pub created_by: String,
    pub created_at: String,
    /// `None` when the invite never expires.
    pub expires_at: Option<String>,
    /// `0` means unlimited.
    pub max_uses: i64,
    pub uses: i64,
}

/// Why a redemption was refused. The handler answers all of them with the
/// same error code — a client that was told which of these it hit could probe
/// for live codes — but the log line names it, which is what an operator
/// asking "why can nobody join" needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Redemption {
    Granted,
    Unknown,
    Expired,
    Exhausted,
}

/// Store a new invite. `expires_at` is `None` for an invite that never
/// expires, `max_uses` is `0` for one with no use limit.
pub async fn create_invite(
    db: &Db,
    code: &str,
    created_by: &str,
    expires_at: Option<DateTime<Utc>>,
    max_uses: i64,
) -> Result<(), DbError> {
    let code = code.to_owned();
    let created_by = created_by.to_owned();
    let expires_at = expires_at.map(|at| at.to_rfc3339());
    db.call_db(move |conn| {
        conn.prepare_cached(
            "INSERT INTO invites (code, created_by, expires_at, max_uses) VALUES (?1, ?2, ?3, ?4)",
        )?
        .execute(params![code, created_by, expires_at, max_uses])?;
        Ok(())
    })
    .await
}

/// Every invite on the server, newest first. Manager-only information: the
/// codes are credentials.
pub async fn list_invites(db: &Db) -> Result<Vec<InviteRecord>, DbError> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT code, created_by, created_at, expires_at, max_uses, uses \
             FROM invites ORDER BY created_at DESC, code",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(InviteRecord {
                    code: row.get(0)?,
                    created_by: row.get(1)?,
                    created_at: row.get(2)?,
                    expires_at: row.get(3)?,
                    max_uses: row.get(4)?,
                    uses: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// How many invites exist, for the per-server cap.
pub async fn count_invites(db: &Db) -> Result<i64, DbError> {
    db.call_db(|conn| {
        conn.prepare_cached("SELECT COUNT(*) FROM invites")?
            .query_row([], |row| row.get(0))
    })
    .await
}

/// Withdraw an invite. The row is deleted outright rather than flagged: a
/// revoked invite has nothing left to say, and the memberships it granted
/// carry the code as plain text, so nothing depends on the row surviving.
/// Returns `true` if the code existed.
pub async fn revoke_invite(db: &Db, code: &str) -> Result<bool, DbError> {
    let code = code.to_owned();
    db.call_db(move |conn| {
        let affected = conn
            .prepare_cached("DELETE FROM invites WHERE code = ?1")?
            .execute(params![code])?;
        Ok(affected > 0)
    })
    .await
}

/// Whether this public key has already joined through some invite. Checked on
/// every presence frame before the invite itself, so that a member's
/// reconnects neither consume a use nor depend on the invite still existing.
pub async fn is_invite_member(db: &Db, public_key: &str) -> Result<bool, DbError> {
    let public_key = public_key.to_owned();
    db.call_db(move |conn| {
        let found: Option<i64> = conn
            .prepare_cached("SELECT 1 FROM invite_members WHERE public_key = ?1")?
            .query_row(params![public_key], |row| row.get(0))
            .optional()?;
        Ok(found.is_some())
    })
    .await
}

/// Whether `code` would be accepted right now. Shared by the read-only
/// [`check_invite`] and the transactional [`redeem_invite`], so the two can
/// never disagree about what "still valid" means.
fn invite_status(
    conn: &rusqlite::Connection,
    code: &str,
    now: DateTime<Utc>,
) -> rusqlite::Result<Redemption> {
    let found: Option<(Option<String>, i64, i64)> = conn
        .prepare_cached("SELECT expires_at, max_uses, uses FROM invites WHERE code = ?1")?
        .query_row(params![code], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()?;

    let Some((expires_at, max_uses, uses)) = found else {
        return Ok(Redemption::Unknown);
    };

    // A stored timestamp that no longer parses counts as expired: an invite
    // nobody can date is not one to keep honouring.
    if let Some(raw) = expires_at.as_deref() {
        let expired = DateTime::parse_from_rfc3339(raw)
            .map(|at| at.with_timezone(&Utc) <= now)
            .unwrap_or(true);
        if expired {
            return Ok(Redemption::Expired);
        }
    }

    if max_uses > 0 && uses >= max_uses {
        return Ok(Redemption::Exhausted);
    }

    Ok(Redemption::Granted)
}

/// Whether `code` is currently redeemable, without spending a use.
///
/// Presence checks this before it validates the account name and the ban
/// list, and only calls [`redeem_invite`] once those pass — so a rejected
/// connection never costs the invite one of its uses. `now` is passed in
/// rather than read from SQLite so a test can place an invite either side of
/// its expiry without waiting for it.
pub async fn check_invite(db: &Db, code: &str, now: DateTime<Utc>) -> Result<Redemption, DbError> {
    let code = code.to_owned();
    db.call_db(move |conn| invite_status(conn, &code, now))
        .await
}

/// Spend one use of `code` on behalf of `public_key`, recording the key as a
/// member. The validity check and the increment share one transaction, so two
/// clients racing for the last use of an invite cannot both get in.
pub async fn redeem_invite(
    db: &Db,
    code: &str,
    public_key: &str,
    now: DateTime<Utc>,
) -> Result<Redemption, DbError> {
    let code = code.to_owned();
    let public_key = public_key.to_owned();
    db.call_db(move |conn| {
        let tx = conn.transaction()?;

        let status = invite_status(&tx, &code, now)?;
        if status != Redemption::Granted {
            return Ok(status);
        }

        tx.prepare_cached(
            "INSERT OR IGNORE INTO invite_members (public_key, invite_code) VALUES (?1, ?2)",
        )?
        .execute(params![public_key, code])?;
        tx.prepare_cached("UPDATE invites SET uses = uses + 1 WHERE code = ?1")?
            .execute(params![code])?;

        tx.commit()?;
        Ok(Redemption::Granted)
    })
    .await
}
