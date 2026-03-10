//! Bot API module for programmatic server access.
//!
//! Provides REST endpoints for bots to interact with the server,
//! including reading/sending messages, listing channels and users.
//! Bot management (creation, deletion) requires the `ADMIN_TOKEN`.

pub mod db;
pub mod models;
pub mod routes;
