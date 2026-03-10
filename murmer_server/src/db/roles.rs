//! Role persistence operations.

use tokio_postgres::Client;

/// Get the role for a user by public key, if any.
pub async fn get_role(db: &Client, key: &str) -> Option<(String, Option<String>)> {
    db.query_opt(
        "SELECT role, color FROM roles WHERE public_key = $1",
        &[&key],
    )
    .await
    .ok()
    .flatten()
    .map(|row| (row.get(0), row.get(1)))
}

/// Insert or update a user's role.
pub async fn set_role(
    db: &Client,
    key: &str,
    role: &str,
    color: Option<&str>,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO roles (public_key, role, color) VALUES ($1, $2, $3) \
        ON CONFLICT (public_key) DO UPDATE SET role = EXCLUDED.role, color = EXCLUDED.color",
        &[&key, &role, &color],
    )
    .await
    .map(|_| ())
}

/// Remove a user's role by public key.
pub async fn remove_role(db: &Client, key: &str) -> Result<bool, tokio_postgres::Error> {
    db.execute("DELETE FROM roles WHERE public_key = $1", &[&key])
        .await
        .map(|affected| affected > 0)
}
