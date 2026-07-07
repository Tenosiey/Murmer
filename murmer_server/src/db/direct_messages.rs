//! Direct message persistence between pairs of users.
//!
//! Messages are stored as opaque JSON text like channel messages, with the
//! participants duplicated into indexed columns so conversations can be
//! queried without parsing the payload.

use tokio_postgres::Client;

/// Insert a direct message and return its id.
pub async fn insert_direct_message(
    db: &Client,
    sender: &str,
    recipient: &str,
    content: &str,
) -> Result<i64, tokio_postgres::Error> {
    let row = db
        .query_one(
            "INSERT INTO direct_messages (sender, recipient, content) VALUES ($1, $2, $3) RETURNING id::bigint",
            &[&sender, &recipient, &content],
        )
        .await?;
    Ok(row.get(0))
}

/// Fetch messages exchanged between two users, newest first.
pub async fn fetch_dm_history(
    db: &Client,
    user_a: &str,
    user_b: &str,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<(i64, String)>, tokio_postgres::Error> {
    let rows = if let Some(before) = before {
        let before32 = match i32::try_from(before) {
            Ok(value) => value,
            Err(_) => return Ok(Vec::new()),
        };
        db.query(
            "SELECT id::bigint, content FROM direct_messages \
             WHERE ((sender = $1 AND recipient = $2) OR (sender = $2 AND recipient = $1)) \
             AND id < $3 ORDER BY id DESC LIMIT $4",
            &[&user_a, &user_b, &before32, &limit],
        )
        .await?
    } else {
        db.query(
            "SELECT id::bigint, content FROM direct_messages \
             WHERE (sender = $1 AND recipient = $2) OR (sender = $2 AND recipient = $1) \
             ORDER BY id DESC LIMIT $3",
            &[&user_a, &user_b, &limit],
        )
        .await?
    };
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<_, i64>(0), row.get(1)))
        .collect())
}
