//! Endpoint for storing uploaded images on disk.
//!
//! Files are sanitized and saved under the `UPLOAD_DIR` directory. The returned
//! JSON contains a relative URL that clients can combine with the server URL to
//! fetch the image later.

use axum::{
    Json,
    extract::{Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sanitize_filename::sanitize;
use std::sync::Arc;
use tracing::{error, warn};

use crate::AppState;

/// Maximum file size in bytes (10MB)
pub const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Allowed MIME types for uploads
static ALLOWED_IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Allowed file extensions (backup validation)
static ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp"];

/// Detect file type by magic bytes
fn detect_file_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Validate file extension
fn has_valid_extension(filename: &str) -> bool {
    if let Some(ext) = filename.rsplit('.').next() {
        ALLOWED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
    } else {
        false
    }
}

#[tracing::instrument(skip(state, multipart))]
pub async fn upload(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        Ok(None) => return StatusCode::BAD_REQUEST.into_response(),
        Err(err) => {
            warn!(?err, "Failed to read multipart field");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let mut filename = field
        .file_name()
        .map(sanitize)
        .unwrap_or_else(|| "upload".to_string());

    if filename.is_empty() {
        filename = "upload".to_string();
    }

    if !has_valid_extension(&filename) {
        warn!("Rejected upload with invalid extension: {}", filename);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    let data = match field.bytes().await {
        Ok(bytes) => bytes,
        Err(err) => {
            error!(?err, "Failed to read multipart bytes");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    if data.len() > MAX_FILE_SIZE {
        warn!("Rejected upload exceeding size limit: {} bytes", data.len());
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }

    if !detect_file_type(&data).is_some_and(|t| ALLOWED_IMAGE_TYPES.contains(&t)) {
        warn!("Rejected upload with invalid file type for: {}", filename);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    let key = format!("{}-{}", chrono::Utc::now().timestamp_millis(), filename);
    let path = state.upload_dir.join(&key);
    let temp_path = path.with_extension("tmp");

    match tokio::fs::write(temp_path.as_path(), &data).await {
        Ok(_) => match tokio::fs::rename(temp_path.as_path(), &path).await {
            Ok(_) => {
                let url = format!("/files/{}", key);
                Json(serde_json::json!({"url": url})).into_response()
            }
            Err(e) => {
                error!("Failed to move uploaded file: {}", e);
                let _ = tokio::fs::remove_file(temp_path).await;
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        },
        Err(e) => {
            error!("Failed to write uploaded file: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
