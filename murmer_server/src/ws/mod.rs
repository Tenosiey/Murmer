//! WebSocket handler and helper utilities.
//!
//! Submodules:
//! - [`handlers`] – message dispatch and domain-specific handlers
//! - [`helpers`] – broadcast, send and permission utilities
//! - [`constants`] – tuning knobs (limits, allowed roles, defaults)
//! - [`errors`] – pre-built JSON error response strings
//! - [`validation`] – input validation for status, quality and bitrate

mod constants;
mod errors;
mod handlers;
pub mod helpers;
mod validation;

pub use handlers::ws_handler;
