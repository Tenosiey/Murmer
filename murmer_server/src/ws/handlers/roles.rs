//! Handlers for custom role management: create/update/delete/reorder role
//! definitions and assign roles to users.
//!
//! Every action requires the `MANAGE_ROLES` permission and is bounded by the
//! role hierarchy to prevent privilege escalation:
//! - a manager may only touch roles positioned strictly below their own
//!   highest role,
//! - may never grant a permission they do not themselves hold, and
//! - may not reassign a user who is not strictly outranked.
//!
//! Administrators (the Owner role) sit at [`i64::MAX`] and bypass these bounds.
//! All checks run against the in-memory role state, so a client cannot spoof
//! authority by editing its local copy.

use crate::permissions::{self, Permissions};
use crate::roles::RoleDef;
use crate::ws::{constants::*, errors, helpers::*, validation::*};
use crate::{AppState, db};
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// Resolve the requester and verify they may manage roles, returning their
/// name, effective permission mask and hierarchy position. Sends
/// `role-permission-denied` and returns `None` otherwise.
async fn authorize_manager(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    user_name: &Option<String>,
) -> Option<(String, Permissions, i64)> {
    let Some(requester) = user_name.clone() else {
        send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
        return None;
    };
    let mask = effective_permissions(state, &requester).await;
    if !permissions::mask_allows(mask, permissions::MANAGE_ROLES) {
        send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
        return None;
    }
    let position = top_position(state, &requester).await;
    Some((requester, mask, position))
}

/// Whether `mask` includes the administrator bit.
fn is_admin(mask: Permissions) -> bool {
    mask & permissions::ADMINISTRATOR != 0
}

/// Parse and validate the `permissions` field. Sends an error and returns
/// `None` if missing or if it contains unknown bits.
async fn parse_permissions(
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
) -> Option<Permissions> {
    let Some(raw) = v.get("permissions").and_then(|p| p.as_u64()) else {
        send_error(sender, errors::INVALID_ROLE_PERMISSIONS).await;
        return None;
    };
    if !permissions::is_valid_mask(raw) {
        send_error(sender, errors::INVALID_ROLE_PERMISSIONS).await;
        return None;
    }
    Some(raw)
}

/// Read and validate an optional `color` field. Returns `Ok(None)` when the
/// field is absent/empty/null (clearing the color) and `Err(())` after sending
/// an error for a malformed value.
async fn parse_color(
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
) -> Result<Option<String>, ()> {
    match v.get("color") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => {
            if validate_role_color(s) {
                Ok(Some(s.clone()))
            } else {
                send_error(sender, errors::INVALID_ROLE_COLOR).await;
                Err(())
            }
        }
        Some(_) => {
            send_error(sender, errors::INVALID_ROLE_COLOR).await;
            Err(())
        }
    }
}

/// Snapshot the current role definitions from the in-memory map.
async fn snapshot_defs(state: &Arc<AppState>) -> Vec<RoleDef> {
    state.role_defs.lock().await.values().cloned().collect()
}

/// Handle create-role: define a new custom role.
pub(super) async fn handle_create_role(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some((requester, mask, requester_pos)) = authorize_manager(state, sender, user_name).await
    else {
        return;
    };

    let Some(name) = v.get("name").and_then(|n| n.as_str()).map(str::trim) else {
        send_error(sender, errors::INVALID_ROLE_NAME).await;
        return;
    };
    if !validate_role_name(name) {
        send_error(sender, errors::INVALID_ROLE_NAME).await;
        return;
    }

    let Ok(color) = parse_color(sender, v).await else {
        return;
    };

    let Some(perms) = parse_permissions(sender, v).await else {
        return;
    };
    // A manager can never mint a role more powerful than themselves.
    if !is_admin(mask) && (perms & !mask) != 0 {
        send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
        return;
    }

    let defs = snapshot_defs(state).await;
    if defs.len() >= MAX_ROLES {
        send_error(sender, errors::ROLE_LIMIT_REACHED).await;
        return;
    }
    if defs.iter().any(|d| d.name.eq_ignore_ascii_case(name)) {
        send_error(sender, errors::ROLE_NAME_TAKEN).await;
        return;
    }

    // Position the role just below the Owner, and never above the requester's
    // own highest role.
    let max_non_owner = defs
        .iter()
        .filter(|d| !d.is_owner)
        .map(|d| d.position)
        .max()
        .unwrap_or(0);
    let mut position = max_non_owner + 1;
    if requester_pos != i64::MAX {
        position = position.min((requester_pos - 1).max(1));
    }

    let id = match db::create_role_def(&state.db, name, color.as_deref(), perms, position).await {
        Ok(id) => id,
        Err(e) => {
            error!("failed to create role {name}: {e}");
            send_error(sender, errors::ROLE_UPDATE_FAILED).await;
            return;
        }
    };
    let def = RoleDef {
        id,
        name: name.to_string(),
        color,
        permissions: perms,
        position,
        is_default: false,
        is_owner: false,
    };
    state.role_defs.lock().await.insert(id, def);
    broadcast_role_definitions(state).await;
    info!(requester, role = name, "Role created");
}

/// Handle update-role: edit an existing role's name, color and permissions.
pub(super) async fn handle_update_role(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some((requester, mask, requester_pos)) = authorize_manager(state, sender, user_name).await
    else {
        return;
    };

    let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
        send_error(sender, errors::ROLE_NOT_FOUND).await;
        return;
    };
    let Some(target) = state.role_defs.lock().await.get(&id).cloned() else {
        send_error(sender, errors::ROLE_NOT_FOUND).await;
        return;
    };

    // Must strictly outrank the role being edited.
    if requester_pos <= target.position {
        send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
        return;
    }

    // The `@everyone` name is reserved and fixed; other roles may be renamed.
    let name = if target.is_default {
        crate::roles::EVERYONE_ROLE_NAME.to_string()
    } else {
        match v.get("name").and_then(|n| n.as_str()).map(str::trim) {
            Some(n) => {
                if !validate_role_name(n) {
                    send_error(sender, errors::INVALID_ROLE_NAME).await;
                    return;
                }
                n.to_string()
            }
            None => target.name.clone(),
        }
    };
    if !name.eq_ignore_ascii_case(&target.name) {
        let taken = state
            .role_defs
            .lock()
            .await
            .values()
            .any(|d| d.id != id && d.name.eq_ignore_ascii_case(&name));
        if taken {
            send_error(sender, errors::ROLE_NAME_TAKEN).await;
            return;
        }
    }

    let color = match v.get("color") {
        None => target.color.clone(),
        Some(_) => match parse_color(sender, v).await {
            Ok(c) => c,
            Err(()) => return,
        },
    };

    // The Owner role is locked to ADMINISTRATOR; otherwise keep the existing
    // permissions when the field is omitted.
    let perms = if target.is_owner {
        permissions::ADMINISTRATOR
    } else {
        match v.get("permissions") {
            None => target.permissions,
            Some(_) => match parse_permissions(sender, v).await {
                Some(p) => p,
                None => return,
            },
        }
    };
    if !is_admin(mask) && (perms & !mask) != 0 {
        send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
        return;
    }

    if let Err(e) = db::update_role_def(&state.db, id, &name, color.as_deref(), perms).await {
        error!("failed to update role {id}: {e}");
        send_error(sender, errors::ROLE_UPDATE_FAILED).await;
        return;
    }
    {
        let mut map = state.role_defs.lock().await;
        if let Some(def) = map.get_mut(&id) {
            def.name = name.clone();
            def.color = color;
            def.permissions = perms;
        }
    }
    broadcast_role_definitions(state).await;
    info!(requester, role = %name, "Role updated");
}

/// Handle delete-role: remove a custom role and detach it from all users.
pub(super) async fn handle_delete_role(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some((requester, _mask, requester_pos)) = authorize_manager(state, sender, user_name).await
    else {
        return;
    };

    let Some(id) = v.get("id").and_then(|i| i.as_i64()) else {
        send_error(sender, errors::ROLE_NOT_FOUND).await;
        return;
    };
    let Some(target) = state.role_defs.lock().await.get(&id).cloned() else {
        send_error(sender, errors::ROLE_NOT_FOUND).await;
        return;
    };
    if target.is_default || target.is_owner {
        send_error(sender, errors::ROLE_PROTECTED).await;
        return;
    }
    if requester_pos <= target.position {
        send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
        return;
    }

    match db::delete_role_def(&state.db, id).await {
        Ok(true) => {}
        Ok(false) => {
            send_error(sender, errors::ROLE_NOT_FOUND).await;
            return;
        }
        Err(e) => {
            error!("failed to delete role {id}: {e}");
            send_error(sender, errors::ROLE_UPDATE_FAILED).await;
            return;
        }
    }

    state.role_defs.lock().await.remove(&id);

    // Detach the role from every in-memory assignment and note who changed so
    // their clients can update.
    let affected: Vec<(String, Vec<i64>)> = {
        let mut assignments = state.user_roles.lock().await;
        let mut changed = Vec::new();
        for (user, ids) in assignments.iter_mut() {
            if let Some(pos) = ids.iter().position(|&x| x == id) {
                ids.remove(pos);
                changed.push((user.clone(), ids.clone()));
            }
        }
        changed
    };

    broadcast_role_definitions(state).await;
    for (user, ids) in affected {
        broadcast_user_roles(state, &user, &ids).await;
    }
    info!(requester, role = %target.name, "Role deleted");
}

/// Handle reorder-roles: `orderedIds` lists the manageable custom roles from
/// highest power to lowest. `@everyone` stays at the bottom and the Owner is
/// kept at the top.
pub(super) async fn handle_reorder_roles(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some((requester, _mask, requester_pos)) = authorize_manager(state, sender, user_name).await
    else {
        return;
    };

    let Some(arr) = v.get("orderedIds").and_then(|a| a.as_array()) else {
        send_error(sender, errors::ROLE_UPDATE_FAILED).await;
        return;
    };
    let ids: Vec<i64> = arr.iter().filter_map(|x| x.as_i64()).collect();
    if ids.len() != arr.len() || ids.len() > MAX_ROLES {
        send_error(sender, errors::ROLE_UPDATE_FAILED).await;
        return;
    }

    let defs = state.role_defs.lock().await.clone();
    for id in &ids {
        let Some(def) = defs.get(id) else {
            send_error(sender, errors::ROLE_NOT_FOUND).await;
            return;
        };
        if def.is_default || def.is_owner {
            send_error(sender, errors::ROLE_PROTECTED).await;
            return;
        }
        if requester_pos <= def.position {
            send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
            return;
        }
    }

    // First listed id gets the highest position; the Owner is bumped above all.
    let n = ids.len() as i64;
    let owner_id = defs.values().find(|d| d.is_owner).map(|d| d.id);
    for (index, id) in ids.iter().enumerate() {
        let position = n - index as i64;
        if let Err(e) = db::set_role_position(&state.db, *id, position).await {
            error!("failed to reorder role {id}: {e}");
            send_error(sender, errors::ROLE_UPDATE_FAILED).await;
            return;
        }
    }
    if let Some(owner_id) = owner_id
        && let Err(e) = db::set_role_position(&state.db, owner_id, n + 1).await
    {
        error!("failed to reposition owner role: {e}");
    }

    {
        let mut map = state.role_defs.lock().await;
        for (index, id) in ids.iter().enumerate() {
            if let Some(def) = map.get_mut(id) {
                def.position = n - index as i64;
            }
        }
        if let Some(owner_id) = owner_id
            && let Some(owner) = map.get_mut(&owner_id)
        {
            owner.position = n + 1;
        }
    }
    broadcast_role_definitions(state).await;
    info!(requester, "Roles reordered");
}

/// Handle set-user-roles: replace the set of roles assigned to a user.
pub(super) async fn handle_set_user_roles(
    state: &Arc<AppState>,
    sender: &mut SplitSink<WebSocket, Message>,
    v: &Value,
    user_name: &Option<String>,
) {
    let Some((requester, mask, requester_pos)) = authorize_manager(state, sender, user_name).await
    else {
        return;
    };

    let Some(target_user) = v.get("user").and_then(|u| u.as_str()) else {
        send_error(sender, errors::ROLE_TARGET_NOT_FOUND).await;
        return;
    };
    let Some(arr) = v.get("roleIds").and_then(|a| a.as_array()) else {
        send_error(sender, errors::ROLE_UPDATE_FAILED).await;
        return;
    };
    let mut ids: Vec<i64> = arr.iter().filter_map(|x| x.as_i64()).collect();
    if ids.len() != arr.len() {
        send_error(sender, errors::ROLE_UPDATE_FAILED).await;
        return;
    }
    ids.sort_unstable();
    ids.dedup();

    let Some(key) = lookup_user_key(state, target_user).await else {
        send_error(sender, errors::ROLE_TARGET_NOT_FOUND).await;
        return;
    };

    // A manager may only reassign a user they strictly outrank.
    let target_pos = top_position(state, target_user).await;
    if requester_pos <= target_pos {
        send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
        return;
    }

    let defs = state.role_defs.lock().await.clone();
    for id in &ids {
        let Some(def) = defs.get(id) else {
            send_error(sender, errors::ROLE_NOT_FOUND).await;
            return;
        };
        // The default `@everyone` role is implicit and never assigned directly.
        if def.is_default {
            send_error(sender, errors::ROLE_PROTECTED).await;
            return;
        }
        // Only grant roles below the requester and no more powerful than them.
        if requester_pos <= def.position || (!is_admin(mask) && (def.permissions & !mask) != 0) {
            send_error(sender, errors::ROLE_PERMISSION_DENIED).await;
            return;
        }
    }

    if let Err(e) = db::set_user_roles(&state.db, &key, &ids).await {
        error!("failed to set roles for {target_user}: {e}");
        send_error(sender, errors::ROLE_UPDATE_FAILED).await;
        return;
    }
    state
        .user_roles
        .lock()
        .await
        .insert(target_user.to_string(), ids.clone());
    broadcast_user_roles(state, target_user, &ids).await;
    info!(requester, target_user, "User roles updated");
}
