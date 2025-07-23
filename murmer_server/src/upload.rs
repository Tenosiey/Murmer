//! Endpoint for storing uploaded images on disk.
//!
//! Files are sanitized and saved under the `UPLOAD_DIR` directory. The returned
//! JSON contains a relative URL that clients can combine with the server URL to
//! fetch the image later.

use axum::{
    Json, http::StatusCode,
    extract::{Multipart, State},
    response::{IntoResponse, Response},
};
use sanitize_filename::sanitize;
use std::sync::Arc;
use tracing::{error, warn};

use crate::AppState;

/// Maximum file size in bytes (10MB)
const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Allowed MIME types for uploads
static ALLOWED_IMAGE_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png", 
    "image/gif",
    "image/webp"
];

/// Allowed file extensions (backup validation)
static ALLOWED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp"
];

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
    if let Some(ext) = filename.split('.').last() {
        ALLOWED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
    } else {
        false
    }
}

pub async fn upload(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let mut filename = field
            .file_name()
            .map(|s| sanitize(s))
            .unwrap_or_else(|| "upload".to_string());
        
        if filename.is_empty() {
            filename = "upload".to_string();
        }
        
        // Validate file extension
        if !has_valid_extension(&filename) {
            warn!("Rejected upload with invalid extension: {}", filename);
            return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
        }
        
        if let Ok(data) = field.bytes().await {
            // Check file size
            if data.len() > MAX_FILE_SIZE {
                warn!("Rejected upload exceeding size limit: {} bytes", data.len());
                return StatusCode::PAYLOAD_TOO_LARGE.into_response();
            }
            
            // Validate file type by magic bytes
            let detected_type = detect_file_type(&data);
            if !detected_type.map_or(false, |t| ALLOWED_IMAGE_TYPES.contains(&t)) {
                warn!("Rejected upload with invalid file type for: {}", filename);
                return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
            }
            
            // Generate secure filename with timestamp
            let key = format!("{}-{}", chrono::Utc::now().timestamp_millis(), filename);
            let path = state.upload_dir.join(&key);
            
            // Use atomic write operation
            let temp_path = format!("{}.tmp", path.display());
            let temp_path = std::path::Path::new(&temp_path);
            
            match tokio::fs::write(temp_path, &data).await {
                Ok(_) => {
                    match tokio::fs::rename(temp_path, &path).await {
                        Ok(_) => {
                            let url = format!("/files/{}", key);
                            return Json(serde_json::json!({"url": url})).into_response();
                        }
                        Err(e) => {
                            error!("Failed to move uploaded file: {}", e);
                            let _ = tokio::fs::remove_file(temp_path).await;
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to write uploaded file: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
    }
    StatusCode::BAD_REQUEST.into_response()
}
