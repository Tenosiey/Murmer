//! Raised hands in voice channels: the speaking queue for calls with more
//! listeners than talkers.
//!
//! Anyone in a voice channel may raise or lower their own hand, and nobody
//! else's. The server stamps when a hand went up, so the queue order is the
//! server's rather than whatever clock a client reports, and raising an
//! already raised hand keeps its place. Hands live in memory only and drop
//! when their owner leaves the channel: a hand raised in one call has no
//! business following its owner into the next, and without the lowered
//! broadcast everyone else would keep showing it.

use super::*;

/// Handle `voice-hand`: raise or lower the sender's hand in the voice channel
/// they are in.
pub(super) async fn handle_voice_hand(
    state: &Arc<AppState>,
    v: &Value,
    user_name: &Option<String>,
    voice_channel: Option<i32>,
) {
    let (Some(user), Some(ch_id)) = (user_name.as_deref(), voice_channel) else {
        return;
    };
    // Only into the channel the connection actually sits in; the frame is
    // filtered by its `channelId`, so a forged one could reach the wrong room.
    if i32_field(v, "channelId") != Some(ch_id) {
        return;
    }
    let Some(raised) = v.get("raised").and_then(|r| r.as_bool()) else {
        return;
    };

    let at = {
        let mut hands = state.voice_hands.lock().await;
        if raised {
            let entry = hands
                .entry(user.to_string())
                .or_insert((ch_id, chrono::Utc::now().timestamp_millis()));
            // A hand still up in a channel it has since left starts over.
            if entry.0 != ch_id {
                *entry = (ch_id, chrono::Utc::now().timestamp_millis());
            }
            Some(entry.1)
        } else {
            if hands.remove(user).is_none() {
                return;
            }
            None
        }
    };
    broadcast_hand(state, user, ch_id, at);
}

/// Lower `user`'s hand, if raised, and tell its channel. Called whenever the
/// user leaves a voice channel or disconnects.
pub(super) async fn lower_hand(state: &Arc<AppState>, user: &str) {
    let removed = state.voice_hands.lock().await.remove(user);
    if let Some((ch_id, _)) = removed {
        broadcast_hand(state, user, ch_id, None);
    }
}

/// Send the raised hands of one voice channel to a single client that just
/// joined it. Sent even when empty: it replaces whatever the client still
/// holds from an earlier visit.
pub(super) async fn send_voice_hands(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    channel_id: i32,
) {
    let hands: serde_json::Map<String, Value> = state
        .voice_hands
        .lock()
        .await
        .iter()
        .filter(|(_, (ch, _))| *ch == channel_id)
        .map(|(user, (_, at))| (user.clone(), Value::from(*at)))
        .collect();
    send_json(
        sender,
        &serde_json::json!({
            "type": "voice-hands-active",
            "channelId": channel_id,
            "hands": hands,
        }),
    )
    .await;
}

/// `at` is when the hand went up, or `None` for a lowered hand.
fn broadcast_hand(state: &AppState, user: &str, channel_id: i32, at: Option<i64>) {
    broadcast(
        state,
        &serde_json::json!({
            "type": "voice-hand",
            "user": user,
            "channelId": channel_id,
            "raised": at.is_some(),
            "at": at,
        }),
    );
}
