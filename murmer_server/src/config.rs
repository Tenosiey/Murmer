//! Server configuration management.
//!
//! This module handles loading and validating configuration from environment variables.

use anyhow::{Context, Result};
use axum::http::{HeaderValue, Method, header};
use std::{env, net::SocketAddr, path::PathBuf};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Server configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Socket address to bind the server to.
    pub bind_addr: SocketAddr,
    /// Path to the SQLite database file.
    pub database_path: String,
    /// Directory for storing uploaded files.
    pub upload_dir: PathBuf,
    /// Optional server password for authentication.
    pub password: Option<String>,
    /// Optional admin token for role management.
    pub admin_token: Option<String>,
    /// CORS allowlist (None means CORS is disabled).
    cors_allowlist: Option<Vec<HeaderValue>>,
    /// Directory holding the built web client, served at the site root
    /// (None means the server serves no web client).
    pub web_client_dir: Option<PathBuf>,
    /// STUN server URLs handed to clients for WebRTC ICE gathering.
    pub stun_servers: Vec<String>,
}

/// Used when `STUN_SERVERS` is unset. Dropping it would break direct
/// connections that work today, so opting out of Google is an explicit
/// `STUN_SERVERS=` rather than the default.
const DEFAULT_STUN_SERVER: &str = "stun:stun.l.google.com:19302";

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Environment Variables
    ///
    /// - `DATABASE_PATH` (optional): Path to the SQLite database file (default: `murmer.db`)
    /// - `BIND_ADDRESS` (optional): Socket address to bind to (default: `0.0.0.0:3001`)
    /// - `UPLOAD_DIR` (optional): Directory for uploads (default: `uploads`)
    /// - `SERVER_PASSWORD` (optional): Password required for client authentication
    /// - `ADMIN_TOKEN` (optional): Token for administrative operations
    /// - `CORS_ALLOW_ORIGINS` (optional): Comma-separated list of allowed origins
    /// - `WEB_CLIENT_DIR` (optional): Directory with the built web client to serve
    /// - `STUN_SERVERS` (optional): Comma-separated STUN URLs; empty for none
    pub fn from_env() -> Result<Self> {
        let database_path = env::var("DATABASE_PATH").unwrap_or_else(|_| "murmer.db".to_string());

        let bind_addr = env::var("BIND_ADDRESS")
            .unwrap_or_else(|_| "0.0.0.0:3001".to_string())
            .parse()
            .context("failed to parse BIND_ADDRESS as a socket address")?;

        let upload_dir = env::var("UPLOAD_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("uploads"));

        let password = env::var("SERVER_PASSWORD").ok().filter(|s| !s.is_empty());
        let admin_token = env::var("ADMIN_TOKEN").ok().filter(|s| !s.is_empty());

        let cors_allowlist = Self::parse_cors_origins()?;

        // Serving the web client from the same origin as `/ws` and `/upload`
        // is what lets it work with CORS off, which is the production default.
        let web_client_dir = env::var("WEB_CLIENT_DIR")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);

        let stun_servers = parse_stun_servers(env::var("STUN_SERVERS").ok().as_deref())?;

        Ok(Self {
            bind_addr,
            database_path,
            upload_dir,
            password,
            admin_token,
            cors_allowlist,
            web_client_dir,
            stun_servers,
        })
    }

    /// Parse CORS_ALLOW_ORIGINS environment variable.
    fn parse_cors_origins() -> Result<Option<Vec<HeaderValue>>> {
        match env::var("CORS_ALLOW_ORIGINS") {
            Ok(raw) => {
                let mut origins = Vec::new();
                for origin in raw.split(',') {
                    let trimmed = origin.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    origins.push(HeaderValue::from_str(trimmed).with_context(|| {
                        format!("invalid origin '{trimmed}' in CORS_ALLOW_ORIGINS")
                    })?);
                }
                Ok(if origins.is_empty() {
                    None
                } else {
                    Some(origins)
                })
            }
            Err(_) => Ok(None),
        }
    }

    /// Build a CORS layer if CORS is configured.
    ///
    /// Returns `None` if CORS is disabled (production default).
    pub fn cors_layer(&self) -> Option<CorsLayer> {
        self.cors_allowlist.as_ref().map(|origins| {
            let allowed = AllowOrigin::list(origins.clone());
            CorsLayer::new()
                .allow_origin(allowed)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        })
    }

    /// Get the list of allowed CORS origins for logging purposes.
    pub fn cors_origins(&self) -> Option<&Vec<HeaderValue>> {
        self.cors_allowlist.as_ref()
    }
}

/// Parse `STUN_SERVERS`. Unset means the built-in default; set but empty
/// means no STUN at all, which confines calls to peers that can reach each
/// other directly (a LAN).
///
/// Only `stun:`/`stuns:` is accepted: a `turn:` URL needs credentials, and
/// `RTCPeerConnection` throws on one without them — which would surface as
/// voice failing to connect for everybody, far from this setting.
fn parse_stun_servers(raw: Option<&str>) -> Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(vec![DEFAULT_STUN_SERVER.to_string()]);
    };
    raw.split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| {
            if url.starts_with("stun:") || url.starts_with("stuns:") {
                Ok(url.to_string())
            } else {
                anyhow::bail!("STUN_SERVERS entry '{url}' must start with stun: or stuns:")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stun_servers_default_empty_and_list() {
        assert_eq!(parse_stun_servers(None).unwrap(), [DEFAULT_STUN_SERVER]);
        assert!(parse_stun_servers(Some("")).unwrap().is_empty());
        assert_eq!(
            parse_stun_servers(Some(" stun:a:3478, ,stuns:b:5349 ")).unwrap(),
            ["stun:a:3478", "stuns:b:5349"]
        );
    }

    #[test]
    fn stun_servers_rejects_other_schemes() {
        assert!(parse_stun_servers(Some("turn:relay:3478")).is_err());
        assert!(parse_stun_servers(Some("stun:a,https://x")).is_err());
    }
}
