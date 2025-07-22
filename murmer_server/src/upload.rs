//! Endpoint for storing uploaded images on disk.
//!
//! Files are sanitized and saved under the `UPLOAD_DIR` directory. The returned
//! JSON contains a relative URL that clients can combine with the server URL to
//! fetch the image later.

use axum::{
    Json,
    extract::{Multipart, State},
    response::{IntoResponse, Response},
};
use sanitize_filename::sanitize;
use std::sync::Arc;
use tracing::error;

use crate::AppState;

pub async fn upload(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let mut filename = field
            .file_name()
            .map(|s| sanitize(s))
            .unwrap_or_else(|| "upload".to_string());
        if filename.is_empty() {
            filename = "upload".to_string();
        }
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
