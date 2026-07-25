//! Endpoint for storing uploaded files on disk.
//!
//! Files are sanitized and saved under the `UPLOAD_DIR` directory. Images are
//! validated by magic bytes; other attachments are restricted to a safe-list
//! of extensions so active content (HTML, SVG, scripts) can never be served
//! back from `/files` and executed in a browser context. The returned JSON
//! contains a relative URL that clients can combine with the server URL to
//! fetch the file later.
//!
//! The safe-list is grouped into categories (images, documents, archives,
//! audio, video). Which categories are accepted and how large a file may be
//! are server-wide settings managed from the Server Dashboard and persisted by
//! [`crate::db::upload_config`]; the extension safe-list itself is fixed in
//! code, so no setting can ever admit active content.

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

/// Default per-file size limit in bytes (10 MB) for servers that have not
/// configured one.
pub const DEFAULT_MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Smallest per-file limit an operator may configure (64 KB).
pub const MIN_CONFIGURABLE_FILE_SIZE: usize = 64 * 1024;

/// Hard ceiling for the configurable per-file limit (100 MB). The request body
/// limit is derived from this, and uploads are streamed and aborted as soon as
/// they exceed the *configured* limit, so a high ceiling does not let an
/// unauthenticated caller buffer more than the operator allowed.
pub const MAX_CONFIGURABLE_FILE_SIZE: usize = 100 * 1024 * 1024;

/// Allowed MIME types for image uploads
static ALLOWED_IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// One group of the extension safe-list, toggled as a unit by server admins.
pub struct UploadCategory {
    /// Stable id used in settings and on the wire.
    pub id: &'static str,
    /// Human-readable name shown in the client.
    pub label: &'static str,
    /// Lowercase extensions belonging to this category.
    pub extensions: &'static [&'static str],
}

/// The complete upload safe-list. Deliberately excludes anything a browser
/// might interpret as active content when served from `/files` (html, svg,
/// xml, js, css, ...) — admins can narrow this list, never widen it.
pub static UPLOAD_CATEGORIES: &[UploadCategory] = &[
    UploadCategory {
        id: "images",
        label: "Images",
        extensions: &["jpg", "jpeg", "png", "gif", "webp"],
    },
    UploadCategory {
        id: "documents",
        label: "Documents",
        extensions: &[
            "pdf", "txt", "md", "log", "csv", "json", "toml", "yaml", "yml", "rtf", "doc", "docx",
            "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp",
        ],
    },
    UploadCategory {
        id: "archives",
        label: "Archives",
        extensions: &["zip", "gz", "tar", "bz2", "xz", "7z", "rar"],
    },
    UploadCategory {
        id: "audio",
        label: "Audio",
        extensions: &["mp3", "wav", "ogg", "flac", "m4a", "opus"],
    },
    UploadCategory {
        id: "video",
        label: "Video",
        extensions: &["mp4", "webm", "mkv", "mov", "avi"],
    },
];

/// Id of the image category, whose uploads additionally pass magic-byte checks.
const IMAGE_CATEGORY: &str = "images";

/// Whether `id` names a category known to this build.
pub fn is_known_category(id: &str) -> bool {
    UPLOAD_CATEGORIES.iter().any(|c| c.id == id)
}

/// Every category id, i.e. the default (fully permissive) configuration.
pub fn default_category_ids() -> Vec<String> {
    UPLOAD_CATEGORIES.iter().map(|c| c.id.to_string()).collect()
}

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

/// Detect an audio container by magic bytes.
///
/// Used for soundboard sounds, which the client auto-plays to everyone in a
/// voice channel — a stricter bar than a generic attachment nobody opens. The
/// generic `/upload` path deliberately does *not* call this: it would reject
/// legitimate but exotic audio attachments, and those are only ever downloaded
/// deliberately by a user.
pub fn detect_audio_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"ID3") {
        Some("audio/mpeg")
    } else if data.len() >= 2 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0 {
        // MPEG frame sync: 11 set bits, i.e. a headerless MP3.
        Some("audio/mpeg")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WAVE" {
        Some("audio/wav")
    } else if data.starts_with(b"OggS") {
        // Covers both Vorbis and Opus in an Ogg container.
        Some("audio/ogg")
    } else if data.len() >= 8 && &data[4..8] == b"ftyp" {
        Some("audio/mp4")
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

/// Resolve a filename to its safe-list category, or `None` when the extension
/// is not on the list at all.
pub fn classify_extension(filename: &str) -> Option<&'static UploadCategory> {
    let ext = file_extension(filename)?;
    UPLOAD_CATEGORIES
        .iter()
        .find(|category| category.extensions.contains(&ext.as_str()))
}

#[tracing::instrument(skip(state, multipart))]
pub async fn upload(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    // The policy is read per request so dashboard changes apply immediately.
    let config = match crate::db::upload_config(&state.db).await {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load upload configuration: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut field = match multipart.next_field().await {
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

    let Some(category) = classify_extension(&filename) else {
        warn!("Rejected upload with invalid extension: {}", filename);
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    if !config.categories.iter().any(|id| id == category.id) {
        warn!(
            "Rejected upload of disabled category '{}': {}",
            category.id, filename
        );
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    // Stream the body so an oversize upload is abandoned as soon as it passes
    // the configured limit instead of being buffered in full first.
    let max_bytes = config.max_bytes as usize;
    let mut data: Vec<u8> = Vec::new();
    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                if data.len() + chunk.len() > max_bytes {
                    warn!("Rejected upload exceeding the {max_bytes} byte limit");
                    return StatusCode::PAYLOAD_TOO_LARGE.into_response();
                }
                data.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(err) => {
                error!(?err, "Failed to read multipart bytes");
                return StatusCode::BAD_REQUEST.into_response();
            }
        }
    }

    if data.is_empty() {
        warn!("Rejected empty upload: {}", filename);
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Image extensions must also pass magic-byte validation so a mislabelled
    // file cannot masquerade as an image.
    if category.id == IMAGE_CATEGORY
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
                    "kind": if category.id == IMAGE_CATEGORY { "image" } else { "file" },
                    "category": category.id,
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
