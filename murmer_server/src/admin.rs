//! Endpoints used to administratively modify server state.
//!
//! The `/role` endpoint requires the `ADMIN_TOKEN` environment variable to be
//! set and assigns a role to a user by their public key. It is the primary way
//! to bootstrap the first Owner before the dashboard is reachable; the role is
//! added to any existing assignments rather than replacing them.
//!
//! It is also the most privileged permission change on the server, so it is
//! recorded in the audit log like the dashboard's own. The bearer token is
//! not an account, so the entry is attributed to
//! [`ACTOR_ADMIN_TOKEN`](crate::db::ACTOR_ADMIN_TOKEN) — a name no account can
//! hold.

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use serde::Deserialize;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::error;

use crate::roles::default_color;
use crate::ws::helpers;
use crate::{AppState, db};

#[derive(Debug, Deserialize)]
pub struct RoleBody {
    pub key: String,
    pub role: String,
    pub color: Option<String>,
}

#[tracing::instrument(skip(state, bearer), fields(key = %body.key, role = %body.role))]
pub async fn set_role(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<RoleBody>,
) -> impl IntoResponse {
    // Use constant-time comparison to prevent timing attacks
    let authorized = if let Some(expected_token) = &state.admin_token {
        expected_token
            .as_bytes()
            .ct_eq(bearer.token().as_bytes())
            .into()
    } else {
        false
    };

    if !authorized {
        return StatusCode::UNAUTHORIZED;
    }

    let color = body.color.clone().or_else(|| default_color(&body.role));
    let def = match db::assign_named_role(&state.db, &body.key, &body.role, color.as_deref()).await
    {
        Ok(def) => def,
        Err(e) => {
            error!("Failed to set role for user {}: {}", body.key, e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    // Reflect a possibly newly created role definition in memory.
    state.role_defs.lock().await.insert(def.id, def.clone());

    // Update the in-memory assignments of any currently-connected users bound
    // to this key.
    let affected: Vec<String> = {
        let user_keys = state.user_keys.lock().await;
        user_keys
            .iter()
            .filter(|(_, key)| *key == &body.key)
            .map(|(user, _)| user.clone())
            .collect()
    };
    {
        let mut assignments = state.user_roles.lock().await;
        for user in &affected {
            let entry = assignments.entry(user.clone()).or_default();
            if !entry.contains(&def.id) {
                entry.push(def.id);
            }
        }
    }

    // The endpoint takes a key, so the audit entry names the account bound to
    // it where there is one — a base64 key means nothing to somebody reading
    // the log. On the bootstrap path there is often no binding yet, and then
    // the key is the only identity there is.
    let target = db::user_for_key(&state.db, &body.key)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| body.key.clone());
    helpers::record_audit(
        &state,
        db::actions::ADMIN_ROLE_GRANT,
        db::ACTOR_ADMIN_TOKEN,
        &target,
        &def.name,
    )
    .await;

    helpers::broadcast_role_definitions(&state).await;
    for user in affected {
        let ids = state
            .user_roles
            .lock()
            .await
            .get(&user)
            .cloned()
            .unwrap_or_default();
        helpers::broadcast_user_roles(&state, &user, &ids).await;
    }
    StatusCode::OK
}
