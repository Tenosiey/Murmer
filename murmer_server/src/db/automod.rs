//! Auto-moderation rule persistence.
//!
//! The rules are edited in the Server Dashboard as one ordered list and saved
//! the same way, so this stores them as one: a save replaces the whole table
//! in a single transaction. That is what lets the row's `position` be its
//! primary key — there is no partial update to reconcile, and the order the
//! operator arranged is the order the matcher sees.
//!
//! The rule model, its bounds and the matching itself are
//! [`crate::automod`]; this file only moves rows. Rules are normalized on
//! read as well as on write, so a hand-edited row cannot widen a bound.

use rusqlite::params;
use tracing::warn;

use super::{Db, DbCall, DbError};
use crate::automod::{AutomodRule, RuleAction, RuleKind, normalize_rule};

pub(super) fn automod_schema() -> String {
    r#"CREATE TABLE IF NOT EXISTS automod_rules (
    position INTEGER PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    pattern TEXT NOT NULL,
    kind TEXT NOT NULL,
    action TEXT NOT NULL,
    mute_seconds INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1
);
"#
    .to_string()
}

/// Load every rule in the operator's order.
///
/// A row whose `kind` or `action` this build does not know is skipped rather
/// than guessed at: falling back to some default would apply an action the
/// operator never chose, and the two vocabularies only grow.
pub async fn automod_rules(db: &Db) -> Result<Vec<AutomodRule>, DbError> {
    db.call_db(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT name, pattern, kind, action, mute_seconds, enabled FROM automod_rules \
             ORDER BY position ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let kind: String = row.get(2)?;
                let action: String = row.get(3)?;
                Ok((
                    AutomodRule {
                        name: row.get(0)?,
                        pattern: row.get(1)?,
                        kind: RuleKind::Word,
                        action: RuleAction::Warn,
                        mute_seconds: row.get(4)?,
                        enabled: row.get::<_, i64>(5)? != 0,
                    },
                    kind,
                    action,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut rules = Vec::with_capacity(rows.len());
        for (rule, kind, action) in rows {
            match (RuleKind::parse(&kind), RuleAction::parse(&action)) {
                (Some(kind), Some(action)) => rules.push(normalize_rule(&AutomodRule {
                    kind,
                    action,
                    ..rule
                })),
                _ => warn!(
                    kind,
                    action, "Skipping auto-moderation rule of unknown type"
                ),
            }
        }
        Ok(rules)
    })
    .await
}

/// Replace the stored rules with `rules` (Owner/Admin action, validated by
/// the caller). Returns the normalized rules that were actually written.
pub async fn set_automod_rules(
    db: &Db,
    rules: &[AutomodRule],
) -> Result<Vec<AutomodRule>, DbError> {
    let stored: Vec<AutomodRule> = rules.iter().map(normalize_rule).collect();
    let to_write = stored.clone();
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM automod_rules", [])?;
        for (position, rule) in to_write.iter().enumerate() {
            tx.execute(
                "INSERT INTO automod_rules \
                 (position, name, pattern, kind, action, mute_seconds, enabled) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    position as i64,
                    rule.name,
                    rule.pattern,
                    rule.kind.as_str(),
                    rule.action.as_str(),
                    rule.mute_seconds,
                    rule.enabled as i64,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await?;
    Ok(stored)
}
