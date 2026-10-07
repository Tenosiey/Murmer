//! Poll vote persistence.
//!
//! A poll is an ordinary message whose `poll` field lists its options; the
//! question is the message's text. Votes live beside it in `poll_votes`, one
//! row per account, and are never written into the stored frame — the tally
//! is attached whenever the message is served (see [`attach_poll_votes`]).
//!
//! Votes cascade away with their message through the foreign key, so every
//! path that deletes messages (one by one, the retention sweep, the Danger
//! Zone) cleans them up without having to know they exist.

use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;

use super::{Db, DbCall, DbError};

/// Record `user`'s vote on a poll, replacing any earlier one, or take it
/// back with `None`. The caller has already checked the option exists.
pub async fn set_poll_vote(
    db: &Db,
    message_id: i64,
    user: &str,
    option: Option<usize>,
) -> Result<(), DbError> {
    let user = user.to_owned();
    db.call_db(move |conn| {
        match option {
            Some(option) => conn
                .prepare_cached(
                    "INSERT INTO poll_votes (message_id, user_name, option) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (message_id, user_name) DO UPDATE SET option = excluded.option",
                )?
                .execute(params![message_id, user, option as i64])?,
            None => conn
                .prepare_cached("DELETE FROM poll_votes WHERE message_id = ?1 AND user_name = ?2")?
                .execute(params![message_id, user])?,
        };
        Ok(())
    })
    .await
}

/// Every vote on the given messages, as `(option, user)` pairs per message.
pub async fn get_poll_votes_for_messages(
    db: &Db,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<(i64, String)>>, DbError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    // A JSON array keeps the statement text fixed for any batch size; see
    // `get_reactions_for_messages`.
    let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
    let rows = db
        .call_db(move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT message_id, option, user_name FROM poll_votes \
                 WHERE message_id IN (SELECT value FROM json_each(?1)) \
                 ORDER BY user_name",
            )?;
            let rows = stmt
                .query_map(params![ids_json], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get(2)?))
                })?
                .collect::<Result<Vec<(i64, i64, String)>, _>>()?;
            Ok(rows)
        })
        .await?;
    let mut map: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    for (message_id, option, user) in rows {
        map.entry(message_id).or_default().push((option, user));
    }
    Ok(map)
}

/// The tally of one poll: for each of its options, in order, the accounts
/// that chose it. Shaped from the poll's own option list so a vote for an
/// option that is somehow out of range is dropped rather than served.
pub fn poll_tally(poll: &Value, votes: &[(i64, String)]) -> Value {
    let count = poll
        .get("options")
        .and_then(|o| o.as_array())
        .map_or(0, Vec::len);
    let mut tally = vec![Vec::<&str>::new(); count];
    for (option, user) in votes {
        if let Some(voters) = usize::try_from(*option).ok().and_then(|i| tally.get_mut(i)) {
            voters.push(user);
        }
    }
    serde_json::json!(tally)
}

/// Fill in `poll.votes` on every poll among `messages`, which must carry
/// their `id`. Messages that are not polls are left alone.
pub async fn attach_poll_votes(db: &Db, messages: &mut [Value]) {
    let ids: Vec<i64> = messages
        .iter()
        .filter(|m| m.get("poll").is_some_and(Value::is_object))
        .filter_map(|m| m.get("id").and_then(Value::as_i64))
        .collect();
    if ids.is_empty() {
        return;
    }
    let votes = get_poll_votes_for_messages(db, &ids)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("db poll vote load error: {e}");
            HashMap::new()
        });
    for msg in messages.iter_mut() {
        let Some(id) = msg.get("id").and_then(Value::as_i64) else {
            continue;
        };
        if let Some(poll) = msg.get_mut("poll").filter(|p| p.is_object()) {
            let tally = poll_tally(poll, votes.get(&id).map_or(&[], Vec::as_slice));
            poll["votes"] = tally;
        }
    }
}
