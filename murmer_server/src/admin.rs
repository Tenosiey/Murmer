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
    StatusCode::OK
}
