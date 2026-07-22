//! Persistence for per-channel permission overrides.
//!
//! Rows are keyed by `(channel_kind, channel_id, target_type, target_id)`.
//! `target_id` is a role id (as text) for role overrides and a user public key
//! for user overrides; `target_label` holds the username for the manager UI.
//! The in-memory [`OverrideSet`](crate::channel_overrides::OverrideSet) cache is
//! the hot path — these functions load it at startup and keep the DB in sync.

use rusqlite::params;

use super::{Db, DbCall, DbError};
use crate::channel_overrides::{ChannelKind, OverridePair, OverrideSet};
use crate::permissions::Permissions;

/// One persisted override row (used to seed the in-memory cache and to send the
/// manager view).
pub struct ChannelOverrideRow {
    pub kind: ChannelKind,
    pub channel_id: i32,
    pub target_type: String,
    pub target_id: String,
    pub target_label: String,
    pub allow: Permissions,
    pub deny: Permissions,
}

/// Load every override row, grouped into an [`OverrideSet`] per channel, for the
/// startup cache.
pub async fn load_all_overrides(
    db: &Db,
) -> Result<std::collections::HashMap<(ChannelKind, i32), OverrideSet>, DbError> {
    let rows = db
        .call_db(|conn| {
            let mut stmt = conn.prepare(
                "SELECT channel_kind, channel_id, target_type, target_id, target_label, allow, deny \
                 FROM channel_overrides",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;

    let mut map: std::collections::HashMap<(ChannelKind, i32), OverrideSet> =
        std::collections::HashMap::new();
    for (kind, channel_id, target_type, target_id, allow, deny) in rows {
        let Some(kind) = ChannelKind::parse(&kind) else {
            continue;
        };
        let pair = OverridePair {
            allow: allow as Permissions,
            deny: deny as Permissions,
        };
        let set = map.entry((kind, channel_id)).or_default();
        match target_type.as_str() {
            "role" => {
                if let Ok(id) = target_id.parse::<i64>() {
                    set.roles.insert(id, pair);
                }
            }
            "user" => {
                set.users.insert(target_id, pair);
            }
            "everyone" => set.everyone = pair,
            _ => {}
        }
    }
    Ok(map)
}

/// Return the override rows for one channel (for the manager view).
pub async fn get_channel_overrides(
    db: &Db,
    kind: ChannelKind,
    channel_id: i32,
) -> Result<Vec<ChannelOverrideRow>, DbError> {
    let kind_str = kind.as_str().to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT target_type, target_id, target_label, allow, deny FROM channel_overrides \
             WHERE channel_kind = ?1 AND channel_id = ?2",
        )?;
        let rows = stmt.query_map(params![kind_str, channel_id], |row| {
            Ok(ChannelOverrideRow {
                kind,
                channel_id,
                target_type: row.get(0)?,
                target_id: row.get(1)?,
                target_label: row.get(2)?,
                allow: row.get::<_, i64>(3)? as Permissions,
                deny: row.get::<_, i64>(4)? as Permissions,
            })
        })?;
        rows.collect()
    })
    .await
}

/// Insert or replace one override. `allow`/`deny` of zero still stores a row;
/// callers delete instead when both are empty.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_channel_override(
    db: &Db,
    kind: ChannelKind,
    channel_id: i32,
    target_type: &str,
    target_id: &str,
    target_label: &str,
    allow: Permissions,
    deny: Permissions,
) -> Result<(), DbError> {
    let kind = kind.as_str().to_owned();
    let target_type = target_type.to_owned();
    let target_id = target_id.to_owned();
    let target_label = target_label.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "INSERT INTO channel_overrides \
             (channel_kind, channel_id, target_type, target_id, target_label, allow, deny) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT (channel_kind, channel_id, target_type, target_id) \
             DO UPDATE SET target_label = excluded.target_label, \
                           allow = excluded.allow, deny = excluded.deny",
            params![
                kind,
                channel_id,
                target_type,
                target_id,
                target_label,
                allow as i64,
                deny as i64
            ],
        )?;
        Ok(())
    })
    .await
}

/// Delete one override target. Returns whether a row was removed.
pub async fn delete_channel_override(
    db: &Db,
    kind: ChannelKind,
    channel_id: i32,
    target_type: &str,
    target_id: &str,
) -> Result<bool, DbError> {
    let kind = kind.as_str().to_owned();
    let target_type = target_type.to_owned();
    let target_id = target_id.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute(
            "DELETE FROM channel_overrides \
             WHERE channel_kind = ?1 AND channel_id = ?2 AND target_type = ?3 AND target_id = ?4",
            params![kind, channel_id, target_type, target_id],
        )?;
        Ok(affected > 0)
    })
    .await
}

/// Remove every override for a channel (called when the channel is deleted).
pub async fn delete_overrides_for_channel(
    db: &Db,
    kind: ChannelKind,
    channel_id: i32,
) -> Result<(), DbError> {
    let kind = kind.as_str().to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "DELETE FROM channel_overrides WHERE channel_kind = ?1 AND channel_id = ?2",
            params![kind, channel_id],
        )?;
        Ok(())
    })
    .await
}
