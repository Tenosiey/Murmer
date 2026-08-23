//! Handlers for the auto-moderation rules: reading them, replacing them, and
//! applying them to a message.
//!
//! The rules themselves are manager-only information and are never broadcast:
//! a pattern is the description of what a server is trying to keep out, and
//! handing it to everyone is handing it to whoever wants to walk around it.
//! They travel only in the direct answer to `get-automod-rules`, the way the
//! ban list and the profanity word list do.
//!
//! [`screen`] is the enforcement point, called from `messages.rs` for a new
//! message and for an edit alike — otherwise posting something and editing it
//! a second later would walk straight past every rule. It runs on the text as
//! the sender typed it, *before* the profanity mask, so a word the mask would
//! have starred out cannot slip a rule that matches it.
//!
//! The rule model and the matching are [`crate::automod`]; this file decides
//! who may configure them and what happens when one fires.

use crate::automod::{
    AutomodRule, DEFAULT_AUTOMOD_MUTE_SECONDS, RuleAction, RuleError, RuleKind, RuleSet,
    normalize_rules,
};
use crate::ws::{errors, helpers::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use chrono::{Duration as ChronoDuration, Utc};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Recorded as a mute's `muted_by`. A mute row answers "who did this", and
/// for an automatic mute the honest answer is not a member's name.
const AUTOMOD_ACTOR: &str = "AutoMod";

/// What auto-moderation decided about one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Screen {
    /// Nothing matched, or the match only warranted a warning — either way
    /// the message goes out.
    Allowed,
    /// A rule refused it. The sender has already been told why.
    Blocked,
}

/// Serialize a rule list as an `automod-rules` frame.
fn rules_frame(rules: &[AutomodRule]) -> Option<String> {
    let entries: Vec<Value> = rules
        .iter()
        .map(|rule| {
            serde_json::json!({
                "name": rule.name,
                "pattern": rule.pattern,
                "kind": rule.kind.as_str(),
                "action": rule.action.as_str(),
                "muteSeconds": rule.mute_seconds,
                "enabled": rule.enabled,
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "type": "automod-rules",
        "rules": entries,
    }))
    .ok()
}

/// Read one rule from a client frame. Returns `None` for anything structurally
/// wrong, including a `kind` or `action` this build does not know — guessing a
/// default there would apply an action the operator never chose.
fn parse_rule(value: &Value) -> Option<AutomodRule> {
    let object = value.as_object()?;
    Some(AutomodRule {
        name: object
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or_default()
            .to_string(),
        pattern: object.get("pattern")?.as_str()?.to_string(),
        kind: RuleKind::parse(object.get("kind")?.as_str()?)?,
        action: RuleAction::parse(object.get("action")?.as_str()?)?,
        mute_seconds: object
            .get("muteSeconds")
            .and_then(|seconds| seconds.as_i64())
            .unwrap_or(DEFAULT_AUTOMOD_MUTE_SECONDS),
        enabled: object
            .get("enabled")
            .and_then(|enabled| enabled.as_bool())
            .unwrap_or(true),
    })
}

/// Handle `get-automod-rules`: answer a manager with the configured rules.
/// Read from the database rather than from the compiled mirror, so what the
/// editor loads is what is actually stored.
pub(super) async fn handle_get_automod_rules(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        return;
    };
    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        info!(requester, "Denied auto-moderation rule request");
        return;
    }

    match db::automod_rules(&state.db).await {
        Ok(rules) => {
            if let Some(msg) = rules_frame(&rules) {
                let _ = sender.send(Message::Text(msg.into())).await;
            }
        }
        Err(e) => {
            error!("Failed to load the auto-moderation rules: {e}");
            send_error(sender, errors::AUTOMOD_UPDATE_FAILED).await;
        }
    }
}

/// Handle `set-automod-rules`: replace the rule list, recompile the mirror and
/// answer with what was stored.
///
/// A rule that fails validation refuses the *whole* frame rather than being
/// dropped from it. A dashboard that silently saves fewer rules than it was
/// given, or a regex that quietly never fires, is the failure an operator
/// finds out about weeks later from the thing it was supposed to stop.
pub(super) async fn handle_set_automod_rules(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(requester) = user_name.as_deref() else {
        send_error(sender, errors::AUTOMOD_PERMISSION_DENIED).await;
        return;
    };

    // Server-side permission check; clients cannot spoof this.
    if !has_permission(state, requester, crate::permissions::MANAGE_SERVER).await {
        warn!("User {requester} attempted to change the auto-moderation rules without permission");
        send_error(sender, errors::AUTOMOD_PERMISSION_DENIED).await;
        return;
    }

    let Some(raw_rules) = v.get("rules").and_then(|rules| rules.as_array()) else {
        send_error(sender, errors::INVALID_AUTOMOD_RULES).await;
        return;
    };
    let mut requested = Vec::with_capacity(raw_rules.len());
    for entry in raw_rules {
        match parse_rule(entry) {
            Some(rule) => requested.push(rule),
            None => {
                send_error(sender, errors::INVALID_AUTOMOD_RULES).await;
                return;
            }
        }
    }

    let requested = match normalize_rules(&requested) {
        Ok(rules) => rules,
        Err(problem) => {
            info!(requester, ?problem, "Rejected an auto-moderation rule");
            // A bad pattern is told apart from a bad frame: it is the failure
            // an operator can fix by looking at what they typed, and a generic
            // "not valid" for a mistyped regex is how a rule ends up silently
            // never saved.
            send_error(
                sender,
                match problem {
                    RuleError::InvalidRegex | RuleError::PatternNotAWord => {
                        errors::INVALID_AUTOMOD_PATTERN
                    }
                    _ => errors::INVALID_AUTOMOD_RULES,
                },
            )
            .await;
            return;
        }
    };

    let stored = match db::set_automod_rules(&state.db, &requested).await {
        Ok(stored) => stored,
        Err(e) => {
            error!("Failed to store the auto-moderation rules: {e}");
            send_error(sender, errors::AUTOMOD_UPDATE_FAILED).await;
            return;
        }
    };

    info!(
        requester,
        rules = stored.len(),
        "Auto-moderation rules updated"
    );
    *state.automod.lock().await = RuleSet::compile(stored.clone());

    if let Some(msg) = rules_frame(&stored) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }
}

/// Apply the rules to `text` and act on the most severe match.
///
/// Returns [`Screen::Blocked`] when the caller must drop the message; the
/// sender has been told by then, so the caller only has to stop.
pub(super) async fn screen(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user: &str,
    text: &str,
) -> Screen {
    // Matching comes before the exemption below, so a server with no rules —
    // the common case — never reaches the permission lookup at all.
    let matched = {
        let rules = state.automod.lock().await;
        if !rules.is_active() {
            return Screen::Allowed;
        }
        rules.evaluate(text).cloned()
    };
    let Some(rule) = matched else {
        return Screen::Allowed;
    };

    // Members who can manage messages are exempt, exactly as they are from
    // slow mode. These are the people who moderate the room, and a rule that
    // mutes them for quoting the thing it filters leaves nobody able to lift
    // it — the pattern that goes wrong is never the one that was tested.
    if has_permission(state, user, crate::permissions::MANAGE_MESSAGES).await {
        return Screen::Allowed;
    }

    match rule.action {
        RuleAction::Warn => {
            // Carries the rule's *name*, never its pattern: telling somebody
            // which rule they tripped is the point of a warning; telling them
            // exactly what it looks for is telling them how to phrase it next
            // time.
            let frame = serde_json::json!({
                "type": "automod-warning",
                "rule": rule.name,
            });
            let _ = sender.send(Message::Text(frame.to_string().into())).await;
            info!(user, rule = %rule.name, "Auto-moderation warned a member");
            Screen::Allowed
        }
        RuleAction::Delete => {
            send_error(sender, errors::AUTOMOD_BLOCKED).await;
            info!(user, rule = %rule.name, "Auto-moderation blocked a message");
            Screen::Blocked
        }
        RuleAction::Mute => {
            send_error(sender, errors::AUTOMOD_BLOCKED).await;
            apply_mute(state, user, &rule).await;
            Screen::Blocked
        }
    }
}

/// Mute `user` for the duration `rule` carries.
async fn apply_mute(state: &Arc<AppState>, user: &str, rule: &AutomodRule) {
    let Some(key) = lookup_user_key(state, user).await else {
        // A mute is bound to the key, so without one there is nothing to
        // record. The message was refused either way.
        warn!(user, "Auto-moderation found no key to mute");
        return;
    };
    let until = Utc::now() + ChronoDuration::seconds(rule.mute_seconds);

    if let Err(e) = db::add_mute(&state.db, &key, user, AUTOMOD_ACTOR, Some(until)).await {
        error!("Failed to persist an auto-moderation mute for {user}: {e}");
        return;
    }
    state.mutes.lock().await.insert(key, Some(until));

    // Announced like a moderator's mute, so the room and the member's other
    // clients hear about it. Auto-moderation acting in silence is how an
    // operator learns about a bad rule from the people it hit rather than
    // from the app.
    let msg = serde_json::json!({
        "type": "user-muted",
        "user": user,
        "by": AUTOMOD_ACTOR,
        "until": until.to_rfc3339(),
    });
    let _ = state.tx.send(msg.to_string().into());
    info!(
        user,
        rule = %rule.name,
        seconds = rule.mute_seconds,
        "Auto-moderation muted a member"
    );
}
