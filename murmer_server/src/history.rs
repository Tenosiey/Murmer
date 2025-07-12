use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use crate::{AppState, db};

#[derive(Deserialize)]
pub struct HistoryParams {
    pub channel: String,
    pub before: Option<i64>,
}

pub async fn history(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HistoryParams>,
) -> impl IntoResponse {
    let before = params.before.unwrap_or(i64::MAX);
    let msgs = db::fetch_messages(&state.db, &params.channel, before, 25).await;
    let out: Vec<Value> = msgs
        .into_iter()
        .map(|(id, content)| {
            let mut v = serde_json::from_str::<Value>(&content).unwrap_or(Value::Null);
            if let Value::Object(ref mut obj) = v {
                obj.insert("id".into(), id.into());
            }
            v
        })
        .collect();
    Json(out)
}
