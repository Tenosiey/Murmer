//! Bot API module for programmatic server access.
//!
//! Provides REST endpoints for bots to interact with the server: reading,
//! searching, sending, editing and deleting messages, replying in threads,
//! reactions (unicode and custom emoji shortcodes), pins, typing indicators,
//! channel management and user/emoji listings.
//! Bot management (creation, deletion) requires the `ADMIN_TOKEN`.

pub mod db;
pub mod models;
pub mod routes;
