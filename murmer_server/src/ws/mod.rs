//! WebSocket handler and helper utilities.
//!
//! This module handles the main WebSocket connection logic for the Murmer chat server.
//!
//! ## Message Flow
//! 1. Clients connect and authenticate using Ed25519 signatures
//! 2. Server validates signatures and implements anti-replay protection
//! 3. Authenticated clients can send chat messages, create channels, and join voice channels
//! 4. All messages are broadcast to relevant subscribers
//!
//! ## Security Features
//! - Ed25519 signature-based authentication
//! - Replay attack prevention using nonces
//! - Rate limiting on messages and authentication attempts
//! - Input validation for channel names and user names
//! - Bounds checking on history requests
//!
//! ## Message Types
//! - `presence`: Authentication and user registration
//! - `chat`: Text messages with optional images
//! - `switch-channel`: Change active text channel
//! - `load-history`: Request message history
//! - `create-channel`/`delete-channel`: Channel management
//! - `voice-*`: Voice channel operations
//!
//! Messages are JSON objects with a `type` field. Clients must authenticate with a
//! presence message before sending other events.

mod constants;
mod errors;
mod handlers;
mod helpers;
mod validation;

pub use handlers::ws_handler;
