//! Server-wide permission bitmask shared by every authorization check.
//!
//! A role grants a set of capabilities encoded as bit flags. A user's
//! effective permissions are the union (bitwise OR) of the built-in
//! `@everyone` baseline role and every role assigned to them; holding
//! [`ADMINISTRATOR`] short-circuits every check. The mask is mirrored on the
//! client in `murmer_client/src/lib/chat/permissions.ts` — keep the two in
//! sync when adding or renaming a flag.
//!
//! Enforcement is always server-side: clients use the same flags only to show
//! or hide UI, never as the source of truth.

/// Bitmask type used for role permissions throughout the server.
pub type Permissions = u64;

/// See channels and read message history. Newly enforced by the role system.
pub const VIEW_CHANNELS: Permissions = 1 << 0;
/// Send chat messages and add reactions. Newly enforced by the role system.
pub const SEND_MESSAGES: Permissions = 1 << 1;
/// Delete or pin messages authored by other users.
pub const MANAGE_MESSAGES: Permissions = 1 << 2;
/// Create, delete, move, reorder and re-topic channels and categories.
pub const MANAGE_CHANNELS: Permissions = 1 << 3;
/// Create, edit, rename and delete wiki pages.
pub const MANAGE_WIKI: Permissions = 1 << 4;
/// Create, edit, delete, reorder and assign roles.
pub const MANAGE_ROLES: Permissions = 1 << 5;
/// Add or remove custom server emojis.
pub const MANAGE_EMOJIS: Permissions = 1 << 6;
/// Edit server identity, stat tracking, screen-share cap and other settings.
pub const MANAGE_SERVER: Permissions = 1 << 7;
/// Query server details such as the running version.
pub const VIEW_SERVER_INFO: Permissions = 1 << 8;
/// View other users' self-reported connection stats.
pub const VIEW_CONNECTION_STATS: Permissions = 1 << 9;
/// Kick members from the server.
pub const KICK_MEMBERS: Permissions = 1 << 10;
/// Ban members from the server.
pub const BAN_MEMBERS: Permissions = 1 << 11;
/// Mute members.
pub const MUTE_MEMBERS: Permissions = 1 << 12;
/// Grants every permission and bypasses hierarchy checks (the Owner role).
pub const ADMINISTRATOR: Permissions = 1 << 13;

/// Union of every defined permission flag. Used to reject unknown bits from
/// clients and to expand [`ADMINISTRATOR`] into a concrete mask.
pub const ALL: Permissions = VIEW_CHANNELS
    | SEND_MESSAGES
    | MANAGE_MESSAGES
    | MANAGE_CHANNELS
    | MANAGE_WIKI
    | MANAGE_ROLES
    | MANAGE_EMOJIS
    | MANAGE_SERVER
    | VIEW_SERVER_INFO
    | VIEW_CONNECTION_STATS
    | KICK_MEMBERS
    | BAN_MEMBERS
    | MUTE_MEMBERS
    | ADMINISTRATOR;

/// Baseline permissions granted to every user through the `@everyone` role.
/// Keeps a fresh or unadministered server usable: everyone can read and chat.
pub const DEFAULT_EVERYONE: Permissions = VIEW_CHANNELS | SEND_MESSAGES;

/// Default permissions seeded for the built-in `Mod` role. Mirrors the legacy
/// Mod capabilities: manage channels/wiki/emojis, moderate messages and act
/// against lower-ranked members.
pub const DEFAULT_MOD: Permissions = DEFAULT_EVERYONE
    | MANAGE_MESSAGES
    | MANAGE_CHANNELS
    | MANAGE_WIKI
    | MANAGE_EMOJIS
    | KICK_MEMBERS
    | BAN_MEMBERS
    | MUTE_MEMBERS;

/// Default permissions seeded for the built-in `Admin` role: everything a Mod
/// can do plus server settings and read-only server/connection insight.
pub const DEFAULT_ADMIN: Permissions =
    DEFAULT_MOD | MANAGE_SERVER | VIEW_SERVER_INFO | VIEW_CONNECTION_STATS;

/// Default permissions seeded for the built-in `Owner` role.
pub const DEFAULT_OWNER: Permissions = ADMINISTRATOR;

/// Whether a permission mask satisfies `required`. [`ADMINISTRATOR`] grants
/// everything.
pub fn mask_allows(mask: Permissions, required: Permissions) -> bool {
    mask & ADMINISTRATOR != 0 || mask & required == required
}

/// Whether a mask only uses defined flags (rejects unknown bits from clients).
pub fn is_valid_mask(mask: Permissions) -> bool {
    mask & !ALL == 0
}
