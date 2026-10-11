//! Message reports: any member flags a message to the moderators, who work
//! the queue in the Server Dashboard.
//!
//! Filing (`report-message`) needs only that the reporter can see the
//! message's channel — a report about something you cannot see would be a
//! probe for hidden ids. Reading (`list-reports`) and closing
//! (`resolve-report`) need `MANAGE_MESSAGES` *in the reported channel*, so a
//! moderator of one private channel never sees reports from another. The
//! queue is an answer to a request, not a broadcast: it names who reported
//! whom.
//!
//! In an encrypted channel the server cannot read the message, so the queue
//! carries no excerpt there; the moderator opens the message in the channel.

use crate::channel_overrides::ChannelKind;
use crate::ws::{constants::*, errors, helpers::*};
use crate::{AppState, db, security};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

/// Handle `report-message {messageId, reason}`.
pub(super) async fn handle_report_message(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(reporter) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    // Shares the message budget: a report is a write a member makes, and
    // the queue is where a flood would land.
    if !security::check_message_rate_limit(&state.rate_limiter, reporter).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return;
    }
    let Some(message_id) = v.get("messageId").and_then(Value::as_i64) else {
        send_error(sender, errors::INVALID_MESSAGE_ID).await;
        return;
    };
    let reason = v.get("reason").and_then(Value::as_str).unwrap_or("").trim();
    if reason.chars().count() > MAX_REPORT_REASON_LENGTH {
        send_error(sender, errors::REPORT_REASON_TOO_LONG).await;
        return;
    }
    let record = match db::get_message_record(&state.db, message_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            send_error(sender, errors::MESSAGE_NOT_FOUND).await;
            return;
        }
        Err(e) => {
            error!("Failed to load message {message_id} for a report: {e}");
            send_error(sender, errors::REPORT_FAILED).await;
            return;
        }
    };
    // Not found rather than denied, so the answer reveals nothing about a
    // channel the reporter cannot see.
    if !can_view_channel(state, reporter, ChannelKind::Text, record.channel_id).await {
        send_error(sender, errors::MESSAGE_NOT_FOUND).await;
        return;
    }
    let author = record
        .content
        .get("user")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Err(e) = db::add_report(
        &state.db,
        message_id,
        record.channel_id,
        author,
        reporter,
        reason,
    )
    .await
    {
        error!("Failed to file a report on message {message_id}: {e}");
        send_error(sender, errors::REPORT_FAILED).await;
        return;
    }
    // A repeat report is acknowledged the same way: the reporter's intent is
    // already on file.
    send_json(
        sender,
        &serde_json::json!({ "type": "message-reported", "messageId": message_id }),
    )
    .await;
}

/// What the queue shows of a reported message: its text, cut short, or
/// nothing when it is sealed or gone.
fn excerpt(content: Option<&str>) -> Option<String> {
    let value: Value = serde_json::from_str(content?).ok()?;
    let text = value.get("text").and_then(Value::as_str)?;
    Some(text.chars().take(REPORT_EXCERPT_CHARS).collect())
}

/// Handle `list-reports`: answer with the open reports in every channel the
/// requester moderates. Someone who moderates none gets an empty queue.
pub(super) async fn handle_list_reports(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let reports = match db::list_reports(&state.db, MAX_LISTED_REPORTS).await {
        Ok(reports) => reports,
        Err(e) => {
            error!("Failed to load the report queue: {e}");
            send_error(sender, errors::REPORT_FAILED).await;
            return;
        }
    };
    let mut entries = Vec::new();
    for report in reports {
        if !moderates(state, requester, report.channel_id).await {
            continue;
        }
        entries.push(serde_json::json!({
            "id": report.id,
            "messageId": report.message_id,
            "channelId": report.channel_id,
            "author": report.author,
            "reporter": report.reporter,
            "reason": report.reason,
            "createdAt": report.created_at,
            "deleted": report.content.is_none(),
            "excerpt": excerpt(report.content.as_deref()),
        }));
    }
    send_json(
        sender,
        &serde_json::json!({ "type": "reports", "reports": entries }),
    )
    .await;
}

/// Handle `resolve-report {id}`: close a report in a channel the requester
/// moderates, then answer with the queue as it now stands.
pub(super) async fn handle_resolve_report(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::NOT_AUTHENTICATED).await;
        return;
    };
    let Some(id) = v.get("id").and_then(Value::as_i64) else {
        send_error(sender, errors::REPORT_NOT_FOUND).await;
        return;
    };
    let channel = match db::report_channel(&state.db, id).await {
        Ok(channel) => channel,
        Err(e) => {
            error!("Failed to load report {id}: {e}");
            send_error(sender, errors::REPORT_FAILED).await;
            return;
        }
    };
    let Some(channel) = channel else {
        send_error(sender, errors::REPORT_NOT_FOUND).await;
        return;
    };
    if !moderates(state, requester, channel).await {
        send_error(sender, errors::REPORT_NOT_FOUND).await;
        return;
    }
    if let Err(e) = db::delete_report(&state.db, id).await {
        error!("Failed to resolve report {id}: {e}");
        send_error(sender, errors::REPORT_FAILED).await;
        return;
    }
    handle_list_reports(state, sender, user_name).await;
}

async fn moderates(state: &Arc<AppState>, user: &str, channel_id: i32) -> bool {
    has_channel_permission(
        state,
        user,
        ChannelKind::Text,
        channel_id,
        crate::permissions::MANAGE_MESSAGES,
    )
    .await
}
