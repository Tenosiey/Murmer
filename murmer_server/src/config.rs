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
    /// PostgreSQL connection string.
    pub database_url: String,
    /// Directory for storing uploaded files.
    pub upload_dir: PathBuf,
    /// Optional server password for authentication.
    pub password: Option<String>,
    /// Optional admin token for role management.
    pub admin_token: Option<String>,
    /// CORS allowlist (None means CORS is disabled).
    cors_allowlist: Option<Vec<HeaderValue>>,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Environment Variables
    ///
    /// - `DATABASE_URL` (required): PostgreSQL connection string
    /// - `BIND_ADDRESS` (optional): Socket address to bind to (default: `0.0.0.0:3001`)
    /// - `UPLOAD_DIR` (optional): Directory for uploads (default: `uploads`)
    /// - `SERVER_PASSWORD` (optional): Password required for client authentication
    /// - `ADMIN_TOKEN` (optional): Token for administrative operations
    /// - `CORS_ALLOW_ORIGINS` (optional): Comma-separated list of allowed origins
    pub fn from_env() -> Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required"))?;

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

        Ok(Self {
            bind_addr,
            database_url,
            upload_dir,
            password,
            admin_token,
            cors_allowlist,
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
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        })
    }

    /// Get the list of allowed CORS origins for logging purposes.
    pub fn cors_origins(&self) -> Option<&Vec<HeaderValue>> {
        self.cors_allowlist.as_ref()
    }
}
