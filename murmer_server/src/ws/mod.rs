//! The WebSocket endpoint: dispatch in `handlers`, the shared broadcast,
//! permission and routing helpers in `helpers`.

mod constants;
pub mod errors;
mod handlers;
pub mod helpers;
pub mod validation;

pub use handlers::{recover_claimed_scheduled_messages, spawn_scheduler, ws_handler};
