//! Endpoint for storing uploaded files on disk.
//!
//! Files are sanitized and saved under the `UPLOAD_DIR` directory. Images are
//! validated by magic bytes; other attachments are restricted to a safe-list
//! of extensions so active content (HTML, SVG, scripts) can never be served
//! back from `/files` and executed in a browser context. The returned JSON
//! contains a relative URL that clients can combine with the server URL to
//! fetch the file later.

use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use sanitize_filename::sanitize;
use std::sync::Arc;
use tracing::{error, warn};

use crate::AppState;

/// Maximum file size in bytes (10MB)
pub const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Allowed MIME types for image uploads
static ALLOWED_IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Allowed image file extensions (backup validation)
static ALLOWED_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp"];

/// Allowed extensions for non-image attachments. Deliberately excludes
/// anything a browser might interpret as active content when served from
/// `/files` (html, svg, xml, js, css, ...).
static ALLOWED_ATTACHMENT_EXTENSIONS: &[&str] = &[
    // documents and plain text
    "pdf", "txt", "md", "log", "csv", "json", "toml", "yaml", "yml", "rtf", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "odt", "ods", "odp", // archives
    "zip", "gz", "tar", "bz2", "xz", "7z", "rar", // audio
    "mp3", "wav", "ogg", "flac", "m4a", "opus", // video
    "mp4", "webm", "mkv", "mov", "avi",
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

/// Extract the lowercase extension from a filename, if any.
fn file_extension(filename: &str) -> Option<String> {
    let (stem, ext) = filename.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    Some(ext.to_lowercase())
}

/// Classification of an upload derived from its file extension.
enum UploadKind {
    Image,
    Attachment,
}

fn classify_extension(filename: &str) -> Option<UploadKind> {
    let ext = file_extension(filename)?;
    if ALLOWED_IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(UploadKind::Image)
    } else if ALLOWED_ATTACHMENT_EXTENSIONS.contains(&ext.as_str()) {
        Some(UploadKind::Attachment)
    } else {
        None
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

    let Some(kind) = classify_extension(&filename) else {
        warn!("Rejected upload with invalid extension: {}", filename);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

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

    if data.is_empty() {
        warn!("Rejected empty upload: {}", filename);
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Image extensions must also pass magic-byte validation so a mislabelled
    // file cannot masquerade as an image.
    if matches!(kind, UploadKind::Image)
        && !detect_file_type(&data).is_some_and(|t| ALLOWED_IMAGE_TYPES.contains(&t))
    {
        warn!("Rejected upload with invalid file type for: {}", filename);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    let key = format!("{}-{}", chrono::Utc::now().timestamp_millis(), filename);
    let path = state.upload_dir.join(&key);
    // Append ".tmp" rather than replacing the extension: with_extension()
    // would map same-millisecond uploads of "a.pdf" and "a.zip" onto the same
    // temp file and let the concurrent writes corrupt each other.
    let temp_path = state.upload_dir.join(format!("{key}.tmp"));

    match tokio::fs::write(temp_path.as_path(), &data).await {
        Ok(_) => match tokio::fs::rename(temp_path.as_path(), &path).await {
            Ok(_) => {
                let url = format!("/files/{}", key);
                Json(serde_json::json!({
                    "url": url,
                    "name": filename,
                    "size": data.len(),
                    "kind": match kind {
                        UploadKind::Image => "image",
                        UploadKind::Attachment => "file",
                    },
                }))
                .into_response()
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
