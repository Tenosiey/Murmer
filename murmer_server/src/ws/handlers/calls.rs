//! Direct calls: one member rings another from a DM, the callee accepts or
//! declines, and either side hangs up.
//!
//! A call belongs to no channel the server stores, but its media does not
//! know that. Starting one creates an ordinary in-memory voice channel under
//! a negative id, which both clients then join with `voice-join` like any
//! other. That is what lets the mesh, the SFU, cameras, screen shares,
//! mutes and connection repair all work in a call without a line of their
//! own. What keeps it a *private* call lives in `helpers.rs`: `is_call`
//! makes the channel invisible to everyone but its two parties —
//! administrators included — and keeps it out of every channel list.
//!
//! The channel is sticky, so on a server running the SFU the call goes
//! through it from the first join, like every other channel there, and
//! neither side learns the other's address. Without the SFU it is a mesh
//! of two.
//!
//! Frames: `call-start {target}` from the caller, then `call-ringing` to
//! both parties; `call-accept {callId}` from the callee, then
//! `call-accepted`; `call-end {callId}` from either side, then `call-ended`
//! with a `reason` (`declined`, `cancelled`, `missed` or `ended`). All of
//! them go direct: a call is nobody else's business.
//!
//! A call ends by itself when its headcount drops below the most it held
//! (one party leaving a call of two ends it for the other), when it rings
//! out unanswered, when an accepted call is never joined, and when a
//! party's last connection closes.

use super::*;
use crate::{Call, VoiceChannelState, VoiceMode};
use std::sync::atomic::{AtomicI32, Ordering};

/// Call ids count down from -1, so they can never meet a database row (see
/// `helpers::is_call`).
static NEXT_CALL_ID: AtomicI32 = AtomicI32::new(-1);

/// Handle `call-start`: ring `target`. Anyone authenticated may call anyone
/// they could poke — online, not appearing offline, not themselves — paced
/// like a message and silenced by a server mute. Blocking is the callee's
/// client's job, as for DMs: it lets a blocked caller ring out unanswered.
pub(super) async fn handle_call_start(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(caller) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let Some(callee) = v.get("target").and_then(|t| t.as_str()) else {
        send_error(sender, errors::CALL_UNAVAILABLE).await;
        return;
    };
    let reachable = callee != caller
        && state.direct.lock().await.contains_key(callee)
        && state
            .statuses
            .lock()
            .await
            .get(callee)
            .is_none_or(|status| status != "offline");
    if !reachable {
        send_error(sender, errors::CALL_UNAVAILABLE).await;
        return;
    }
    if moderation::is_muted(state, caller).await {
        send_error(sender, errors::MUTED).await;
        return;
    }
    if !crate::security::check_message_rate_limit(&state.rate_limiter, caller).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return;
    }
    let quality = db::voice_defaults(&state.db)
        .await
        .unwrap_or_default()
        .quality;

    let id = NEXT_CALL_ID.fetch_sub(1, Ordering::Relaxed);
    {
        let mut calls = state.calls.lock().await;
        if calls
            .values()
            .any(|call| call.has_party(caller) || call.has_party(callee))
        {
            drop(calls);
            send_error(sender, errors::CALL_BUSY).await;
            return;
        }
        calls.insert(
            id,
            Call {
                caller: caller.to_string(),
                callee: callee.to_string(),
                accepted: false,
                peak: 0,
            },
        );
    }
    // Registered after the call, so the channel is never visible without
    // the parties that decide who may see it.
    state.voice_channels.lock().await.insert(
        id,
        VoiceChannelState {
            name: String::new(),
            users: HashSet::new(),
            quality,
            bitrate: None,
            category_id: None,
            position: 0,
            breakout_parent: None,
            user_limit: 2,
            mode: VoiceMode::Mesh,
            sticky: true,
        },
    );
    info!(call = id, "call started");
    let frame = serde_json::json!({
        "type": "call-ringing",
        "callId": id,
        "caller": caller,
        "callee": callee,
    });
    send_to_parties(state, caller, callee, &frame).await;
    spawn_watchdog(state, id, |call| !call.accepted, "missed");
}

/// Handle `call-accept`: only the callee, only while it rings. Every one of
/// their connections hears `call-accepted`, so the devices that did not
/// answer stop ringing.
pub(super) async fn handle_call_accept(
    state: &Arc<AppState>,
    v: &Value,
    user_name: &Option<String>,
) {
    let (Some(user), Some(id)) = (user_name.as_deref(), i32_field(v, "callId")) else {
        return;
    };
    let parties = {
        let mut calls = state.calls.lock().await;
        match calls.get_mut(&id) {
            Some(call) if call.callee == user && !call.accepted => {
                call.accepted = true;
                Some((call.caller.clone(), call.callee.clone()))
            }
            _ => None,
        }
    };
    let Some((caller, callee)) = parties else {
        return;
    };
    let frame = serde_json::json!({ "type": "call-accepted", "callId": id });
    send_to_parties(state, &caller, &callee, &frame).await;
    // Answered but never joined (a client that failed to open the
    // microphone, say) must not leave both parties busy for good.
    spawn_watchdog(state, id, |call| call.peak < 2, "ended");
}

/// Handle `call-end`: either party declines, cancels or hangs up.
pub(super) async fn handle_call_end(state: &Arc<AppState>, v: &Value, user_name: &Option<String>) {
    let (Some(user), Some(id)) = (user_name.as_deref(), i32_field(v, "callId")) else {
        return;
    };
    let reason = match state.calls.lock().await.get(&id) {
        Some(call) if !call.has_party(user) => return,
        Some(call) if call.accepted => "ended",
        Some(call) if call.callee == user => "declined",
        Some(_) => "cancelled",
        None => return,
    };
    end_call(state, id, reason).await;
}

/// Follow a call's headcount (from `broadcast_voice`, on every join and
/// leave). Somebody leaving ends it: a call of two with one person in it is
/// over, and the one left behind should not have to notice.
pub(crate) async fn headcount_changed(state: &Arc<AppState>, id: i32, headcount: usize) {
    let reason = {
        let mut calls = state.calls.lock().await;
        let Some(call) = calls.get_mut(&id) else {
            return;
        };
        if headcount >= call.peak {
            call.peak = headcount;
            return;
        }
        if call.accepted { "ended" } else { "cancelled" }
    };
    end_call(state, id, reason).await;
}

/// End every call `user` is a party to, once their last connection closed.
/// A callee who goes away while it rings missed it.
pub(super) async fn end_calls_of(state: &Arc<AppState>, user: &str) {
    let ended: Vec<(i32, &'static str)> = state
        .calls
        .lock()
        .await
        .iter()
        .filter(|(_, call)| call.has_party(user))
        .map(|(id, call)| (*id, if call.accepted { "ended" } else { "missed" }))
        .collect();
    for (id, reason) in ended {
        end_call(state, id, reason).await;
    }
}

/// Forget a call and its voice channel and tell both parties why it ended.
/// Their clients leave the voice session themselves, as they do when a
/// channel is deleted under them.
async fn end_call(state: &Arc<AppState>, id: i32, reason: &str) {
    let Some(call) = state.calls.lock().await.remove(&id) else {
        return;
    };
    drop_voice_channel(state, id).await;
    info!(call = id, reason, "call ended");
    let frame = serde_json::json!({ "type": "call-ended", "callId": id, "reason": reason });
    send_to_parties(state, &call.caller, &call.callee, &frame).await;
}

/// End call `id` with `reason` after the ring timeout if `stale` still
/// holds for it then.
fn spawn_watchdog(state: &Arc<AppState>, id: i32, stale: fn(&Call) -> bool, reason: &'static str) {
    let state = Arc::clone(state);
    tokio::spawn(async move {
        tokio::time::sleep(state.ring_timeout).await;
        let expired = state.calls.lock().await.get(&id).is_some_and(stale);
        if expired {
            end_call(&state, id, reason).await;
        }
    });
}

async fn send_to_parties(state: &Arc<AppState>, caller: &str, callee: &str, frame: &Value) {
    let frame: crate::Frame = frame.to_string().into();
    send_to_user(state, caller, frame.clone()).await;
    send_to_user(state, callee, frame).await;
}
