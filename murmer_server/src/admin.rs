//! Endpoints used to administratively modify server state.
//!
//! The `/role` endpoint requires the `ADMIN_TOKEN` environment variable to be
//! set and assigns a role to a user by their public key. It is the primary way
//! to bootstrap the first Owner before the dashboard is reachable; the role is
//! added to any existing assignments rather than replacing them.

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
