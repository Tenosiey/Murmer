//! Reaction persistence operations.

use std::collections::HashMap;
use tokio_postgres::Client;

/// Retrieve reactions for a set of message IDs, grouped by message and emoji.
pub async fn get_reactions_for_messages(
    db: &Client,
    ids: &[i32],
) -> Result<HashMap<i32, HashMap<String, Vec<String>>>, tokio_postgres::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = db
        .query(
            "SELECT message_id, emoji, user_name FROM reactions WHERE message_id = ANY($1)",
            &[&ids],
        )
        .await?;

    let mut map: HashMap<i32, HashMap<String, Vec<String>>> = HashMap::new();
    for row in rows {
        let message_id: i32 = row.get(0);
        let emoji: String = row.get(1);
        let user: String = row.get(2);
        let emoji_map = map.entry(message_id).or_default();
        let users = emoji_map.entry(emoji).or_default();
        users.push(user);
    }

    for emoji_map in map.values_mut() {
        for users in emoji_map.values_mut() {
            users.sort();
            users.dedup();
        }
    }

    Ok(map)
}

/// Retrieve all reactions for a single message, grouped by emoji.
pub async fn get_reaction_summary(
    db: &Client,
    message_id: i32,
) -> Result<HashMap<String, Vec<String>>, tokio_postgres::Error> {
    let mut map = get_reactions_for_messages(db, &[message_id]).await?;
    Ok(map.remove(&message_id).unwrap_or_default())
}

/// Add a reaction to a message. Duplicate reactions by the same user are ignored.
pub async fn add_reaction(
    db: &Client,
    message_id: i32,
    user: &str,
    emoji: &str,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "INSERT INTO reactions (message_id, user_name, emoji) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        &[&message_id, &user, &emoji],
    )
    .await
    .map(|_| ())
}

/// Remove a reaction from a message.
pub async fn remove_reaction(
    db: &Client,
    message_id: i32,
    user: &str,
    emoji: &str,
) -> Result<(), tokio_postgres::Error> {
    db.execute(
        "DELETE FROM reactions WHERE message_id = $1 AND user_name = $2 AND emoji = $3",
        &[&message_id, &user, &emoji],
    )
    .await
    .map(|_| ())
}
