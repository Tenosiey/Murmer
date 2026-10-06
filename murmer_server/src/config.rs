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
    /// Delete channel messages older than this many days (None keeps them
    /// forever).
    pub message_retention_days: Option<u32>,
    /// Reverse proxies whose `X-Forwarded-For` header is believed.
    pub trusted_proxies: Vec<ipnet::IpNet>,
    /// Byte quotas for stored uploads.
    pub upload_quota: crate::upload::UploadQuota,
}

/// Read a quota given in megabytes as bytes, or `default` when unset. Like
/// the retention limit, a typo is fatal rather than read as "no limit".
fn quota_bytes(var: &str, default: u64) -> Result<u64> {
    match env::var(var) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map(|mb| mb.saturating_mul(1024 * 1024))
            .with_context(|| format!("{var} must be a whole number of megabytes")),
        _ => Ok(default),
    }
}

/// Used when `STUN_SERVERS` is unset. Dropping it would break direct
/// connections that work today, so opting out of Google is an explicit
/// `STUN_SERVERS=` rather than the default.
const DEFAULT_STUN_SERVER: &str = "stun:stun.l.google.com:19302";

/// Shortest `ADMIN_TOKEN` the server starts with.
const MIN_ADMIN_TOKEN_CHARS: usize = 32;

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
    /// - `MESSAGE_RETENTION_DAYS` (optional): Age after which channel messages
    ///   are deleted; unset or `0` keeps them forever
    /// - `TRUSTED_PROXIES` (optional): Comma-separated proxy addresses or
    ///   CIDR ranges whose `X-Forwarded-For` is believed
    /// - `UPLOAD_QUOTA_USER_MB`, `UPLOAD_QUOTA_TOTAL_MB` (optional): upload
    ///   storage quotas in megabytes; `0` means no limit
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
        // The token grants Owner, so a short one is refused rather than
        // guessed: the failure limit in `security` slows guessing, it does
        // not make a weak token strong.
        if admin_token
            .as_ref()
            .is_some_and(|t| t.chars().count() < MIN_ADMIN_TOKEN_CHARS)
        {
            anyhow::bail!(
                "ADMIN_TOKEN must be at least {MIN_ADMIN_TOKEN_CHARS} characters; \
                 generate one with `openssl rand -hex 32`"
            );
        }

        let cors_allowlist = Self::parse_cors_origins()?;

        // Serving the web client from the same origin as `/ws` and `/upload`
        // is what lets it work with CORS off, which is the production default.
        let web_client_dir = env::var("WEB_CLIENT_DIR")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);

        let stun_servers = parse_stun_servers(env::var("STUN_SERVERS").ok().as_deref())?;

        // A typo must not start a server that silently keeps everything (or,
        // worse, read as a shorter limit), so an unparseable value is fatal.
        let message_retention_days = match env::var("MESSAGE_RETENTION_DAYS") {
            Ok(value) if !value.trim().is_empty() => Some(
                value
                    .trim()
                    .parse::<u32>()
                    .context("MESSAGE_RETENTION_DAYS must be a whole number of days")?,
            )
            .filter(|days| *days > 0),
            _ => None,
        };

        let trusted_proxies = parse_trusted_proxies(env::var("TRUSTED_PROXIES").ok().as_deref())?;

        let defaults = crate::upload::UploadQuota::default();
        let upload_quota = crate::upload::UploadQuota {
            per_user: quota_bytes("UPLOAD_QUOTA_USER_MB", defaults.per_user)?,
            total: quota_bytes("UPLOAD_QUOTA_TOTAL_MB", defaults.total)?,
        };

        Ok(Self {
            bind_addr,
            database_path,
            upload_dir,
            password,
            admin_token,
            cors_allowlist,
            web_client_dir,
            stun_servers,
            message_retention_days,
            trusted_proxies,
            upload_quota,
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

/// Parse `TRUSTED_PROXIES`: addresses or CIDR ranges, comma separated. A typo
/// is fatal rather than skipped — a proxy silently missing from the list puts
/// every user behind it back into one rate-limit bucket.
fn parse_trusted_proxies(value: Option<&str>) -> Result<Vec<ipnet::IpNet>> {
    value
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse::<ipnet::IpNet>()
                .or_else(|_| entry.parse::<std::net::IpAddr>().map(ipnet::IpNet::from))
                .with_context(|| {
                    format!("TRUSTED_PROXIES entry {entry:?} is not an address or CIDR range")
                })
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

    #[test]
    fn trusted_proxies_take_addresses_and_ranges() {
        assert!(parse_trusted_proxies(None).unwrap().is_empty());
        let nets = parse_trusted_proxies(Some("10.0.0.1, 172.16.0.0/12,::1")).unwrap();
        assert_eq!(nets.len(), 3);
        assert!(nets[1].contains(&"172.18.0.5".parse::<std::net::IpAddr>().unwrap()));
        assert!(parse_trusted_proxies(Some("proxy.local")).is_err());
    }
}
