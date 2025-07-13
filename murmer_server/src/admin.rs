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

use crate::{AppState, db};

#[derive(Deserialize)]
pub struct RoleBody {
    pub key: String,
    pub role: String,
}

pub async fn set_role(
    State(state): State<Arc<AppState>>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<RoleBody>,
) -> impl IntoResponse {
    if state.admin_token.as_deref() != Some(bearer.token()) {
        return StatusCode::UNAUTHORIZED;
    }
    if let Err(e) = db::set_role(&state.db, &body.key, &body.role).await {
        eprintln!("set_role error: {e}");
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
    {
        let mut roles = state.roles.lock().await;
        for user in affected.iter() {
            roles.insert(user.clone(), body.role.clone());
        }
    }
    for user in affected {
        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
            "type": "role-update",
            "user": user,
            "role": body.role,
        })) {
            let _ = state.tx.send(msg);
        }
    }
    StatusCode::OK
}
