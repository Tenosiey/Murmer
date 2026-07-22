//! Channel wiki handlers: per-channel Markdown pages with revision history.
//!
//! Reads (`wiki-get`, `wiki-resolve`) are open to every authenticated user.
//! Writes are role-gated like channel management. Every mutation broadcasts a
//! full `wiki-index` metadata snapshot (no bodies) to the channel; joining
//! clients receive the same snapshot, so all clients converge on the
//! persisted state. Page bodies travel only in `wiki-page`/`wiki-conflict`
//! replies correlated by `requestId`.

use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, db, security};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// Build the client-facing JSON for a full wiki page.
fn page_json(page: &db::WikiPage) -> Value {
    serde_json::json!({
        "slug": page.slug,
        "title": page.title,
        "body": page.body,
        "author": page.author,
        "updatedBy": page.updated_by,
        "revision": page.revision,
        "createdAt": page.created_at,
        "updatedAt": page.updated_at,
    })
}

/// Build the `wiki-index` snapshot payload for a channel.
async fn wiki_index_payload(state: &Arc<AppState>, channel_id: i32) -> Option<String> {
    match db::list_wiki_pages(&state.db, channel_id).await {
        Ok(pages) => {
            let pages: Vec<Value> = pages
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "slug": p.slug,
                        "title": p.title,
                        "revision": p.revision,
                        "updatedBy": p.updated_by,
                        "updatedAt": p.updated_at,
                    })
                })
                .collect();
            Some(
                serde_json::json!({
                    "type": "wiki-index",
                    "channelId": channel_id,
                    "pages": pages,
                })
                .to_string(),
            )
        }
        Err(e) => {
            error!("Failed to load wiki index for channel {channel_id}: {e}");
            None
        }
    }
}

/// Send the current wiki index for a channel to a single client.
pub(super) async fn send_wiki_index(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    channel_id: i32,
) {
    if let Some(payload) = wiki_index_payload(state, channel_id).await {
        let _ = sender.send(Message::Text(payload.into())).await;
    }
}

/// Broadcast the current wiki index to every client joined to the channel.
async fn broadcast_wiki_index(state: &Arc<AppState>, channel_id: i32) {
    if let Some(payload) = wiki_index_payload(state, channel_id).await {
        let chan_tx = get_or_create_channel(state, channel_id).await;
        let _ = chan_tx.send(payload);
    }
}

/// Extract the target channel id from a wiki request.
fn channel_id_of(v: &Value) -> Option<i32> {
    v.get("channelId")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
}

/// Resolve the requester name and check the channel-management role gate.
/// Sends a permission error and returns `None` when the check fails.
async fn require_wiki_writer(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) -> Option<String> {
    let requester = match user_name.as_deref() {
        Some(name) => name,
        None => {
            send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
            return None;
        }
    };
    if !security::check_message_rate_limit(&state.rate_limiter, requester).await {
        send_error(sender, errors::MESSAGE_RATE_LIMIT).await;
        return None;
    }
    if !has_permission(state, requester, crate::permissions::MANAGE_WIKI).await {
        error!("User {requester} attempted a wiki write without permission");
        send_error(sender, errors::CHANNEL_PERMISSION_DENIED).await;
        return None;
    }
    Some(requester.to_owned())
}

/// Handle `wiki-get`: reply with the full page, or `null` when it does not
/// exist (a missing page is a normal answer that drives the create-stub UI).
pub(super) async fn handle_wiki_get(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
) {
    let request_id = v.get("requestId").cloned().unwrap_or(Value::Null);
    let Some(channel_id) = channel_id_of(v) else {
        return;
    };
    let Some(slug) = v.get("slug").and_then(|s| s.as_str()) else {
        return;
    };

    match db::get_wiki_page(&state.db, channel_id, slug).await {
        Ok(page) => {
            let payload = serde_json::json!({
                "type": "wiki-page",
                "requestId": request_id,
                "channelId": channel_id,
                "page": page.as_ref().map(page_json),
            });
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
        }
        Err(e) => {
            error!("Failed to load wiki page {slug} in channel {channel_id}: {e}");
            send_error(sender, errors::WIKI_SAVE_FAILED).await;
        }
    }
}

/// Handle `wiki-resolve`: batched existence check for `[[channel/page]]`
/// links so a rendered document costs one round trip.
pub(super) async fn handle_wiki_resolve(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
) {
    let request_id = v.get("requestId").cloned().unwrap_or(Value::Null);
    let Some(links) = v.get("links").and_then(|l| l.as_array()) else {
        return;
    };

    let pairs: Vec<(String, String)> = links
        .iter()
        .take(MAX_WIKI_RESOLVE_LINKS)
        .filter_map(|link| {
            let channel = link.get("channel")?.as_str()?;
            let slug = link.get("slug")?.as_str()?;
            Some((channel.to_owned(), slug.to_owned()))
        })
        .collect();

    match db::resolve_wiki_links(&state.db, pairs.clone()).await {
        Ok(exists) => {
            let results: Vec<Value> = pairs
                .iter()
                .zip(exists)
                .map(|((channel, slug), exists)| {
                    serde_json::json!({
                        "channel": channel,
                        "slug": slug,
                        "exists": exists,
                    })
                })
                .collect();
            let payload = serde_json::json!({
                "type": "wiki-resolved",
                "requestId": request_id,
                "results": results,
            });
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
        }
        Err(e) => {
            error!("Failed to resolve wiki links: {e}");
        }
    }
}

/// Handle `wiki-create`: validate, role-gate, insert and broadcast the index.
pub(super) async fn handle_wiki_create(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(channel_id) = channel_id_of(v) else {
        return;
    };
    let Some(slug) = v.get("slug").and_then(|s| s.as_str()) else {
        return;
    };
    let Some(title) = v.get("title").and_then(|t| t.as_str()) else {
        return;
    };
    let body = v.get("body").and_then(|b| b.as_str()).unwrap_or("");

    if !validate_wiki_slug(slug) {
        send_error(sender, errors::INVALID_WIKI_SLUG).await;
        return;
    }
    let title = title.trim();
    if !validate_wiki_title(title) {
        send_error(sender, errors::INVALID_WIKI_TITLE).await;
        return;
    }
    if body.len() > MAX_WIKI_BODY_BYTES {
        send_error(sender, errors::WIKI_BODY_TOO_LARGE).await;
        return;
    }

    let Some(user) = require_wiki_writer(state, sender, user_name).await else {
        return;
    };

    match db::create_wiki_page(
        &state.db,
        channel_id,
        slug,
        title,
        body,
        &user,
        MAX_WIKI_PAGES_PER_CHANNEL,
    )
    .await
    {
        Ok(db::CreateWikiResult::Created) => {
            info!(user, channel_id, slug, "Wiki page created");
            broadcast_wiki_index(state, channel_id).await;
        }
        Ok(db::CreateWikiResult::SlugTaken) => {
            send_error(sender, errors::WIKI_SLUG_TAKEN).await;
        }
        Ok(db::CreateWikiResult::LimitReached) => {
            send_error(sender, errors::WIKI_PAGE_LIMIT_REACHED).await;
        }
        Err(e) => {
            error!("Failed to create wiki page {slug} in channel {channel_id}: {e}");
            send_error(sender, errors::WIKI_SAVE_FAILED).await;
        }
    }
}

/// Handle `wiki-update`: compare-and-swap save. Replies `wiki-saved` on
/// success or `wiki-conflict` with the current page when the editor's base
/// revision is stale, then broadcasts the index.
pub(super) async fn handle_wiki_update(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let request_id = v.get("requestId").cloned().unwrap_or(Value::Null);
    let Some(channel_id) = channel_id_of(v) else {
        return;
    };
    let Some(slug) = v.get("slug").and_then(|s| s.as_str()) else {
        return;
    };
    let Some(title) = v.get("title").and_then(|t| t.as_str()) else {
        return;
    };
    let Some(body) = v.get("body").and_then(|b| b.as_str()) else {
        return;
    };
    let Some(expected_revision) = v.get("expectedRevision").and_then(|r| r.as_i64()) else {
        return;
    };

    let title = title.trim();
    if !validate_wiki_title(title) {
        send_error(sender, errors::INVALID_WIKI_TITLE).await;
        return;
    }
    if body.len() > MAX_WIKI_BODY_BYTES {
        send_error(sender, errors::WIKI_BODY_TOO_LARGE).await;
        return;
    }

    let Some(user) = require_wiki_writer(state, sender, user_name).await else {
        return;
    };

    match db::update_wiki_page(
        &state.db,
        channel_id,
        slug,
        title,
        body,
        &user,
        expected_revision,
        MAX_WIKI_REVISIONS_KEPT,
    )
    .await
    {
        Ok(db::UpdateWikiResult::Saved(revision)) => {
            let payload = serde_json::json!({
                "type": "wiki-saved",
                "requestId": request_id,
                "channelId": channel_id,
                "slug": slug,
                "revision": revision,
            });
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
            info!(user, channel_id, slug, revision, "Wiki page saved");
            broadcast_wiki_index(state, channel_id).await;
        }
        Ok(db::UpdateWikiResult::Conflict(current)) => {
            let payload = serde_json::json!({
                "type": "wiki-conflict",
                "requestId": request_id,
                "channelId": channel_id,
                "slug": slug,
                "page": page_json(&current),
            });
            let _ = sender.send(Message::Text(payload.to_string().into())).await;
        }
        Ok(db::UpdateWikiResult::NotFound) => {
            send_error(sender, errors::WIKI_PAGE_NOT_FOUND).await;
        }
        Err(e) => {
            error!("Failed to update wiki page {slug} in channel {channel_id}: {e}");
            send_error(sender, errors::WIKI_SAVE_FAILED).await;
        }
    }
}

/// Handle `wiki-delete`: remove the page and broadcast the index.
pub(super) async fn handle_wiki_delete(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(channel_id) = channel_id_of(v) else {
        return;
    };
    let Some(slug) = v.get("slug").and_then(|s| s.as_str()) else {
        return;
    };

    let Some(user) = require_wiki_writer(state, sender, user_name).await else {
        return;
    };

    match db::delete_wiki_page(&state.db, channel_id, slug).await {
        Ok(true) => {
            info!(user, channel_id, slug, "Wiki page deleted");
            broadcast_wiki_index(state, channel_id).await;
        }
        Ok(false) => {
            send_error(sender, errors::WIKI_PAGE_NOT_FOUND).await;
        }
        Err(e) => {
            error!("Failed to delete wiki page {slug} in channel {channel_id}: {e}");
            send_error(sender, errors::WIKI_SAVE_FAILED).await;
        }
    }
}

/// Handle `wiki-rename`: change a page's slug and broadcast the index.
/// Inbound `[[links]]` to the old slug become create-page stubs by design.
pub(super) async fn handle_wiki_rename(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some(channel_id) = channel_id_of(v) else {
        return;
    };
    let Some(slug) = v.get("slug").and_then(|s| s.as_str()) else {
        return;
    };
    let Some(new_slug) = v.get("newSlug").and_then(|s| s.as_str()) else {
        return;
    };

    if !validate_wiki_slug(new_slug) {
        send_error(sender, errors::INVALID_WIKI_SLUG).await;
        return;
    }

    let Some(user) = require_wiki_writer(state, sender, user_name).await else {
        return;
    };

    match db::rename_wiki_page(&state.db, channel_id, slug, new_slug).await {
        Ok(db::RenameWikiResult::Renamed) => {
            info!(user, channel_id, slug, new_slug, "Wiki page renamed");
            broadcast_wiki_index(state, channel_id).await;
        }
        Ok(db::RenameWikiResult::SlugTaken) => {
            send_error(sender, errors::WIKI_SLUG_TAKEN).await;
        }
        Ok(db::RenameWikiResult::NotFound) => {
            send_error(sender, errors::WIKI_PAGE_NOT_FOUND).await;
        }
        Err(e) => {
            error!("Failed to rename wiki page {slug} in channel {channel_id}: {e}");
            send_error(sender, errors::WIKI_SAVE_FAILED).await;
        }
    }
}
