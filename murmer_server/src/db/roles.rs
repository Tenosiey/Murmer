//! Role persistence: role definitions and per-user role assignments.
//!
//! Two tables back the role system:
//! - `role_definitions` — one row per role (name, color, permission bitmask,
//!   hierarchy position and the protected `is_default`/`is_owner` flags).
//! - `user_roles` — a many-to-many map of `public_key` → `role_id`; a user's
//!   effective permissions are the union of their assigned roles plus the
//!   default `@everyone` role.
//!
//! Legacy databases used a single `roles(public_key, role, color)` table with
//! one free-text role per user. [`migrate_roles`] backfills the new tables
//! from it once, guarded by a marker in `server_settings`.

use rusqlite::{OptionalExtension, params};

use super::{Db, DbCall, DbError};
use crate::permissions::Permissions;
use crate::roles::{BUILTIN_ROLES, RoleDef};

/// Build a [`RoleDef`] from a `role_definitions` row in column order
/// `id, name, color, permissions, position, is_default, is_owner`.
fn row_to_def(row: &rusqlite::Row) -> rusqlite::Result<RoleDef> {
    let permissions: i64 = row.get(3)?;
    Ok(RoleDef {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        permissions: permissions as Permissions,
        position: row.get(4)?,
        is_default: row.get::<_, i64>(5)? != 0,
        is_owner: row.get::<_, i64>(6)? != 0,
    })
}

const SELECT_DEF: &str =
    "SELECT id, name, color, permissions, position, is_default, is_owner FROM role_definitions";

/// Load every role definition, ordered by ascending position.
pub async fn list_role_defs(db: &Db) -> Result<Vec<RoleDef>, DbError> {
    db.call_db(move |conn| {
        let mut stmt = conn.prepare(&format!("{SELECT_DEF} ORDER BY position ASC, id ASC"))?;
        let rows = stmt.query_map([], row_to_def)?;
        rows.collect()
    })
    .await
}

/// Fetch a single role definition by id.
pub async fn get_role_def(db: &Db, id: i64) -> Result<Option<RoleDef>, DbError> {
    db.call_db(move |conn| {
        conn.query_row(
            &format!("{SELECT_DEF} WHERE id = ?1"),
            params![id],
            row_to_def,
        )
        .optional()
    })
    .await
}

/// Fetch a role definition by case-insensitive name.
pub async fn get_role_def_by_name(db: &Db, name: &str) -> Result<Option<RoleDef>, DbError> {
    let name = name.to_owned();
    db.call_db(move |conn| {
        conn.query_row(
            &format!("{SELECT_DEF} WHERE name = ?1 COLLATE NOCASE"),
            params![name],
            row_to_def,
        )
        .optional()
    })
    .await
}

/// Insert a new role definition and return its id. Custom roles are never
/// default or owner roles.
pub async fn create_role_def(
    db: &Db,
    name: &str,
    color: Option<&str>,
    permissions: Permissions,
    position: i64,
) -> Result<i64, DbError> {
    let name = name.to_owned();
    let color = color.map(str::to_owned);
    db.call_db(move |conn| {
        conn.execute(
            "INSERT INTO role_definitions (name, color, permissions, position, is_default, is_owner) \
             VALUES (?1, ?2, ?3, ?4, 0, 0)",
            params![name, color, permissions as i64, position],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
}

/// Update a role's name, color and permission mask.
pub async fn update_role_def(
    db: &Db,
    id: i64,
    name: &str,
    color: Option<&str>,
    permissions: Permissions,
) -> Result<(), DbError> {
    let name = name.to_owned();
    let color = color.map(str::to_owned);
    db.call_db(move |conn| {
        conn.execute(
            "UPDATE role_definitions SET name = ?2, color = ?3, permissions = ?4 WHERE id = ?1",
            params![id, name, color, permissions as i64],
        )?;
        Ok(())
    })
    .await
}

/// Set a role's hierarchy position.
pub async fn set_role_position(db: &Db, id: i64, position: i64) -> Result<(), DbError> {
    db.call_db(move |conn| {
        conn.execute(
            "UPDATE role_definitions SET position = ?2 WHERE id = ?1",
            params![id, position],
        )?;
        Ok(())
    })
    .await
}

/// Delete a role definition. Assignments referencing it are removed by the
/// `ON DELETE CASCADE` on `user_roles`. Returns whether a row was deleted.
pub async fn delete_role_def(db: &Db, id: i64) -> Result<bool, DbError> {
    db.call_db(move |conn| {
        let affected = conn.execute("DELETE FROM role_definitions WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    })
    .await
}

/// All role ids assigned to a public key.
pub async fn get_user_role_ids(db: &Db, key: &str) -> Result<Vec<i64>, DbError> {
    let key = key.to_owned();
    db.call_db(move |conn| {
        let mut stmt =
            conn.prepare("SELECT role_id FROM user_roles WHERE public_key = ?1 ORDER BY role_id")?;
        let rows = stmt.query_map(params![key], |row| row.get::<_, i64>(0))?;
        rows.collect()
    })
    .await
}

/// Replace the full set of roles assigned to a key. `role_ids` are inserted
/// verbatim; callers validate them first. Runs in a transaction so a key never
/// observes a partially-applied assignment.
pub async fn set_user_roles(db: &Db, key: &str, role_ids: &[i64]) -> Result<(), DbError> {
    let key = key.to_owned();
    let role_ids = role_ids.to_vec();
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM user_roles WHERE public_key = ?1", params![key])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO user_roles (public_key, role_id) VALUES (?1, ?2)",
            )?;
            for id in &role_ids {
                stmt.execute(params![key, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    })
    .await
}

/// Add a single role to a key (no-op if already assigned).
pub async fn add_user_role(db: &Db, key: &str, role_id: i64) -> Result<(), DbError> {
    let key = key.to_owned();
    db.call_db(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO user_roles (public_key, role_id) VALUES (?1, ?2)",
            params![key, role_id],
        )?;
        Ok(())
    })
    .await
}

/// Find a role definition by name, creating a baseline custom role (positioned
/// just below Owner) if none exists, then assign it to `key`. Returns the
/// resolved definition so callers can update in-memory state and broadcast it.
///
/// Used by the token-gated `/role` HTTP endpoint and the `set-role` CLI to
/// bootstrap roles (notably the first Owner) without the dashboard.
pub async fn assign_named_role(
    db: &Db,
    key: &str,
    name: &str,
    color: Option<&str>,
) -> Result<RoleDef, DbError> {
    let key = key.to_owned();
    let name = name.to_owned();
    let color = color.map(str::to_owned);
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let existing = tx
            .query_row(
                &format!("{SELECT_DEF} WHERE name = ?1 COLLATE NOCASE"),
                params![name],
                row_to_def,
            )
            .optional()?;
        let def = match existing {
            Some(def) => def,
            None => {
                let position: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(position), 0) + 1 FROM role_definitions WHERE is_owner = 0",
                    [],
                    |row| row.get(0),
                )?;
                let permissions = crate::permissions::DEFAULT_EVERYONE;
                tx.execute(
                    "INSERT INTO role_definitions \
                     (name, color, permissions, position, is_default, is_owner) \
                     VALUES (?1, ?2, ?3, ?4, 0, 0)",
                    params![name, color, permissions as i64, position],
                )?;
                RoleDef {
                    id: tx.last_insert_rowid(),
                    name: name.clone(),
                    color: color.clone(),
                    permissions,
                    position,
                    is_default: false,
                    is_owner: false,
                }
            }
        };
        tx.execute(
            "INSERT OR IGNORE INTO user_roles (public_key, role_id) VALUES (?1, ?2)",
            params![key, def.id],
        )?;
        tx.commit()?;
        Ok(def)
    })
    .await
}

/// One-time migration/seed. Idempotent: guarded by a `server_settings` marker
/// so it runs at most once even though `run_schema` runs on every startup.
///
/// Seeds the built-in roles, converts any legacy `roles` rows into role
/// definitions plus `user_roles` assignments, and records the marker.
pub fn migrate_roles(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let already: i64 = conn.query_row(
        "SELECT COUNT(*) FROM server_settings WHERE key = 'roles_v2'",
        [],
        |row| row.get(0),
    )?;
    if already > 0 {
        return Ok(());
    }

    // Seed the built-in roles. Names are unique; INSERT OR IGNORE is safe here
    // because this whole block runs only once.
    for role in BUILTIN_ROLES {
        conn.execute(
            "INSERT OR IGNORE INTO role_definitions \
             (name, color, permissions, position, is_default, is_owner) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                role.name,
                role.color,
                role.permissions as i64,
                role.position,
                role.is_default as i64,
                role.is_owner as i64,
            ],
        )?;
    }

    // Backfill from the legacy single-role table if it exists.
    let legacy_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'roles'",
        [],
        |row| row.get(0),
    )?;
    if legacy_exists > 0 {
        // Highest position among built-ins that custom legacy roles slot below
        // the Owner (kept at the top). Built-in Owner is position 3.
        let mut next_position: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), 0) FROM role_definitions WHERE is_owner = 0",
            [],
            |row| row.get(0),
        )?;

        // Create a definition for every distinct legacy role name that is not
        // already a built-in. Legacy custom roles carried no special powers, so
        // they inherit only the baseline permissions.
        let legacy_names: Vec<(String, Option<String>)> = {
            let mut stmt = conn.prepare("SELECT DISTINCT role, color FROM roles ORDER BY role")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (name, color) in &legacy_names {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM role_definitions WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |row| row.get(0),
            )?;
            if exists == 0 {
                next_position += 1;
                conn.execute(
                    "INSERT INTO role_definitions \
                     (name, color, permissions, position, is_default, is_owner) \
                     VALUES (?1, ?2, ?3, ?4, 0, 0)",
                    params![
                        name,
                        color,
                        crate::permissions::DEFAULT_EVERYONE as i64,
                        next_position,
                    ],
                )?;
            }
        }

        // Map each legacy assignment (public_key → role name) onto the new
        // user_roles table by resolving the name to its definition id.
        conn.execute(
            "INSERT OR IGNORE INTO user_roles (public_key, role_id) \
             SELECT r.public_key, d.id \
             FROM roles r \
             JOIN role_definitions d ON d.name = r.role COLLATE NOCASE",
            [],
        )?;
    }

    conn.execute(
        "INSERT OR IGNORE INTO server_settings (key, value) VALUES ('roles_v2', '1')",
        [],
    )?;
    Ok(())
}
