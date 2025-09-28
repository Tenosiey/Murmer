//! Endpoints used to administratively modify server state.
//!
//! The `/role` endpoint requires the `ADMIN_TOKEN` environment variable to be
//! set and allows assigning roles to users via their public key.

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

use crate::{
    AppState, db,
    roles::{RoleInfo, default_color},
};

#[derive(Deserialize)]
pub struct RoleBody {
    pub key: String,
    pub role: String,
    pub color: Option<String>,
}

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
    if let Err(e) = db::set_role(&state.db, &body.key, &body.role, color.as_deref()).await {
        error!("Failed to set role for user {}: {}", body.key, e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    let mut affected = Vec::new();
    {
        let user_keys = state.user_keys.lock().await;
        for (user, key) in user_keys.iter() {
            if key == &body.key {
                affected.push(user.clone());
            }
        }
    }
    let info = RoleInfo {
        role: body.role.clone(),
        color,
    };
    {
        let mut roles = state.roles.lock().await;
        for user in affected.iter() {
            roles.insert(user.clone(), info.clone());
        }
    }
    for user in affected {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "role-update",
            "user": user,
            "role": info.role,
            "color": info.color,
        })) {
            let _ = state.tx.send(msg);
        }
    }
    StatusCode::OK
}
