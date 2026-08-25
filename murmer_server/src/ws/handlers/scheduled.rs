//! Reminders and scheduled messages, plus the background scheduler that
//! delivers both.
//!
//! The two features share one timer and nothing else. A **reminder** is a
//! private note the server hands back to its owner at a chosen time; a
//! **scheduled message** is an ordinary channel message written now and posted
//! later, as its author.
//!
//! Three decisions here are worth knowing before changing anything:
//!
//! - **A scheduled message is screened when it is written, and authorized
//!   again when it is posted.** Screening (auto-moderation, the profanity
//!   mask, the length cap) happens at compose time because that is the only
//!   moment its author is present to be told why it was refused. Authorization
//!   — can this account still see the channel, still send in it, is it muted,
//!   is the channel still encrypted the same way — is re-checked at delivery,
//!   because all four can change in the meantime and the server is the only
//!   enforcement point. A message that fails the second check is kept and
//!   marked failed, never posted and never silently dropped.
//! - **Reminders are not end-to-end encrypted.** The note is written by the
//!   user and stored in plaintext, like a channel topic. A reminder set on a
//!   message in an encrypted channel therefore stores only what its owner
//!   typed — the client never copies the decrypted message into it. See
//!   `docs/security.md`.
//! - **Both lists are answers, not broadcasts.** They concern exactly one
//!   account, so every frame here leaves through that user's direct mailbox.

use crate::channel_overrides::ChannelKind;
use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::time::{Duration, MissedTickBehavior, interval};
use tracing::{error, info, warn};

/// Marker written into `failed_reason` for a row left claimed by a process
/// that stopped mid-delivery. Distinct from the authorization refusals so an
/// author can tell "the server restarted" from "you may no longer post here".
const INTERRUPTED_REASON: &str = "interrupted";

/// Read and bound a scheduling timestamp. A time inside
/// [`MIN_SCHEDULE_LEAD_SECONDS`] is refused rather than clamped: unlike an
/// ephemeral expiry, "when" is the whole point of the request, so silently
/// moving it would deliver something the user did not ask for.
fn parse_schedule_time(v: &Value, field: &str) -> Option<DateTime<Utc>> {
    let raw = v.get(field)?.as_str()?;
    let parsed = DateTime::parse_from_rfc3339(raw).ok()?.with_timezone(&Utc);
    let now = Utc::now();
    if parsed < now + ChronoDuration::seconds(MIN_SCHEDULE_LEAD_SECONDS)
        || parsed > now + ChronoDuration::seconds(MAX_SCHEDULE_AHEAD_SECONDS)
    {
        return None;
    }
    Some(parsed)
}

/// One scheduled message as the client sees it. The stored body travels along
/// so the author's list can render what they wrote — including the sealed
/// envelope of an encrypted channel, which only they can open.
fn scheduled_entry(row: &db::ScheduledMessage) -> Value {
    let body: Value = serde_json::from_str(&row.body).unwrap_or(Value::Null);
    serde_json::json!({
        "id": row.id,
        "channelId": row.channel_id,
        "scheduledFor": row.scheduled_for,
        "createdAt": row.created_at,
        "failedReason": row.failed_reason,
        "text": body.get("text").cloned().unwrap_or(Value::Null),
        "enc": body.get("enc").cloned().unwrap_or(Value::Null),
    })
}

fn reminder_entry(row: &db::Reminder) -> Value {
    serde_json::json!({
        "id": row.id,
        "text": row.text,
        "remindAt": row.remind_at,
        "channelId": row.channel_id,
        "messageId": row.message_id,
        "createdAt": row.created_at,
        "firedAt": row.fired_at,
    })
}

async fn scheduled_payload(state: &Arc<AppState>, user: &str) -> Option<String> {
    match db::get_scheduled_messages(&state.db, user).await {
        Ok(rows) => Some(
            serde_json::json!({
                "type": "scheduled-messages",
                "items": rows.iter().map(scheduled_entry).collect::<Vec<_>>(),
            })
            .to_string(),
        ),
        Err(e) => {
            error!("Failed to load scheduled messages for {user}: {e}");
            None
        }
    }
}

async fn reminders_payload(state: &Arc<AppState>, user: &str) -> Option<String> {
    match db::get_reminders(&state.db, user).await {
        Ok(rows) => Some(
            serde_json::json!({
                "type": "reminders",
                "items": rows.iter().map(reminder_entry).collect::<Vec<_>>(),
            })
            .to_string(),
        ),
        Err(e) => {
            error!("Failed to load reminders for {user}: {e}");
            None
        }
    }
}

/// Send a user's scheduled messages down one specific socket (the presence
/// snapshot).
pub(super) async fn send_scheduled_messages(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
) {
    if let Some(payload) = scheduled_payload(state, user).await {
        let _ = sender.send(Message::Text(payload.into())).await;
    }
}

/// Send a user's reminders down one specific socket (the presence snapshot).
pub(super) async fn send_reminders(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
) {
    if let Some(payload) = reminders_payload(state, user).await {
        let _ = sender.send(Message::Text(payload.into())).await;
    }
}

/// Push a fresh list to every connection the user has open, so their other
/// devices see a change made on this one. A user with nothing open simply
/// misses it and gets the snapshot on their next `presence`.
async fn push_scheduled_messages(state: &Arc<AppState>, user: &str) {
    if let Some(payload) = scheduled_payload(state, user).await {
        send_to_user(state, user, payload.into()).await;
    }
}

async fn push_reminders(state: &Arc<AppState>, user: &str) {
    if let Some(payload) = reminders_payload(state, user).await {
        send_to_user(state, user, payload.into()).await;
    }
}

/// Handle `schedule-message`: store a message to be posted later.
///
/// The frame is the `chat` frame it will eventually become, plus a
/// `scheduledFor`. It goes through the same authorization and the same
/// screening a message sent now would — see [`super::messages::authorize_send`]
/// and [`super::messages::prepare_chat_body`] — so scheduling is never a way
/// past a rule that applies to typing.
pub(super) async fn handle_schedule_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &mut Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let Some(channel_id) = v
        .get("channelId")
        .and_then(|c| c.as_i64())
        .and_then(|c| i32::try_from(c).ok())
    else {
        send_error(sender, errors::UNKNOWN_CHANNEL).await;
        return;
    };
    let Some(scheduled_for) = parse_schedule_time(v, "scheduledFor") else {
        send_error(sender, errors::INVALID_SCHEDULE_TIME).await;
        return;
    };
    // A channel with no overrides is visible to everyone, which means an id
    // that names nothing at all clears the permission check below. Live
    // messages find that out at the insert; a scheduled one has to be told
    // now, or it is queued against a foreign key that will never resolve.
    if db::get_channel_by_id(&state.db, channel_id).await.is_none() {
        send_error(sender, errors::UNKNOWN_CHANNEL).await;
        return;
    }

    if !super::messages::authorize_send(state, sender, channel_id, &user).await {
        return;
    }
    // Slow mode is deliberately not consulted here. It paces a live
    // conversation, and this message is not entering one yet; the interval it
    // owes is started when the scheduler actually posts it, by the same
    // `publish_message` every other message goes through.
    if super::messages::prepare_chat_body(state, sender, v, channel_id, &user)
        .await
        .is_err()
    {
        return;
    }
    // Whatever the client tagged on beyond a message body is not part of one.
    if let Some(map) = v.as_object_mut() {
        map.remove("scheduledFor");
        map.remove("ephemeral");
        map.remove("expiresAt");
    }
    v["type"] = Value::String("chat".into());

    let body = v.to_string();
    match db::insert_scheduled_message(
        &state.db,
        &user,
        channel_id,
        &body,
        scheduled_for,
        MAX_SCHEDULED_MESSAGES_PER_USER,
    )
    .await
    {
        Ok(Some(id)) => {
            info!(user, id, channel_id, "Message scheduled");
            push_scheduled_messages(state, &user).await;
        }
        Ok(None) => send_error(sender, errors::SCHEDULE_LIMIT_REACHED).await,
        Err(e) => {
            error!("Failed to schedule a message for {user}: {e}");
            send_error(sender, errors::SCHEDULE_FAILED).await;
        }
    }
}

/// Handle `cancel-scheduled-message`. Ownership is enforced inside the
/// statement, so an id belonging to somebody else reads as "not found".
pub(super) async fn handle_cancel_scheduled_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
        send_error(sender, errors::SCHEDULED_MESSAGE_NOT_FOUND).await;
        return;
    };
    match db::cancel_scheduled_message(&state.db, &user, id).await {
        Ok(true) => push_scheduled_messages(state, &user).await,
        Ok(false) => send_error(sender, errors::SCHEDULED_MESSAGE_NOT_FOUND).await,
        Err(e) => {
            error!("Failed to cancel scheduled message {id} for {user}: {e}");
            send_error(sender, errors::SCHEDULE_FAILED).await;
        }
    }
}

/// Handle `get-scheduled-messages`: a user's own queue, and nobody else's.
pub(super) async fn handle_get_scheduled_messages(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    send_scheduled_messages(state, sender, user).await;
}

/// Handle `set-reminder`.
///
/// Any authenticated user may set one for themselves; there is no permission
/// to check, because a reminder never reaches anybody else. A reminder that
/// points at a message must point at one its owner can actually read, which
/// is the one place this frame consults the channel rules.
pub(super) async fn handle_set_reminder(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
    if text.is_empty() || text.len() > MAX_REMINDER_TEXT_LENGTH {
        send_error(sender, errors::INVALID_REMINDER).await;
        return;
    }
    let Some(remind_at) = parse_schedule_time(v, "remindAt") else {
        send_error(sender, errors::INVALID_SCHEDULE_TIME).await;
        return;
    };

    // An attached message is resolved through the stored row rather than
    // trusted from the frame, so the channel a reminder claims to point at is
    // the channel the message is really in.
    let mut target = None;
    if let Some(message_id) = v.get("messageId").and_then(|m| m.as_i64()) {
        match db::get_message_record(&state.db, message_id).await {
            Ok(Some(record))
                if can_view_channel(state, &user, ChannelKind::Text, record.channel_id).await =>
            {
                target = Some((record.channel_id, message_id));
            }
            Ok(_) => {
                send_error(sender, errors::MESSAGE_NOT_FOUND).await;
                return;
            }
            Err(e) => {
                error!("Failed to load reminder target {message_id}: {e}");
                send_error(sender, errors::REMINDER_FAILED).await;
                return;
            }
        }
    }

    match db::insert_reminder(
        &state.db,
        &user,
        text,
        remind_at,
        target,
        MAX_REMINDERS_PER_USER,
    )
    .await
    {
        Ok(Some(id)) => {
            info!(user, id, "Reminder set");
            push_reminders(state, &user).await;
        }
        Ok(None) => send_error(sender, errors::REMINDER_LIMIT_REACHED).await,
        Err(e) => {
            error!("Failed to store a reminder for {user}: {e}");
            send_error(sender, errors::REMINDER_FAILED).await;
        }
    }
}

/// Handle `cancel-reminder`, which is also how a due reminder is dismissed.
pub(super) async fn handle_cancel_reminder(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.clone() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
        send_error(sender, errors::REMINDER_NOT_FOUND).await;
        return;
    };
    match db::cancel_reminder(&state.db, &user, id).await {
        Ok(true) => push_reminders(state, &user).await,
        Ok(false) => send_error(sender, errors::REMINDER_NOT_FOUND).await,
        Err(e) => {
            error!("Failed to cancel reminder {id} for {user}: {e}");
            send_error(sender, errors::REMINDER_FAILED).await;
        }
    }
}

/// Handle `get-reminders`: a user's own reminders, and nobody else's.
pub(super) async fn handle_get_reminders(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(user) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    send_reminders(state, sender, user).await;
}

/// Why a claimed message may not be posted after all, or `None` when it may.
///
/// Everything checked here can change between writing a message and posting
/// it, which is exactly why none of it can be settled at compose time. The
/// encryption arm is the subtle one: a channel that switched to end-to-end
/// encryption after a plaintext message was queued must not receive it, or the
/// server would be the one putting plaintext into an encrypted channel.
async fn delivery_refusal(
    state: &Arc<AppState>,
    user: &str,
    channel_id: i32,
    body: &Value,
) -> Option<&'static str> {
    if !can_view_channel(state, user, ChannelKind::Text, channel_id).await
        || !has_channel_permission(
            state,
            user,
            ChannelKind::Text,
            channel_id,
            crate::permissions::SEND_MESSAGES,
        )
        .await
    {
        return Some("send-permission-denied");
    }
    if super::moderation::is_muted(state, user).await {
        return Some("muted");
    }
    let sealed = body.get("enc").is_some_and(|enc| !enc.is_null());
    match channel_is_e2ee(state, channel_id).await {
        true if !sealed => return Some("channel-requires-encryption"),
        false if sealed => return Some("channel-not-encrypted"),
        _ => {}
    }
    None
}

/// Post one claimed scheduled message, or mark it failed. Every path here ends
/// with the row either gone or carrying a reason, so nothing stays claimed.
async fn deliver_scheduled_message(state: &Arc<AppState>, claimed: db::ClaimedScheduledMessage) {
    let db::ClaimedScheduledMessage {
        id,
        user_name,
        channel_id,
        body,
    } = claimed;

    let Ok(mut body) = serde_json::from_str::<Value>(&body) else {
        error!("scheduled message {id} holds a body that is not JSON; marking it failed");
        fail(state, id, &user_name, "schedule-failed").await;
        return;
    };

    if let Some(reason) = delivery_refusal(state, &user_name, channel_id, &body).await {
        info!(user = user_name, id, reason, "Scheduled message refused");
        fail(state, id, &user_name, reason).await;
        return;
    }

    // The message enters history now, not when it was written, so it is
    // re-stamped: a compose-time timestamp would sort it in among messages
    // that were posted before it.
    let now = Utc::now();
    if let Some(map) = body.as_object_mut() {
        map.remove("time");
    }
    body["timestamp"] = Value::String(now.to_rfc3339());

    super::messages::publish_message(state, &mut body, channel_id, &user_name, &now, None).await;

    if let Err(e) = db::delete_scheduled_message(&state.db, id).await {
        // The message is out; the row is not. Left claimed, it is picked up by
        // the startup sweep and reported to its author rather than re-posted.
        error!("posted scheduled message {id} but failed to clear its row: {e}");
    }
    push_scheduled_messages(state, &user_name).await;
}

async fn fail(state: &Arc<AppState>, id: i64, user: &str, reason: &str) {
    if let Err(e) = db::fail_scheduled_message(&state.db, id, reason).await {
        error!("failed to mark scheduled message {id} as failed: {e}");
    }
    push_scheduled_messages(state, user).await;
}

/// One pass over both queues. Reminders go first: they are cheap and the
/// message deliveries below can each take a database round trip.
async fn tick(state: &Arc<AppState>) {
    let now = Utc::now();

    match db::claim_due_reminders(&state.db, now).await {
        Ok(due) => {
            let mut owners: HashSet<String> = HashSet::new();
            for reminder in due {
                let frame = serde_json::json!({
                    "type": "reminder-due",
                    "id": reminder.id,
                    "text": reminder.text,
                    "remindAt": reminder.remind_at,
                    "channelId": reminder.channel_id,
                    "messageId": reminder.message_id,
                });
                // Best effort: an owner who is offline finds the reminder
                // already marked due in the snapshot they get on reconnect.
                send_to_user(state, &reminder.user_name, frame.to_string().into()).await;
                owners.insert(reminder.user_name);
            }
            for owner in owners {
                push_reminders(state, &owner).await;
            }
        }
        Err(e) => error!("failed to claim due reminders: {e}"),
    }

    match db::claim_due_scheduled_messages(&state.db, now).await {
        Ok(due) => {
            for claimed in due {
                deliver_scheduled_message(state, claimed).await;
            }
        }
        Err(e) => error!("failed to claim due scheduled messages: {e}"),
    }
}

/// Start the scheduler and return.
///
/// One task drains both queues in sequence, so two ticks can never overlap and
/// the per-row claim never has to arbitrate between passes of this process.
/// The interval skips missed ticks rather than firing them back to back — a
/// machine that was suspended for an hour should catch up in one pass, not
/// three hundred and sixty.
pub fn spawn_scheduler(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(SCHEDULER_TICK_SECONDS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            tick(&state).await;
        }
    });
}

/// Clear claims left behind by a process that stopped mid-delivery. Run once
/// at startup, before [`spawn_scheduler`].
pub async fn recover_claimed_scheduled_messages(state: &Arc<AppState>) {
    match db::fail_claimed_scheduled_messages(&state.db, INTERRUPTED_REASON).await {
        Ok(0) => {}
        Ok(n) => warn!("{n} scheduled message(s) were interrupted by a restart and not posted"),
        Err(e) => error!("failed to recover interrupted scheduled messages: {e}"),
    }
}
