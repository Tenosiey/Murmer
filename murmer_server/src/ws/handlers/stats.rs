//! Lifetime user statistics: recording hooks and the stats WebSocket API.
//!
//! Privacy model (double opt-in): counters are only written when the
//! server-wide toggle (Owner/Admin controlled) AND the user's own opt-in are
//! both enabled. The gate itself is enforced inside
//! [`db::record_user_stats`]; every hook in this module funnels through it.
//!
//! Recording hooks are called from the other handler modules after an action
//! succeeded (message persisted, reaction stored, …). Voice and screen share
//! durations are measured in memory (`AppState::voice_session_starts` /
//! `screenshare_session_starts`) and flushed to the database when the
//! session ends or the connection drops.

use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db, db::Stat};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

/// Apply counter increments for a user, ignoring the result when tracking is
/// disabled. Failures are logged and never surface to the client: stats are
/// best-effort bookkeeping and must not break the action they piggyback on.
pub(super) async fn record(state: &Arc<AppState>, user: &str, deltas: Vec<(Stat, i64)>) {
    if let Err(e) = db::record_user_stats(&state.db, user, deltas, None).await {
        error!("failed to record stats for {user}: {e}");
    }
}

/// Like [`record`], additionally bumping the per-emoji reaction counter used
/// for the "favorite reaction" stat.
async fn record_with_reaction(state: &Arc<AppState>, user: &str, emoji: &str) {
    if let Err(e) = db::record_user_stats(
        &state.db,
        user,
        vec![(Stat::ReactionsGiven, 1)],
        Some(emoji.to_string()),
    )
    .await
    {
        error!("failed to record reaction stats for {user}: {e}");
    }
}

/// Whether a URL points at an animated GIF (direct `.gif` links and Giphy
/// pages, mirroring the client's inline GIF embedding).
fn is_gif_url(url: &str) -> bool {
    let no_query = url.split(['?', '#']).next().unwrap_or(url);
    if no_query.to_ascii_lowercase().ends_with(".gif") {
        return true;
    }
    url.split("//")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|host| {
            let host = host.to_ascii_lowercase();
            host == "giphy.com" || host.ends_with(".giphy.com")
        })
        .unwrap_or(false)
}

/// Derive stat increments from a chat message that was just persisted.
/// Only aggregate numbers (lengths, counts, kinds) are derived — the text
/// itself is never stored in the stats tables.
pub(super) fn chat_message_deltas(v: &Value) -> Vec<(Stat, i64)> {
    let mut deltas: Vec<(Stat, i64)> = vec![(Stat::MessagesSent, 1)];

    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        let chars = text.chars().count() as i64;
        if chars > 0 {
            deltas.push((Stat::MessageChars, chars));
            deltas.push((Stat::MessageBytes, text.len() as i64));
            deltas.push((Stat::LongestMessageChars, chars));
        }

        let mut links = 0i64;
        let mut gif_links = 0i64;
        let mut mentions = 0i64;
        for word in text.split_whitespace() {
            if word.starts_with("http://") || word.starts_with("https://") {
                links += 1;
                if is_gif_url(word) {
                    gif_links += 1;
                }
            } else if word.len() > 1 && word.starts_with('@') {
                mentions += 1;
            }
        }
        if links > 0 {
            deltas.push((Stat::LinksShared, links));
        }
        if gif_links > 0 {
            deltas.push((Stat::GifsSent, gif_links));
        }
        if mentions > 0 {
            deltas.push((Stat::MentionsSent, mentions));
        }
    }

    if let Some(image_url) = v.get("image").and_then(|i| i.as_str()) {
        if is_gif_url(image_url) {
            deltas.push((Stat::GifsSent, 1));
        } else {
            deltas.push((Stat::ImagesSent, 1));
        }
    }

    if let Some(attachment) = v.get("attachment").and_then(|a| a.as_object()) {
        deltas.push((Stat::FilesSent, 1));
        // The size is client-reported; clamp it to the upload limit so a
        // crafted frame cannot inflate the byte counter.
        if let Some(size) = attachment.get("size").and_then(|s| s.as_i64()) {
            let clamped = size.clamp(0, crate::upload::MAX_FILE_SIZE as i64);
            if clamped > 0 {
                deltas.push((Stat::UploadBytes, clamped));
            }
        }
    }

    if v.get("replyTo").is_some() {
        deltas.push((Stat::RepliesSent, 1));
    }

    deltas
}

/// Record a reaction "add": the reactor's given-count and favorite emoji,
/// plus the message author's received-count (each gated by their own opt-in).
pub(super) async fn record_reaction_added(
    state: &Arc<AppState>,
    reactor: &str,
    author: Option<&str>,
    emoji: &str,
) {
    record_with_reaction(state, reactor, emoji).await;
    if let Some(author) = author
        && author != reactor
    {
        record(state, author, vec![(Stat::ReactionsReceived, 1)]).await;
    }
}

// ── Voice / screen share session timing ─────────────────────────────────────

/// Note that a user is now in a voice channel. Switching channels keeps the
/// original start so the whole stretch counts as one session.
pub(super) async fn note_voice_join(state: &Arc<AppState>, user: &str) {
    let mut starts = state.voice_session_starts.lock().await;
    if !starts.contains_key(user) {
        starts.insert(user.to_string(), Instant::now());
        drop(starts);
        record(state, user, vec![(Stat::VoiceSessions, 1)]).await;
    }
}

/// Flush a finished voice session into the lifetime counter.
pub(super) async fn flush_voice_session(state: &Arc<AppState>, user: &str) {
    let started = state.voice_session_starts.lock().await.remove(user);
    if let Some(started) = started {
        let seconds = started.elapsed().as_secs() as i64;
        if seconds > 0 {
            record(state, user, vec![(Stat::VoiceSeconds, seconds)]).await;
        }
    }
}

/// Note that a user started screen sharing.
pub(super) async fn note_screenshare_start(state: &Arc<AppState>, user: &str) {
    let mut starts = state.screenshare_session_starts.lock().await;
    starts.entry(user.to_string()).or_insert_with(Instant::now);
}

/// Flush a finished screen share into the lifetime counter.
pub(super) async fn flush_screenshare_session(state: &Arc<AppState>, user: &str) {
    let started = state.screenshare_session_starts.lock().await.remove(user);
    if let Some(started) = started {
        let seconds = started.elapsed().as_secs() as i64;
        if seconds > 0 {
            record(state, user, vec![(Stat::ScreenshareSeconds, seconds)]).await;
        }
    }
}

// ── WebSocket API ────────────────────────────────────────────────────────────

/// Build the per-user `stats-config` frame (server toggle + own opt-in).
async fn stats_config_frame(state: &Arc<AppState>, user: &str) -> Option<String> {
    let server_enabled = match db::stats_server_enabled(&state.db).await {
        Ok(enabled) => enabled,
        Err(e) => {
            error!("failed to load stats server toggle: {e}");
            return None;
        }
    };
    let opted_in = match db::stats_opt_in(&state.db, user).await {
        Ok(opted) => opted,
        Err(e) => {
            error!("failed to load stats opt-in for {user}: {e}");
            return None;
        }
    };
    serde_json::to_string(&serde_json::json!({
        "type": "stats-config",
        "serverEnabled": server_enabled,
        "optedIn": opted_in,
    }))
    .ok()
}

/// Send the current stats configuration to a single client (also used right
/// after authentication so the client knows the state without asking).
pub(super) async fn send_stats_config(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
) {
    if let Some(frame) = stats_config_frame(state, user).await {
        let _ = sender.send(Message::Text(frame.into())).await;
    }
}

/// Handle `get-stats-config`.
pub(super) async fn handle_get_stats_config(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.as_deref() else {
        return;
    };
    send_stats_config(state, sender, user).await;
}

/// Handle `set-stats-opt-in`: a user's own tracking decision. Opting out
/// stops future recording; passing `"purge": true` additionally deletes all
/// counters recorded so far.
pub(super) async fn handle_set_stats_opt_in(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let Some(enabled) = v.get("enabled").and_then(|e| e.as_bool()) else {
        send_error(sender, errors::STATS_UPDATE_FAILED).await;
        return;
    };

    if let Err(e) = db::set_stats_opt_in(&state.db, user, enabled).await {
        error!("failed to store stats opt-in for {user}: {e}");
        send_error(sender, errors::STATS_UPDATE_FAILED).await;
        return;
    }

    if v.get("purge").and_then(|p| p.as_bool()) == Some(true) {
        if let Err(e) = db::purge_user_stats(&state.db, user).await {
            error!("failed to purge stats for {user}: {e}");
            send_error(sender, errors::STATS_UPDATE_FAILED).await;
            return;
        }
        info!(user, "User stats purged");
    }

    info!(user, enabled, "Stats opt-in updated");
    send_stats_config(state, sender, user).await;
}

/// Handle `reset-stats`: delete the requester's own recorded counters while
/// keeping their opt-in preference untouched.
pub(super) async fn handle_reset_stats(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    if let Err(e) = db::purge_user_stats(&state.db, user).await {
        error!("failed to purge stats for {user}: {e}");
        send_error(sender, errors::STATS_UPDATE_FAILED).await;
        return;
    }
    info!(user, "User stats purged");
    send_user_stats(state, sender, user).await;
}

/// Handle `set-stats-enabled`: the server-wide toggle, restricted to
/// `STATS_ADMIN_ROLES`. The change is broadcast so every client can update
/// its settings UI immediately.
pub(super) async fn handle_set_stats_enabled(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::STATS_PERMISSION_DENIED).await;
        return;
    };

    // Server-side role check; clients cannot spoof this.
    let allowed = {
        let roles = state.roles.lock().await;
        roles
            .get(requester)
            .map(|info| {
                STATS_ADMIN_ROLES
                    .iter()
                    .any(|role| info.role.eq_ignore_ascii_case(role))
            })
            .unwrap_or(false)
    };
    if !allowed {
        info!(requester, "Denied stats toggle request");
        send_error(sender, errors::STATS_PERMISSION_DENIED).await;
        return;
    }

    let Some(enabled) = v.get("enabled").and_then(|e| e.as_bool()) else {
        send_error(sender, errors::STATS_UPDATE_FAILED).await;
        return;
    };

    if let Err(e) = db::set_stats_server_enabled(&state.db, enabled).await {
        error!("failed to store stats server toggle: {e}");
        send_error(sender, errors::STATS_UPDATE_FAILED).await;
        return;
    }

    info!(requester, enabled, "Server-wide stat tracking toggled");
    // Broadcast without `optedIn`: each client keeps its own opt-in value.
    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "stats-config",
        "serverEnabled": enabled,
    })) {
        let _ = state.tx.send(msg);
    }
}

/// Serialize a stats snapshot and send it to one client.
async fn send_user_stats(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    target: &str,
) {
    let record = match db::get_user_stats(&state.db, target).await {
        Ok(record) => record.unwrap_or_default(),
        Err(e) => {
            error!("failed to load stats for {target}: {e}");
            send_error(sender, errors::STATS_UPDATE_FAILED).await;
            return;
        }
    };
    let favorites =
        match db::get_favorite_reactions(&state.db, target, MAX_FAVORITE_REACTIONS).await {
            Ok(list) => list,
            Err(e) => {
                error!("failed to load favorite reactions for {target}: {e}");
                Vec::new()
            }
        };
    let favorite_entries: Vec<Value> = favorites
        .into_iter()
        .map(|(emoji, count)| serde_json::json!({ "emoji": emoji, "count": count }))
        .collect();

    let payload = serde_json::json!({
        "type": "user-stats",
        "user": target,
        "trackedSince": record.created_at,
        "stats": {
            "messagesSent": record.messages_sent,
            "messageChars": record.message_chars,
            "messageBytes": record.message_bytes,
            "longestMessageChars": record.longest_message_chars,
            "imagesSent": record.images_sent,
            "gifsSent": record.gifs_sent,
            "filesSent": record.files_sent,
            "uploadBytes": record.upload_bytes,
            "linksShared": record.links_shared,
            "repliesSent": record.replies_sent,
            "mentionsSent": record.mentions_sent,
            "dmsSent": record.dms_sent,
            "reactionsGiven": record.reactions_given,
            "reactionsReceived": record.reactions_received,
            "messagesEdited": record.messages_edited,
            "messagesDeleted": record.messages_deleted,
            "pinsAdded": record.pins_added,
            "voiceSeconds": record.voice_seconds,
            "voiceSessions": record.voice_sessions,
            "screenshareSeconds": record.screenshare_seconds,
        },
        "favoriteReactions": favorite_entries,
    });
    let _ = sender.send(Message::Text(payload.to_string().into())).await;
}

/// Handle `get-user-stats`. Users can always fetch their own stats; another
/// user's stats are only revealed when that user is currently opted in — the
/// opt-in doubles as consent to show the numbers to other members.
pub(super) async fn handle_get_user_stats(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };

    let target = v
        .get("user")
        .and_then(|u| u.as_str())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .unwrap_or(requester);

    if target != requester {
        let visible = match db::stats_server_enabled(&state.db).await {
            Ok(true) => db::stats_opt_in(&state.db, target).await.unwrap_or(false),
            _ => false,
        };
        if !visible {
            send_error(sender, errors::STATS_NOT_AVAILABLE).await;
            return;
        }
    }

    send_user_stats(state, sender, target).await;
}
