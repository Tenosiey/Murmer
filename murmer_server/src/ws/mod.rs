//! WebSocket handler and helper utilities.
//!
//! Submodules:
//! - [`handlers`] – message dispatch and domain-specific handlers
//! - [`helpers`] – broadcast, send and permission utilities
//! - [`constants`] – tuning knobs (limits, allowed roles, defaults)
//! - [`errors`] – pre-built JSON error response strings
//! - [`validation`] – input validation for status, quality and bitrate

mod constants;
pub mod errors;
mod handlers;
pub mod helpers;
pub mod validation;

pub use handlers::{recover_claimed_scheduled_messages, spawn_scheduler, ws_handler};
