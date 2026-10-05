//! Who a stored message mentions, for the mentions inbox.
//!
//! Clients decide about pings themselves, live, with `containsMention` in
//! `murmer_client/src/lib/message-utils.ts`. The inbox has to answer the same
//! question for messages that arrived while the user was away, so
//! [`mentions_user`] restates that rule here and must keep matching exactly
//! what it matches: a mention the inbox lists but no badge counted, or the
//! other way round, is a bug nobody can see the cause of. The cases in
//! `tests/mentions_inbox_test.rs` are the client's own cases for that reason.

use serde_json::Value;

/// A character that continues a word for the mention boundary: JavaScript's
/// `\w` without the `u` flag, which is ASCII only.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Whether `text` mentions the account `user`: `@user`, case-insensitively,
/// not preceded by a word character or another `@` (so `bob@alice.example`
/// is an address, not a mention) and not followed by a word character or a
/// dash (so `@alice` does not match `@alice-bob` or `@alicia`).
pub fn mentions_user(text: &str, user: &str) -> bool {
    if user.is_empty() {
        return false;
    }
    let chars: Vec<char> = text.chars().collect();
    let name: Vec<char> = user.chars().collect();
    for at in 0..chars.len() {
        if chars[at] != '@' {
            continue;
        }
        if at > 0 && (is_word(chars[at - 1]) || chars[at - 1] == '@') {
            continue;
        }
        let end = at + 1 + name.len();
        if end > chars.len() {
            continue;
        }
        let same = chars[at + 1..end]
            .iter()
            .zip(&name)
            .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()));
        if !same {
            continue;
        }
        if chars
            .get(end)
            .is_some_and(|&next| is_word(next) || next == '-')
        {
            continue;
        }
        return true;
    }
    false
}

/// Whether a stored message pinged one of `role_ids` through its
/// server-authorized `mentions` field. `@here` is deliberately not counted:
/// it reached whoever was connected when it was sent, and listing it later
/// to somebody who was not would turn every `@here` into an `@everyone`.
pub fn pings_roles(message: &Value, role_ids: &[i64]) -> bool {
    message
        .get("mentions")
        .and_then(|mentions| mentions.get("roles"))
        .and_then(|roles| roles.as_array())
        .is_some_and(|roles| {
            roles
                .iter()
                .filter_map(|id| id.as_i64())
                .any(|id| role_ids.contains(&id))
        })
}

/// Whether a stored message belongs in `user`'s inbox: somebody else wrote
/// it, and it names them or pings a role they hold. A sealed message has no
/// text here, so only its role pings can be found — see `docs/features.md`.
pub fn is_mention_of(message: &Value, user: &str, role_ids: &[i64]) -> bool {
    if message.get("user").and_then(|u| u.as_str()) == Some(user) {
        return false;
    }
    let named = message
        .get("text")
        .and_then(|t| t.as_str())
        .is_some_and(|text| mentions_user(text, user));
    named || pings_roles(message, role_ids)
}
