//! Data structures and utilities for bot management.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub fn hash_token(token: &str) -> String {
    let result = Sha256::digest(token.as_bytes());
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::random();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("mrm_{encoded}")
}

pub fn generate_bot_id() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Bitfield permission flags controlling what a bot can do.
#[derive(Debug, Clone, Copy)]
pub struct BotPermissions(pub i32);

impl BotPermissions {
    pub const READ_MESSAGES: i32 = 1 << 0;
    pub const SEND_MESSAGES: i32 = 1 << 1;
    pub const MANAGE_MESSAGES: i32 = 1 << 2;
    pub const ADD_REACTIONS: i32 = 1 << 3;
    pub const READ_CHANNELS: i32 = 1 << 4;
    pub const MANAGE_CHANNELS: i32 = 1 << 5;
    pub const READ_USERS: i32 = 1 << 6;

    pub const ALL: i32 = Self::READ_MESSAGES
        | Self::SEND_MESSAGES
        | Self::MANAGE_MESSAGES
        | Self::ADD_REACTIONS
        | Self::READ_CHANNELS
        | Self::MANAGE_CHANNELS
        | Self::READ_USERS;

    pub fn has(self, flag: i32) -> bool {
        self.0 & flag == flag
    }

    pub fn to_list(self) -> Vec<&'static str> {
        let mut list = Vec::new();
        if self.has(Self::READ_MESSAGES) {
            list.push("read_messages");
        }
        if self.has(Self::SEND_MESSAGES) {
            list.push("send_messages");
        }
        if self.has(Self::MANAGE_MESSAGES) {
            list.push("manage_messages");
        }
        if self.has(Self::ADD_REACTIONS) {
            list.push("add_reactions");
        }
        if self.has(Self::READ_CHANNELS) {
            list.push("read_channels");
        }
        if self.has(Self::MANAGE_CHANNELS) {
            list.push("manage_channels");
        }
        if self.has(Self::READ_USERS) {
            list.push("read_users");
        }
        list
    }

    pub fn from_list(names: &[String]) -> i32 {
        let mut bits = 0i32;
        for name in names {
            match name.as_str() {
                "read_messages" => bits |= Self::READ_MESSAGES,
                "send_messages" => bits |= Self::SEND_MESSAGES,
                "manage_messages" => bits |= Self::MANAGE_MESSAGES,
                "add_reactions" => bits |= Self::ADD_REACTIONS,
                "read_channels" => bits |= Self::READ_CHANNELS,
                "manage_channels" => bits |= Self::MANAGE_CHANNELS,
                "read_users" => bits |= Self::READ_USERS,
                _ => {}
            }
        }
        bits
    }
}

/// Bot as returned by the API (token hash is never exposed).
#[derive(Debug, Clone, Serialize)]
pub struct BotInfo {
    pub id: String,
    pub name: String,
    pub owner_key: String,
    pub permissions: Vec<&'static str>,
    pub description: String,
    pub active: bool,
    pub created_at: String,
}

/// Internal bot record including the token hash.
#[derive(Debug, Clone)]
pub struct BotRecord {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub owner_key: String,
    pub permissions: i32,
    pub description: String,
    pub active: bool,
    pub created_at: String,
}

impl BotRecord {
    pub fn to_info(&self) -> BotInfo {
        BotInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            owner_key: self.owner_key.clone(),
            permissions: BotPermissions(self.permissions).to_list(),
            description: self.description.clone(),
            active: self.active,
            created_at: self.created_at.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    #[serde(default)]
    pub owner_key: String,
    pub permissions: Option<Vec<String>>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBotRequest {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub description: Option<String>,
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub text: String,
    #[serde(default)]
    pub ephemeral: bool,
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AddReactionRequest {
    pub emoji: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageQuery {
    pub limit: Option<i64>,
    pub before: Option<i64>,
    pub after: Option<i64>,
}
