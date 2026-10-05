//! Handler for `load-mentions`: the mentions inbox.
//!
//! A per-channel badge says *where* somebody was mentioned; the inbox says
//! *what* was said, across every channel, including while the user was away.
//! It is an answer to a request and goes to the requester alone, built only
//! from channels they can see right now — a channel they lost access to takes
//! its mentions with it, the same as its history.
//!
//! What counts as a mention is [`crate::mentions::is_mention_of`], which
//! restates the client's rule. An encrypted channel's text is sealed, so only
//! its role pings can be found here; name mentions there reach the inbox live,
//! from the client's own copy, while the user is connected.

use crate::channel_overrides::ChannelKind;
use crate::mentions::is_mention_of;
use crate::ws::helpers::*;
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

/// How many mentions the inbox lists. Recent ones are the point; older ones
/// are a search away.
pub const MAX_INBOX_MENTIONS: usize = 50;

/// How many candidate rows the prefilter may hand over. Larger than the
/// inbox because the LIKE also matches `@alicia` for `alice`, and every group
/// ping; small enough that one request stays a bounded amount of work on the
/// single database thread.
const MAX_INBOX_CANDIDATES: i64 = 400;

/// Handle `load-mentions`. Anyone authenticated may ask, and only ever about
/// themselves: the frame names nobody.
pub(super) async fn handle_load_mentions(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.as_deref() else {
        return;
    };

    let mut visible = Vec::new();
    for channel in db::get_channels(&state.db).await {
        if can_view_channel(state, user, ChannelKind::Text, channel.id).await {
            visible.push(channel.id);
        }
    }
    let role_ids = state
        .user_roles
        .lock()
        .await
        .get(user)
        .cloned()
        .unwrap_or_default();

    let candidates = match db::recent_mention_candidates(
        &state.db,
        &visible,
        user,
        MAX_INBOX_CANDIDATES,
    )
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("mentions inbox query failed for {user}: {e}");
            Vec::new()
        }
    };

    let mut rows = Vec::new();
    let mut channel_of = std::collections::HashMap::new();
    for (id, channel_id, content) in candidates {
        let Ok(parsed) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if is_mention_of(&parsed, user, &role_ids) {
            channel_of.insert(id, channel_id);
            rows.push((id, content));
            if rows.len() == MAX_INBOX_MENTIONS {
                break;
            }
        }
    }

    // Rows come from several channels, so the channel is stamped per row
    // rather than through `hydrate_messages`' single fallback.
    let mut messages = db::hydrate_messages(&state.db, rows, 0).await;
    for message in &mut messages {
        if let Some(channel_id) = message
            .get("id")
            .and_then(|id| id.as_i64())
            .and_then(|id| channel_of.get(&id))
        {
            message["channelId"] = Value::from(*channel_id);
        }
    }

    let payload = serde_json::json!({ "type": "mentions-inbox", "messages": messages });
    send_json(sender, &payload).await;
}
