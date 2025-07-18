//! HTTP handler for multipart file uploads.
//!
//! Exposes `POST /upload` to receive files and store them under the configured upload directory.
use axum::{
    Json,
    extract::{Multipart, State},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::error;

use crate::AppState;

pub async fn upload(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload".to_string());
        if let Ok(data) = field.bytes().await {
            let key = format!("{}-{}", chrono::Utc::now().timestamp_millis(), filename);
            let path = state.upload_dir.join(&key);
            match tokio::fs::write(&path, &data).await {
                Ok(_) => {
                    let url = format!("/files/{}", key);
                    return Json(serde_json::json!({"url": url})).into_response();
                }
                Err(e) => {
                    error!("upload error: {e}");
                    return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
    }
    axum::http::StatusCode::BAD_REQUEST.into_response()
}
