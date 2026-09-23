//! Serving the built web client (`WEB_CLIENT_DIR`) under a
//! Content-Security-Policy.
//!
//! The desktop shell ships a policy in `tauri.conf.json`; the same bundle
//! served over HTTP used to get none, so markup slipping through the
//! markdown `{@html}` boundary would have been inert in the app and live in
//! the browser. [`POLICY`] is the Tauri policy, held equal to it by
//! `murmer_client/test/server-mirror.test.ts`.
//!
//! SvelteKit's prerendered pages boot from one inline `<script>`, which
//! `script-src 'self'` blocks. Tauri hashes inline scripts at build time;
//! here each HTML response is hashed as it is served. Hashing at startup
//! would be cheaper, but a client rebuilt under a running server would then
//! be served a policy that blocks its own bootstrap — a blank page with only
//! a console error to explain it. The pages are a couple of kilobytes and
//! loaded once per session.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderValue, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose};
use sha2::{Digest, Sha256};
use std::path::Path;
use tower_http::services::{ServeDir, ServeFile};
use tracing::warn;

/// The `tauri.conf.json` policy. `script-src` is last so the inline script
/// hashes can be appended to it.
const POLICY: &str = "default-src 'self'; img-src 'self' http: https: data: blob:; \
     media-src 'self' blob:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; \
     connect-src 'self' http: https: ws: wss:; object-src 'none'; base-uri 'self'; \
     frame-ancestors 'none'; form-action 'self'; script-src 'self' 'wasm-unsafe-eval'";

/// Far above any prerendered page. The page has to be buffered to be hashed,
/// and this keeps a stray large `.html` in the directory from being read into
/// memory whole.
const MAX_HTML_BYTES: usize = 1024 * 1024;

/// The web client as a fallback service. The client is a prerendered
/// single-page app: a deep link such as `/invite` has no file of its own, so
/// anything unmatched falls back to the SPA shell, which routes it in the
/// browser.
pub fn router(dir: &Path) -> Router {
    let spa = ServeDir::new(dir).fallback(ServeFile::new(dir.join("200.html")));
    Router::new()
        .fallback_service(spa)
        .layer(middleware::map_response(with_policy))
}

async fn with_policy(response: Response) -> Response {
    // Only a full HTML body can be hashed. A 304 in particular must pass
    // untouched: its headers replace the cached ones, and a policy computed
    // from its empty body would block the cached page's script.
    let is_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"));
    if response.status() != StatusCode::OK || !is_html {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let bytes = match to_bytes(body, MAX_HTML_BYTES).await {
        Ok(bytes) => bytes,
        Err(e) => {
            // The body is consumed, so there is nothing left to serve.
            warn!("web client page not served: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let html = String::from_utf8_lossy(&bytes);
    match HeaderValue::from_str(&policy_for(&html)) {
        Ok(value) => {
            parts.headers.insert(header::CONTENT_SECURITY_POLICY, value);
        }
        // Base64 and the fixed policy are always valid header text.
        Err(e) => warn!("web client policy not set: {e}"),
    }
    Response::from_parts(parts, Body::from(bytes))
}

/// [`POLICY`] with the hash of every inline script in `html` admitted.
fn policy_for(html: &str) -> String {
    let mut policy = POLICY.to_string();
    for script in inline_scripts(html) {
        let digest = Sha256::digest(script.as_bytes());
        policy.push_str(&format!(
            " 'sha256-{}'",
            general_purpose::STANDARD.encode(digest)
        ));
    }
    policy
}

/// The bodies of the `<script>` elements without a `src`. This reads the
/// build's own output, not arbitrary HTML: a body is exactly the text between
/// the opening tag and `</script>`, which is what the browser hashes.
fn inline_scripts(html: &str) -> Vec<&str> {
    let mut scripts = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let Some(close) = after.find("</script>") else {
            break;
        };
        if close > open_end && !after[..open_end].contains("src=") {
            scripts.push(&after[open_end + 1..close]);
        }
        rest = &after[close + "</script>".len()..];
    }
    scripts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_inline_scripts_only() {
        let html = "<head><script type=\"module\" src=\"/a.js\"></script></head>\
                    <body><script>\n  boot();\n</script><script>x()</script></body>";
        assert_eq!(inline_scripts(html), ["\n  boot();\n", "x()"]);
        // Known answer: sha256("x()"), base64.
        assert!(policy_for("<script>x()</script>").ends_with(
            "script-src 'self' 'wasm-unsafe-eval' 'sha256-D6IGS8VMvCoyaR/l0h9tERrBTATY01CoPS7l6xDv0kI='"
        ));
    }

    #[test]
    fn page_without_scripts_gets_the_bare_policy() {
        assert_eq!(policy_for("<p>hi</p>"), POLICY);
    }
}
