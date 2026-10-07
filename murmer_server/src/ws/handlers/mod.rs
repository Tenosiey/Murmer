//! WebSocket message handlers.
//!
//! The socket loop and dispatch live here, along with the small voice,
//! screen-share and camera handlers, the ICE server announcement, the
//! server-info and operator-metrics answers, and the rest of what is not
//! worth a file of its own. Each
//! submodule handles one domain and documents itself.

mod audit;
mod auth;
mod automod;
mod breakout;
mod channel_keys;
mod channel_overrides;
mod channels;
mod chat_settings;
mod dms;
mod emojis;
mod hands;
mod identity;
mod invites;
mod maintenance;
mod mentions;
mod messages;
mod moderation;
mod pins;
mod profile;
mod read_markers;
mod roles;
mod scheduled;
mod screenshare;
mod soundboard;
mod stats;
mod uploads;
mod voice_defaults;
mod wiki;

pub use maintenance::spawn_message_retention;
pub use pins::broadcast_pins;
pub use scheduled::{recover_claimed_scheduled_messages, spawn_scheduler};

use super::{
    constants::{DEFAULT_HISTORY_LIMIT, MAX_HISTORY_LIMIT, MAX_WS_MESSAGE_BYTES},
    errors,
    helpers::*,
    validation::*,
};
use crate::channel_overrides::ChannelKind;
use crate::{AppState, db};
use axum::{
    extract::{
        ConnectInfo, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt, stream::SplitSink};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tracing::{debug, info, instrument, warn};

/// Hands out a process-unique id per connection, so a user's mailbox can be
/// removed on disconnect without disturbing their other open connections.
fn next_connection_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Keep the direct-mailbox registration in step with the connection's
/// authenticated identity, which is only known once `presence` succeeds and
/// can change if the socket authenticates again.
async fn sync_direct_registration(
    state: &Arc<AppState>,
    conn_id: u64,
    tx: &tokio::sync::mpsc::Sender<crate::Frame>,
    user_name: &Option<String>,
    registered_as: &mut Option<String>,
) {
    let Some(user) = user_name.as_deref() else {
        return;
    };
    if registered_as.as_deref() == Some(user) {
        return;
    }
    if let Some(previous) = registered_as.as_deref() {
        unregister_direct(state, previous, conn_id).await;
    }
    register_direct(state, user, conn_id, tx.clone()).await;
    *registered_as = Some(user.to_string());
}

/// Relay a WebRTC signaling frame to the single peer it names.
///
/// Offers, answers and ICE candidates concern exactly two peers and every one
/// of them carries the recipient in `target`; clients have always discarded
/// the ones addressed to someone else. Broadcasting them therefore cost every
/// connected client a socket write and a parse per frame, which during the
/// candidate exchange of a mesh call is the bulk of the server's work. The
/// sender is still verified by the caller, so this only narrows who a frame
/// reaches — it never widens it.
async fn relay_to_target(state: &Arc<AppState>, v: &Value, frame: crate::Frame) {
    let Some(target) = v.get("target").and_then(|t| t.as_str()) else {
        debug!("dropping signaling frame without a target");
        return;
    };
    if !send_to_user(state, target, frame).await {
        debug!("signaling target {target} has no open connection");
    }
}

/// Resolve the "general" channel ID from the database.
async fn general_channel_id(state: &Arc<AppState>) -> i32 {
    db::get_channel_id_by_name(&state.db, "general")
        .await
        .unwrap_or(1)
}

/// Main WebSocket loop handling incoming messages and broadcasting events.
#[tracing::instrument(skip(socket, state), fields(client_ip = %client_ip))]
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, client_ip: std::net::IpAddr) {
    let client_ip = client_ip.to_string();
    // Counted for the lifetime of this function, however it ends.
    let _counted = crate::metrics::Connection::open();
    info!("Client connected");

    // Measured from the moment the socket opened, before any of the setup
    // below has to wait on the database.
    let login_deadline = tokio::time::sleep(state.auth_timeout);
    tokio::pin!(login_deadline);
    let (mut sender, mut receiver) = socket.split();
    // What this connection's `presence` must sign. Fresh per socket, so a
    // proof made for another server — or another connection — is worthless
    // here; see `auth::verify_key_proof`. Once the proof succeeds it doubles
    // as the connection's `/upload` session (`AppState::upload_sessions`).
    let challenge = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(rand::random::<[u8; 32]>())
    };
    send_json(
        &mut sender,
        &serde_json::json!({ "type": "auth-challenge", "challenge": challenge }),
    )
    .await;
    let mut global_rx = state.tx.subscribe();
    // Mailbox for frames addressed to this connection's user specifically;
    // registered once `presence` establishes who that is.
    let conn_id = next_connection_id();
    let (direct_tx, mut direct_rx) =
        tokio::sync::mpsc::channel::<crate::Frame>(crate::DIRECT_MAILBOX_CAPACITY);
    let mut registered_as: Option<String> = None;
    let default_channel_id = general_channel_id(&state).await;
    let mut channel_id: i32 = default_channel_id;
    let mut chan_tx = get_or_create_channel(&state, channel_id).await;
    let mut chan_rx = chan_tx.subscribe();
    let mut user_name: Option<String> = None;
    let mut voice_channel: Option<i32> = None;
    let mut authenticated = state.password.is_none();
    let mut last_typing_broadcast: Option<std::time::Instant> = None;
    // This connection's memo of which channels it may receive frames for.
    let mut visibility = VisibilityCache::default();
    let mut frame_budget = crate::security::FrameBudget::new(
        state.rate_limiter.max_frames_per_second,
        state.rate_limiter.clock.now(),
    );
    // Whether the previous frame was refused too, so a client stuck in a loop
    // gets one error and one log line per run rather than one per frame.
    let mut throttled = false;

    loop {
        tokio::select! {
            () = &mut login_deadline, if user_name.is_none() => {
                info!("Closing a connection that never authenticated");
                send_error(&mut sender, errors::UNAUTHENTICATED).await;
                break;
            }
            incoming = receiver.next() => {
                let text = match incoming {
                    Some(Ok(Message::Text(t))) => t,
                    // Axum answers pings itself but still hands them over;
                    // bot libraries send them as keepalives, so they must
                    // not end the connection.
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                    _ => break,
                };
                crate::metrics::frame_received();

                // Checked before the frame is even parsed: parsing is part of
                // the work the budget exists to bound.
                if !frame_budget.take(state.rate_limiter.clock.now()) {
                    crate::metrics::rejected(crate::metrics::Limit::Frames);
                    if !throttled {
                        throttled = true;
                        warn!("Frame rate limit exceeded for {}", user_name.as_deref().unwrap_or("unauthenticated client"));
                        send_error(&mut sender, errors::FRAME_RATE_LIMIT).await;
                    }
                    continue;
                }
                throttled = false;

                if let Ok(mut v) = serde_json::from_str::<Value>(&text) {
                    if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
                        debug!("Received message type: {t}");

                        if !authenticated && t != "presence" && t != "bot-presence" {
                            send_error(&mut sender, errors::UNAUTHENTICATED).await;
                            break;
                        }

                        match t {
                            "presence" => {
                                if auth::handle_presence(&mut sender, &state, &mut v, &mut authenticated, &mut user_name, &client_ip, &challenge, default_channel_id).await.is_err() {
                                    break;
                                }
                                sync_direct_registration(&state, conn_id, &direct_tx, &user_name, &mut registered_as).await;
                            }
                            "bot-presence" => {
                                if auth::handle_bot_presence(&mut sender, &state, &v, &mut authenticated, &mut user_name, default_channel_id).await.is_err() {
                                    break;
                                }
                                sync_direct_registration(&state, conn_id, &direct_tx, &user_name, &mut registered_as).await;
                            }
                            "join" => {
                                messages::handle_join(&state, &mut sender, &v, &mut channel_id, &mut chan_tx, &mut chan_rx, &user_name).await;
                            }
                            "load-history" => {
                                messages::handle_load_history(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "load-thread" => {
                                messages::handle_load_thread(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "pin-message" => {
                                pins::handle_pin_message(&state, &mut sender, &v, &user_name).await;
                            }
                            "unpin-message" => {
                                pins::handle_unpin_message(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-get" => {
                                wiki::handle_wiki_get(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-history" => {
                                wiki::handle_wiki_history(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-revision" => {
                                wiki::handle_wiki_revision(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-restore" => {
                                wiki::handle_wiki_restore(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-resolve" => {
                                wiki::handle_wiki_resolve(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-create" => {
                                wiki::handle_wiki_create(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-update" => {
                                wiki::handle_wiki_update(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-delete" => {
                                wiki::handle_wiki_delete(&state, &mut sender, &v, &user_name).await;
                            }
                            "wiki-rename" => {
                                wiki::handle_wiki_rename(&state, &mut sender, &v, &user_name).await;
                            }
                            "dm" => {
                                dms::handle_dm(&state, &mut sender, &v, &user_name).await;
                            }
                            "load-dm-history" => {
                                dms::handle_load_dm_history(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-user-key" => {
                                dms::handle_get_user_key(&state, &mut sender, &v).await;
                            }
                            "typing" => {
                                messages::handle_typing(&state, channel_id, &user_name, &mut last_typing_broadcast).await;
                            }
                            "mark-read" => {
                                read_markers::handle_mark_read(&state, &v, &user_name).await;
                            }
                            "load-mentions" => {
                                mentions::handle_load_mentions(&state, &mut sender, &user_name).await;
                            }
                            "search-history" => {
                                messages::handle_search_history(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "create-channel" => {
                                channels::handle_create_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-channel" => {
                                if channels::handle_delete_channel(&state, &mut sender, &v, &user_name, &mut channel_id, &mut chan_tx, &mut chan_rx, default_channel_id).await.is_err() {
                                    continue;
                                }
                            }
                            "rename-channel" => {
                                channels::handle_rename_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "move-channel" => {
                                channels::handle_move_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "reorder-channels" => {
                                channels::handle_reorder_channels(&state, &mut sender, &v, &user_name).await;
                            }
                            "reorder-categories" => {
                                channels::handle_reorder_categories(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-channel-topic" => {
                                channels::handle_set_channel_topic(&state, &mut sender, &v, &user_name).await;
                            }
                            "create-category" => {
                                channels::handle_create_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "rename-category" => {
                                channels::handle_rename_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-category" => {
                                channels::handle_delete_category(&state, &mut sender, &v, &user_name).await;
                            }
                            "create-voice-channel" => {
                                channels::handle_create_voice_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "update-voice-channel" => {
                                channels::handle_update_voice_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "rename-voice-channel" => {
                                channels::handle_rename_voice_channel(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-voice-channel" => {
                                channels::handle_delete_voice_channel(&state, &mut sender, &v, &user_name, &mut voice_channel).await;
                            }
                            "open-breakouts" => {
                                breakout::handle_open_breakouts(&state, &mut sender, &v, &user_name).await;
                            }
                            "close-breakouts" => {
                                breakout::handle_close_breakouts(&state, &mut sender, &v, &user_name).await;
                            }
                            "chat" => {
                                messages::handle_chat(&state, &mut sender, &mut v, channel_id, &user_name).await;
                            }
                            "forward-message" => {
                                messages::handle_forward_message(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-message" => {
                                messages::handle_delete_message(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "edit-message" => {
                                messages::handle_edit_message(&state, &mut sender, &v, channel_id, &user_name).await;
                            }
                            "react" => {
                                messages::handle_react(&state, &mut sender, &v, &user_name).await;
                            }
                            "status-update" => {
                                handle_status_update(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-avatar" => {
                                profile::handle_set_avatar(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-profile" => {
                                profile::handle_set_profile(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-status-text" => {
                                profile::handle_set_status_text(&state, &mut sender, &v, &user_name).await;
                            }
                            "poke" => {
                                handle_poke(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-nickname" => {
                                profile::handle_set_nickname(&state, &mut sender, &v, &user_name).await;
                            }
                            "ping" => {
                                handle_ping(&mut sender, &v).await;
                            }
                            "get-server-info" => {
                                handle_get_server_info(&state, &mut sender, &user_name).await;
                            }
                            "get-server-metrics" => {
                                handle_get_server_metrics(&state, &mut sender, &user_name).await;
                            }
                            "connection-stats" => {
                                handle_connection_stats(&state, &v, &user_name).await;
                            }
                            "get-connection-stats" => {
                                handle_get_connection_stats(&state, &mut sender, &user_name).await;
                            }
                            "get-stats-config" => {
                                stats::handle_get_stats_config(&state, &mut sender, &user_name).await;
                            }
                            "set-stats-opt-in" => {
                                stats::handle_set_stats_opt_in(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-stats-enabled" => {
                                stats::handle_set_stats_enabled(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-user-stats" => {
                                stats::handle_get_user_stats(&state, &mut sender, &v, &user_name).await;
                            }
                            "reset-stats" => {
                                stats::handle_reset_stats(&state, &mut sender, &user_name).await;
                            }
                            "voice-join" => {
                                handle_voice_join(&state, &mut sender, &v, &mut voice_channel, &user_name).await;
                            }
                            "voice-leave" => {
                                handle_voice_leave(&state, &v, &mut voice_channel, &user_name).await;
                            }
                            // WebRTC signaling frames are relayed verbatim, so make sure a
                            // client can only speak for itself, and only to a peer in its
                            // own call, before relaying. Each names its recipient, so it
                            // goes to that peer alone.
                            "voice-offer" | "voice-answer" | "voice-candidate"
                            | "screenshare-offer" | "screenshare-answer"
                            | "screenshare-candidate" => {
                                if claims_own_user(&v, &user_name)
                                    && signals_within_own_voice_channel(&state, &v, voice_channel).await
                                {
                                    relay_to_target(&state, &v, text).await;
                                }
                            }
                            // Start/stop are announcements to the whole channel rather
                            // than one peer, so they stay on the broadcast.
                            "screenshare-start" => {
                                if claims_own_user(&v, &user_name) && names_own_voice_channel(&v, voice_channel) {
                                    tile_start(&state.active_screen_shares, &v).await;
                                    if let Some(u) = user_name.as_deref() {
                                        stats::note_screenshare_start(&state, u).await;
                                    }
                                    broadcast_serialized(&state, text, &v);
                                }
                            }
                            "screenshare-stop" => {
                                if claims_own_user(&v, &user_name) {
                                    tile_stop(&state.active_screen_shares, &v).await;
                                    if let Some(u) = user_name.as_deref() {
                                        stats::flush_screenshare_session(&state, u).await;
                                    }
                                    broadcast_serialized(&state, text, &v);
                                }
                            }
                            // A camera is announced to the channel the same way, but
                            // carries no signaling of its own: the video rides the
                            // voice peer connections that already exist.
                            "webcam-start" => {
                                if claims_own_user(&v, &user_name) && names_own_voice_channel(&v, voice_channel) {
                                    tile_start(&state.active_webcams, &v).await;
                                    broadcast_serialized(&state, text, &v);
                                }
                            }
                            "webcam-stop" => {
                                if claims_own_user(&v, &user_name) {
                                    tile_stop(&state.active_webcams, &v).await;
                                    broadcast_serialized(&state, text, &v);
                                }
                            }
                            "set-screenshare-max-bitrate" => {
                                screenshare::handle_set_screenshare_max_bitrate(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-upload-config" => {
                                uploads::handle_set_upload_config(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-storage-usage" => {
                                uploads::handle_get_storage_usage(&state, &mut sender, &user_name).await;
                            }
                            "get-chat-settings" => {
                                chat_settings::handle_get_chat_settings(&state, &mut sender, &user_name).await;
                            }
                            "set-chat-settings" => {
                                chat_settings::handle_set_chat_settings(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-automod-rules" => {
                                automod::handle_get_automod_rules(&state, &mut sender, &user_name).await;
                            }
                            "set-automod-rules" => {
                                automod::handle_set_automod_rules(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-voice-defaults" => {
                                voice_defaults::handle_set_voice_defaults(&state, &mut sender, &v, &user_name).await;
                            }
                            "purge-all-messages" => {
                                maintenance::handle_purge_all_messages(&state, &mut sender, &v, &user_name).await;
                            }
                            "reset-server" => {
                                maintenance::handle_reset_server(&state, &mut sender, &v, &user_name).await;
                            }
                            "voice-mute" => {
                                handle_voice_mute(&state, &v, &user_name, voice_channel).await;
                            }
                            "voice-hand" => {
                                hands::handle_voice_hand(&state, &v, &user_name, voice_channel).await;
                            }
                            "kick-user" => {
                                moderation::handle_kick_user(&state, &mut sender, &v, &user_name).await;
                            }
                            "ban-user" => {
                                moderation::handle_ban_user(&state, &mut sender, &v, &user_name).await;
                            }
                            "unban-user" => {
                                moderation::handle_unban_user(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-ban-list" => {
                                moderation::handle_get_ban_list(&state, &mut sender, &user_name).await;
                            }
                            "get-audit-log" => {
                                audit::handle_get_audit_log(&state, &mut sender, &user_name).await;
                            }
                            "mute-user" => {
                                moderation::handle_mute_user(&state, &mut sender, &v, &user_name).await;
                            }
                            "unmute-user" => {
                                moderation::handle_unmute_user(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-user-roles" => {
                                roles::handle_set_user_roles(&state, &mut sender, &v, &user_name).await;
                            }
                            "create-role" => {
                                roles::handle_create_role(&state, &mut sender, &v, &user_name).await;
                            }
                            "update-role" => {
                                roles::handle_update_role(&state, &mut sender, &v, &user_name).await;
                            }
                            "delete-role" => {
                                roles::handle_delete_role(&state, &mut sender, &v, &user_name).await;
                            }
                            "reorder-roles" => {
                                roles::handle_reorder_roles(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-channel-override" => {
                                channel_overrides::handle_set_channel_override(&state, &mut sender, &v, &user_name).await;
                            }
                            "remove-channel-override" => {
                                channel_overrides::handle_remove_channel_override(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-channel-keys" => {
                                channel_keys::handle_get_channel_keys(&state, &mut sender, &v, &user_name).await;
                            }
                            "put-channel-keys" => {
                                channel_keys::handle_put_channel_keys(&state, &mut sender, &v, &user_name).await;
                            }
                            "set-channel-e2ee" => {
                                channel_keys::handle_set_channel_e2ee(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-channel-overrides" => {
                                channel_overrides::handle_get_channel_overrides(&state, &mut sender, &v, &user_name).await;
                            }
                            "add-emoji" => {
                                emojis::handle_add_emoji(&state, &mut sender, &v, &user_name).await;
                            }
                            "remove-emoji" => {
                                emojis::handle_remove_emoji(&state, &mut sender, &v, &user_name).await;
                            }
                            "add-sound" => {
                                soundboard::handle_add_sound(&state, &mut sender, &v, &user_name).await;
                            }
                            "rename-sound" => {
                                soundboard::handle_rename_sound(&state, &mut sender, &v, &user_name).await;
                            }
                            "remove-sound" => {
                                soundboard::handle_remove_sound(&state, &mut sender, &v, &user_name).await;
                            }
                            "play-sound" => {
                                soundboard::handle_play_sound(&state, &mut sender, &v, &voice_channel, &user_name).await;
                            }
                            "set-server-identity" => {
                                identity::handle_set_server_identity(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-invites" => {
                                invites::handle_get_invites(&state, &mut sender, &user_name).await;
                            }
                            "create-invite" => {
                                invites::handle_create_invite(&state, &mut sender, &v, &user_name).await;
                            }
                            "revoke-invite" => {
                                invites::handle_revoke_invite(&state, &mut sender, &v, &user_name).await;
                            }
                            "schedule-message" => {
                                scheduled::handle_schedule_message(&state, &mut sender, &mut v, &user_name).await;
                            }
                            "cancel-scheduled-message" => {
                                scheduled::handle_cancel_scheduled_message(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-scheduled-messages" => {
                                scheduled::handle_get_scheduled_messages(&state, &mut sender, &user_name).await;
                            }
                            "set-reminder" => {
                                scheduled::handle_set_reminder(&state, &mut sender, &v, &user_name).await;
                            }
                            "cancel-reminder" => {
                                scheduled::handle_cancel_reminder(&state, &mut sender, &v, &user_name).await;
                            }
                            "get-reminders" => {
                                scheduled::handle_get_reminders(&state, &mut sender, &user_name).await;
                            }
                            _ => {
                                warn!("unknown message type: {t}");
                            }
                        }
                    }
                } else {
                    // Client input, not a server fault, and never logged
                    // verbatim: it can be large and can carry message text.
                    warn!(len = text.len(), "invalid json frame");
                }
            }
            // Frames addressed to this connection's user alone. No filtering
            // is needed here: the routing already decided the recipient.
            Some(frame) = direct_rx.recv() => {
                if sender.send(Message::Text(frame)).await.is_err() { break; }
            }
            result = chan_rx.recv() => {
                // Every connection starts subscribed to the default channel
                // before anyone knows who it is, and stays subscribed to the
                // channel it joined after its access to it is taken away. So
                // the subscription is not the gate: nothing is delivered
                // before authentication, and nothing from a channel the user
                // may not (or no longer) see.
                if !authenticated
                    || !visibility.can_receive(&state, user_name.as_deref(), ChannelKind::Text, channel_id).await
                {
                    continue;
                }
                match result {
                    Ok(msg) => {
                        // The frame arrives ready to send: `recv` handed us a
                        // refcounted clone, not a copy of the payload.
                        if sender.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        lagged(user_name.as_deref(), "channel", skipped);
                        // The newest page holds every message, edit, reaction
                        // and deletion missed; the client merges it by id.
                        // It cannot have missed more messages than frames.
                        let limit = (skipped as i64)
                            .saturating_add(DEFAULT_HISTORY_LIMIT)
                            .min(MAX_HISTORY_LIMIT);
                        db::send_history(&state.db, &mut sender, channel_id, None, limit).await;
                    }
                    Err(_) => break,
                }
            }
            result = global_rx.recv() => {
                // The password protects reading as much as posting.
                if !authenticated {
                    continue;
                }
                match result {
                    Ok(routed) => {
                        // The sender decided who this frame is for; nothing
                        // here parses it again.
                        let (frame, route) = &*routed;
                        match route {
                            crate::Route::All => {}
                            // Channel permissions changed: rebuild this
                            // connection's own filtered channel/voice lists.
                            crate::Route::ChannelsRefresh => {
                                send_channels(&state, &mut sender, user_name.as_deref()).await;
                                send_voice_channels(&state, &mut sender, user_name.as_deref()).await;
                                continue;
                            }
                            // Direct messages must only reach their two participants.
                            crate::Route::Dm { .. } => {
                                if route.withholds_dm_from(user_name.as_deref()) {
                                    continue;
                                }
                            }
                            // Channel-scoped frames must not reach a user who
                            // cannot see the channel. Resolved once per
                            // channel and memoised until the permissions that
                            // decide it change.
                            crate::Route::Channel(kind, id) => {
                                if !visibility.can_receive(&state, user_name.as_deref(), *kind, *id).await {
                                    continue;
                                }
                            }
                            crate::Route::ForceDisconnect(_) => {}
                        }

                        // A force-disconnect broadcast targeting this user
                        // (kick/ban) is forwarded so the client learns why,
                        // then the connection is closed.
                        let targets_this_user = matches!(
                            route,
                            crate::Route::ForceDisconnect(u) if Some(u.as_str()) == user_name.as_deref()
                        );
                        if sender.send(Message::Text(frame.clone())).await.is_err() { break; }
                        if targets_this_user {
                            info!("Closing connection after force-disconnect");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        lagged(user_name.as_deref(), "global", skipped);
                        if let Some(user) = user_name.as_deref() {
                            resync_global(&state, &mut sender, user, voice_channel).await;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    if let Some(user) = registered_as.as_deref() {
        unregister_direct(&state, user, conn_id).await;
    }
    state.upload_sessions.lock().await.remove(&challenge);
    handle_disconnect(&state, user_name, voice_channel).await;
    info!(%client_ip, "Client disconnected");
}

/// Record that a connection fell behind a broadcast channel and lost frames.
///
/// The frames themselves are gone; the receive arms re-send current state in
/// their place (the channel's newest history page, or [`resync_global`]). The
/// warning is still the only way to tell a slow client from a server bug
/// when somebody reports a message that appeared late. The socket handler's
/// span already carries the client IP.
fn lagged(user: Option<&str>, receiver: &str, skipped: u64) {
    warn!(
        user = user.unwrap_or("-"),
        receiver, skipped, "Connection fell behind the broadcast; frames were dropped"
    );
}

/// Bring a connection that fell behind the server-wide broadcast back to the
/// current state, instead of closing it: the client does not reconnect on its
/// own, so a closed socket would read as "Connection lost" and drop a call.
///
/// Snapshots cover everything the server can see this connection needs,
/// including the voice channel it sits in. `resync` then tells the client to
/// re-fetch what only it knows is open — an active DM conversation.
///
/// ponytail: a connection that keeps lagging gets a full snapshot per lag,
/// which adds to the backlog it is already failing to drain. Coalesce into
/// one pending resync if that ever shows up in the lag warnings.
async fn resync_global(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
    voice_channel: Option<i32>,
) {
    auth::send_state_snapshot(state, sender, user).await;
    if let Some(ch_id) = voice_channel {
        send_voice_channel_state(state, sender, ch_id).await;
    }
    send_json(sender, &serde_json::json!({ "type": "resync" })).await;
}

/// Whether a relayed frame's `user` field names the connection's own
/// authenticated user. Prevents spoofing other users in signaling frames.
fn claims_own_user(v: &Value, user_name: &Option<String>) -> bool {
    match user_name.as_deref() {
        Some(name) => v.get("user").and_then(|u| u.as_str()) == Some(name),
        None => false,
    }
}

/// Whether a frame's `channelId` is the voice channel this connection is in.
/// A screen share or camera is announced to that channel's members, so
/// without this anyone could post one into any channel, private ones
/// included, where it would sit as a tile that never connects.
fn names_own_voice_channel(v: &Value, voice_channel: Option<i32>) -> bool {
    voice_channel.is_some() && i32_field(v, "channelId") == voice_channel
}

/// Whether a signaling frame stays inside the sender's voice channel: it
/// names the channel this connection joined, and its `target` sits in that
/// channel too.
///
/// A client answers any offer that names its own channel, and the sender
/// writes that field. Without this check a member who may not join a private
/// call could offer to someone inside it and receive their microphone and
/// camera, or offer to a sharer and receive their screen. Watching a share
/// already requires being in its voice channel, so screen-share signaling
/// is held to the same rule.
async fn signals_within_own_voice_channel(
    state: &Arc<AppState>,
    v: &Value,
    voice_channel: Option<i32>,
) -> bool {
    let (Some(channel), Some(target)) = (voice_channel, v.get("target").and_then(|t| t.as_str()))
    else {
        return false;
    };
    names_own_voice_channel(v, voice_channel)
        && state
            .voice_channels
            .lock()
            .await
            .get(&channel)
            .is_some_and(|info| info.users.contains(target))
}

async fn handle_status_update(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let user = match user_name.clone() {
        Some(name) => name,
        None => {
            send_error(sender, errors::NOT_AUTHENTICATED).await;
            return;
        }
    };

    let Some(raw_status) = v.get("status").and_then(|s| s.as_str()) else {
        send_error(sender, errors::INVALID_STATUS).await;
        return;
    };

    let Some(status) = normalize_status(raw_status) else {
        send_error(sender, errors::INVALID_STATUS).await;
        return;
    };
    // Every change reaches every connection, so it is paced like a message.
    // No mute check: online/away is not something a member says.
    if !crate::security::check_message_rate_limit(&state.rate_limiter, &user).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return;
    }

    state
        .statuses
        .lock()
        .await
        .insert(user.clone(), status.to_string());
    broadcast_status(state, &user, status);
}

/// Handle `poke`: deliver a short nudge to one online member, which their
/// client shows even when every channel is muted (TeamSpeak's poke).
///
/// Anyone authenticated may poke, like sending a DM; a server mute silences
/// it, and a per-sender cooldown keeps it a nudge. Blocking is the
/// recipient's client's job, as it is for DMs — the server keeps no block
/// list. It goes direct: nobody but the target has any use for the frame.
async fn handle_poke(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(from) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let Some(target) = v.get("target").and_then(|t| t.as_str()) else {
        send_error(sender, errors::POKE_UNAVAILABLE).await;
        return;
    };
    if target == from || !state.direct.lock().await.contains_key(target) {
        send_error(sender, errors::POKE_UNAVAILABLE).await;
        return;
    }
    if moderation::is_muted(state, from).await {
        send_error(sender, errors::MUTED).await;
        return;
    }
    {
        let cooldown = std::time::Duration::from_millis(super::constants::POKE_COOLDOWN_MS);
        let now = std::time::Instant::now();
        let mut last = state.poke_cooldowns.lock().await;
        if last
            .get(from)
            .is_some_and(|at| now.duration_since(*at) < cooldown)
        {
            send_error(sender, errors::POKE_COOLDOWN).await;
            return;
        }
        last.insert(from.to_string(), now);
    }
    let frame = serde_json::json!({ "type": "poke", "from": from });
    send_to_user(state, target, frame.to_string().into()).await;
}

async fn handle_ping(sender: &mut SplitSink<WebSocket, Message>, v: &Value) {
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let msg = serde_json::json!({ "type": "pong", "id": id });
    send_json(sender, &msg).await;
}

/// Handle a request for server details (currently the running version).
/// Only answered for users whose role is in `SERVER_INFO_ROLES`; the check
/// runs against the server-side role map, so clients cannot spoof access.
/// Unauthorised requests are dropped without an error frame: clients send
/// this automatically based on their (possibly stale) view of their own
/// role, so a denial must not surface as a user-facing error.
async fn handle_get_server_info(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        return;
    };

    if !has_permission(state, requester, crate::permissions::VIEW_SERVER_INFO).await {
        info!(requester, "Denied server info request");
        return;
    }

    let msg = serde_json::json!({
        "type": "server-info",
        "version": env!("CARGO_PKG_VERSION"),
    });
    send_json(sender, &msg).await;
}

/// Handle `get-server-metrics`: answer with the live operator counters —
/// connections, frames taken in, database latency and rate-limit rejections.
///
/// Gated on `MANAGE_SERVER` like the storage report, and dropped silently
/// rather than answered with an error for the same reason as
/// `get-server-info`: the dashboard polls this on a timer, so a denial would
/// otherwise become a stream of error toasts.
///
/// The counters are cumulative and the frame carries the uptime with them, so
/// a viewer can turn two samples into the rate over its own polling interval
/// without the server keeping a window per viewer. See
/// [`crate::metrics`].
async fn handle_get_server_metrics(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        return;
    };

    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        info!(requester, "Denied server metrics request");
        return;
    }

    let m = crate::metrics::snapshot();
    let msg = serde_json::json!({
        "type": "server-metrics",
        "uptimeSeconds": m.uptime_secs,
        "connections": m.connections,
        "peakConnections": m.peak_connections,
        "frames": m.frames,
        "dbCalls": m.db_calls,
        "dbTotalMs": m.db_total_ms,
        "dbMaxMs": m.db_max_ms,
        "rejectedMessages": m.rejected_messages,
        "rejectedAuth": m.rejected_auth,
        "rejectedUploads": m.rejected_uploads,
        "rejectedFrames": m.rejected_frames,
        "rejectedPreviews": m.rejected_previews,
    });
    send_json(sender, &msg).await;
}

/// Store a client's self-reported connection quality numbers (ping, voice
/// RTT/jitter/packet loss). The entry is keyed by the authenticated user name
/// so clients cannot report on behalf of someone else, values are validated
/// and clamped, and everything stays in memory only (dropped on disconnect).
async fn handle_connection_stats(state: &Arc<AppState>, v: &Value, user_name: &Option<String>) {
    let Some(user) = user_name.as_deref() else {
        return;
    };

    fn stat(v: &Value, key: &str, max: f64) -> Option<f64> {
        v.get(key)?
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .map(|n| n.min(max))
    }

    let entry = crate::ConnectionStatsEntry {
        ping_ms: stat(v, "ping", super::constants::MAX_REPORTED_STAT_MS),
        voice_rtt_ms: stat(v, "voiceRtt", super::constants::MAX_REPORTED_STAT_MS),
        voice_jitter_ms: stat(v, "voiceJitter", super::constants::MAX_REPORTED_STAT_MS),
        voice_loss_percent: stat(v, "voiceLoss", 100.0),
        updated_at: std::time::Instant::now(),
    };
    state
        .connection_stats
        .lock()
        .await
        .insert(user.to_string(), entry);
}

/// Send every user's latest self-reported connection stats to the requester.
/// Only answered for Owner/Admin roles, checked against the server-side role
/// map so clients cannot spoof access. Unauthorised requests are dropped
/// silently, mirroring `get-server-info`.
async fn handle_get_connection_stats(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        return;
    };

    if !has_permission(state, requester, crate::permissions::VIEW_CONNECTION_STATS).await {
        info!(requester, "Denied connection stats request");
        return;
    }

    let stats: serde_json::Map<String, Value> = {
        let map = state.connection_stats.lock().await;
        map.iter()
            .map(|(user, entry)| {
                (
                    user.clone(),
                    serde_json::json!({
                        "ping": entry.ping_ms,
                        "voiceRtt": entry.voice_rtt_ms,
                        "voiceJitter": entry.voice_jitter_ms,
                        "voiceLoss": entry.voice_loss_percent,
                        "ageSeconds": entry.updated_at.elapsed().as_secs(),
                    }),
                )
            })
            .collect()
    };
    let msg = serde_json::json!({
        "type": "connection-stats-list",
        "stats": stats,
    });
    send_json(sender, &msg).await;
}

async fn handle_voice_join(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    voice_channel: &mut Option<i32>,
    user_name: &Option<String>,
) {
    let Some(u) = user_name.as_deref() else {
        return;
    };
    if let Some(ch_id) = i32_field(v, "channelId") {
        // A private voice channel is join-gated by View (see + join).
        if !can_view_channel(state, u, ChannelKind::Voice, ch_id).await {
            return;
        }
        let mut map = state.voice_channels.lock().await;
        let Some(entry) = map.get(&ch_id) else {
            return;
        };
        // Capacity is decided before the user is pulled out of whatever
        // channel they are in, so a full target leaves them where they were
        // instead of dropping them out of voice entirely.
        if !crate::security::voice_channel_has_room(&entry.users, u, entry.user_limit) {
            drop(map);
            info!("voice channel {ch_id} is full; refused join from {u}");
            send_error(sender, errors::VOICE_CHANNEL_FULL).await;
            return;
        }
        for info in map.values_mut() {
            info.users.remove(u);
        }
        if let Some(entry) = map.get_mut(&ch_id) {
            entry.users.insert(u.to_string());
        }
        *voice_channel = Some(ch_id);
        drop(map);
        // Switching channels: a hand raised in the old one does not follow.
        if state
            .voice_hands
            .lock()
            .await
            .get(u)
            .is_some_and(|(ch, _)| *ch != ch_id)
        {
            hands::lower_hand(state, u).await;
        }
        stats::note_voice_join(state, u).await;
        broadcast_voice(state, ch_id).await;
        let msg = serde_json::json!({
            "type": "voice-join",
            "user": u,
            "channelId": ch_id,
        });
        broadcast(state, &msg);

        // Tell the joiner whether they may speak here (Talk = SEND in the
        // channel). Voice audio is peer-to-peer, so the client enforces this by
        // disabling its microphone; the server enforces View/join only.
        let can_speak = has_channel_permission(
            state,
            u,
            ChannelKind::Voice,
            ch_id,
            crate::permissions::SEND_MESSAGES,
        )
        .await;
        let perms = serde_json::json!({
            "type": "voice-permissions",
            "channelId": ch_id,
            "canSpeak": can_speak,
        });
        send_json(sender, &perms).await;

        send_voice_channel_state(state, sender, ch_id).await;
    }
}

async fn handle_voice_leave(
    state: &Arc<AppState>,
    v: &Value,
    voice_channel: &mut Option<i32>,
    user_name: &Option<String>,
) {
    let Some(u) = user_name.as_deref() else {
        return;
    };
    if let Some(ch_id) = i32_field(v, "channelId") {
        let mut map = state.voice_channels.lock().await;
        if let Some(info) = map.get_mut(&ch_id) {
            info.users.remove(u);
        }
        drop(map);
        state.voice_mutes.lock().await.remove(u);
        hands::lower_hand(state, u).await;
        stats::flush_voice_session(state, u).await;
        stats::flush_screenshare_session(state, u).await;
        end_tiles_for_user(state, u).await;
        broadcast_voice(state, ch_id).await;
        if *voice_channel == Some(ch_id) {
            *voice_channel = None;
        }
        let msg = serde_json::json!({
            "type": "voice-leave",
            "user": u,
            "channelId": ch_id,
        });
        broadcast(state, &msg);
    }
}

/// Screen shares and cameras are both a set of users per voice channel,
/// switched by a start/stop frame and snapshotted for whoever joins. These
/// functions serve both maps.
type Tiles = Mutex<HashMap<i32, HashSet<String>>>;

/// Record the tile a checked start frame announces.
async fn tile_start(tiles: &Tiles, v: &Value) {
    let (Some(user), Some(ch_id)) = (
        v.get("user").and_then(|u| u.as_str()),
        i32_field(v, "channelId"),
    ) else {
        return;
    };
    tiles
        .lock()
        .await
        .entry(ch_id)
        .or_default()
        .insert(user.to_string());
}

/// Drop the tile a checked stop frame names.
async fn tile_stop(tiles: &Tiles, v: &Value) {
    let (Some(user), Some(ch_id)) = (
        v.get("user").and_then(|u| u.as_str()),
        i32_field(v, "channelId"),
    ) else {
        return;
    };
    let mut map = tiles.lock().await;
    if let Some(set) = map.get_mut(&ch_id) {
        set.remove(user);
        if set.is_empty() {
            map.remove(&ch_id);
        }
    }
}

/// End every screen share and camera owned by `user` and announce each end
/// to all clients. Neither can outlive the voice session that carries it, so
/// this runs on voice-leave and on disconnect; a well-behaved client sends
/// the explicit stop first, in which case this is a no-op.
async fn end_tiles_for_user(state: &Arc<AppState>, user: &str) {
    for (tiles, stop_type) in [
        (&state.active_screen_shares, "screenshare-stop"),
        (&state.active_webcams, "webcam-stop"),
    ] {
        let channels: Vec<i32> = {
            let mut map = tiles.lock().await;
            let channels: Vec<i32> = map
                .iter()
                .filter(|(_, users)| users.contains(user))
                .map(|(ch_id, _)| *ch_id)
                .collect();
            for ch_id in &channels {
                if let Some(set) = map.get_mut(ch_id) {
                    set.remove(user);
                    if set.is_empty() {
                        map.remove(ch_id);
                    }
                }
            }
            channels
        };
        for ch_id in channels {
            broadcast(
                state,
                &serde_json::json!({ "type": stop_type, "user": user, "channelId": ch_id }),
            );
        }
    }
}

/// Send a voice channel's member state to one client that just joined it or
/// fell behind: the screen shares and cameras already on, the mutes and the
/// raised hands.
async fn send_voice_channel_state(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    ch_id: i32,
) {
    for (tiles, frame_type) in [
        (&state.active_screen_shares, "screenshare-active"),
        (&state.active_webcams, "webcam-active"),
    ] {
        let users: Vec<String> = tiles
            .lock()
            .await
            .get(&ch_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        if !users.is_empty() {
            send_json(
                sender,
                &serde_json::json!({ "type": frame_type, "channelId": ch_id, "users": users }),
            )
            .await;
        }
    }
    send_voice_mutes(state, sender, ch_id).await;
    hands::send_voice_hands(state, sender, ch_id).await;
}

/// Record the sender's voice mute state (microphone / output) and tell
/// their voice channel. The outgoing frame is rebuilt from the checked
/// fields rather than relayed, so nothing else a client adds travels with
/// it, and it is scoped to the channel so members who cannot see a private
/// voice channel do not learn who sits in it.
async fn handle_voice_mute(
    state: &Arc<AppState>,
    v: &Value,
    user_name: &Option<String>,
    voice_channel: Option<i32>,
) {
    let (Some(user), Some(ch_id)) = (user_name.as_deref(), voice_channel) else {
        return;
    };
    // Only into the channel the connection actually sits in, as for
    // `voice-hand`: the route follows `channelId`.
    if !names_own_voice_channel(v, voice_channel) {
        return;
    }
    let mic_muted = v.get("micMuted").and_then(|m| m.as_bool()).unwrap_or(false);
    let output_muted = v
        .get("outputMuted")
        .and_then(|m| m.as_bool())
        .unwrap_or(false);
    state
        .voice_mutes
        .lock()
        .await
        .insert(user.to_string(), (mic_muted, output_muted));
    broadcast(
        state,
        &serde_json::json!({
            "type": "voice-mute",
            "user": user,
            "channelId": ch_id,
            "micMuted": mic_muted,
            "outputMuted": output_muted,
        }),
    );
}

/// Send the current mute states of everyone in a voice channel to a single client.
async fn send_voice_mutes(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    channel_id: i32,
) {
    let members: HashSet<String> = {
        let channels = state.voice_channels.lock().await;
        channels
            .get(&channel_id)
            .map(|info| info.users.clone())
            .unwrap_or_default()
    };
    if members.is_empty() {
        return;
    }
    let states: serde_json::Map<String, Value> = {
        let mutes = state.voice_mutes.lock().await;
        members
            .iter()
            .filter_map(|user| {
                mutes.get(user).map(|(mic, output)| {
                    (
                        user.clone(),
                        serde_json::json!({ "micMuted": mic, "outputMuted": output }),
                    )
                })
            })
            .collect()
    };
    if states.is_empty() {
        return;
    }
    send_json(
        sender,
        &serde_json::json!({
            "type": "voice-mute-active",
            "channelId": channel_id,
            "states": states,
        }),
    )
    .await;
}

/// Send the ICE servers for WebRTC to a single client, right after
/// authentication. Voice and screen share build every peer connection from
/// this, so the operator — not a URL compiled into the client — decides whom
/// a call contacts. The shape is `RTCConfiguration.iceServers`, so the client
/// can pass it through.
async fn send_ice_config(state: &Arc<AppState>, sender: &mut SplitSink<WebSocket, Message>) {
    let ice_servers: Vec<Value> = state
        .stun_servers
        .iter()
        .map(|url| serde_json::json!({ "urls": url }))
        .collect();
    send_json(
        sender,
        &serde_json::json!({ "type": "ice-config", "iceServers": ice_servers }),
    )
    .await;
}

/// End `user`'s voice session: bank its time, take them out of their
/// channel, and stop the screen shares and cameras that rode on it.
async fn end_voice_session(state: &Arc<AppState>, user: &str) {
    stats::flush_voice_session(state, user).await;
    stats::flush_screenshare_session(state, user).await;

    let left = state
        .voice_channels
        .lock()
        .await
        .iter_mut()
        .find_map(|(id, info)| info.users.remove(user).then_some(*id));
    if let Some(ch_id) = left {
        broadcast_voice(state, ch_id).await;
    }

    state.voice_mutes.lock().await.remove(user);
    hands::lower_hand(state, user).await;
    end_tiles_for_user(state, user).await;
}

/// Clean up after a closed connection. Its direct mailbox must already be
/// unregistered, since that registry is how the other connections of the
/// same account are found.
///
/// One account may be signed in from several clients at once. Presence
/// belongs to the account, so the user only goes offline with their last
/// connection. A voice session belongs to the connection that joined it, so
/// closing a text-only tab must not pull the desktop app out of a call.
async fn handle_disconnect(
    state: &Arc<AppState>,
    user_name: Option<String>,
    voice_channel: Option<i32>,
) {
    let Some(name) = user_name else {
        return;
    };
    let last_connection = !state.direct.lock().await.contains_key(&name);
    let owns_voice_session = match voice_channel {
        Some(ch_id) => state
            .voice_channels
            .lock()
            .await
            .get(&ch_id)
            .is_some_and(|info| info.users.contains(&name)),
        None => false,
    };

    if last_connection || owns_voice_session {
        end_voice_session(state, &name).await;
    }
    if !last_connection {
        return;
    }

    state.users.lock().await.remove(&name);
    broadcast_users(state).await;
    state.connection_stats.lock().await.remove(&name);
    soundboard::clear_cooldown(state, &name).await;
    state.poke_cooldowns.lock().await.remove(&name);
    // Slow mode paces a live conversation; a member who leaves and comes
    // back is not made to wait out an interval they never spent typing.
    state.slow_mode_sends.lock().await.remove(&name);
    state
        .statuses
        .lock()
        .await
        .insert(name.clone(), "offline".to_string());
    broadcast_status(state, &name, "offline");
}

/// Axum handler that upgrades the HTTP connection to a WebSocket and spawns message processing.
#[instrument(skip(ws, state), fields(client_addr = %addr))]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let client_ip = crate::security::client_ip(&state.trusted_proxies, addr.ip(), &headers);
    ws.max_message_size(MAX_WS_MESSAGE_BYTES)
        .max_frame_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state, client_ip))
}
