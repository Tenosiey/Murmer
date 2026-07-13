//! Role persistence operations.

use rusqlite::params;

use super::{Db, DbCall, DbError};

/// Get the role for a user by public key, if any.
pub async fn get_role(db: &Db, key: &str) -> Option<(String, Option<String>)> {
    let key = key.to_owned();
    db.call_db(move |conn| {
        let row = conn
            .query_row(
                "SELECT role, color FROM roles WHERE public_key = ?1",
                params![key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        Ok(row)
    })
    .await
    .ok()
    .flatten()
}

/// Insert or update a user's role.
pub async fn set_role(db: &Db, key: &str, role: &str, color: Option<&str>) -> Result<(), DbError> {
    let key = key.to_owned();
    let role = role.to_owned();
    let color = color.map(str::to_owned);
    db.call_db(move |conn| {
        conn.execute(
            "INSERT INTO roles (public_key, role, color) VALUES (?1, ?2, ?3) \
             ON CONFLICT (public_key) DO UPDATE SET role = excluded.role, color = excluded.color",
            params![key, role, color],
        )?;
        Ok(())
    })
    .await
}

/// Remove a user's role by public key.
pub async fn remove_role(db: &Db, key: &str) -> Result<bool, DbError> {
    let key = key.to_owned();
    db.call_db(move |conn| {
        let affected = conn.execute("DELETE FROM roles WHERE public_key = ?1", params![key])?;
        Ok(affected > 0)
    })
    .await
}
