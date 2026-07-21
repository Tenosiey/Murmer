//! Role definitions and helpers.
//!
//! A [`RoleDef`] is a named, colored bundle of [permissions](crate::permissions)
//! with a `position` used for hierarchy (higher outranks lower). Two roles are
//! special and protected from deletion:
//! - the **default** role (`@everyone`) whose permissions apply to every user
//!   as the baseline of the permission union; it is never assigned explicitly.
//! - the **owner** role which always holds [`ADMINISTRATOR`](crate::permissions::ADMINISTRATOR)
//!   and sits at the top of the hierarchy.

use crate::permissions::{self, Permissions};

/// A server role: an id, display name/color, a permission bitmask and a
/// hierarchy position. `is_default` marks the implicit `@everyone` baseline;
/// `is_owner` marks the protected administrator role.
#[derive(Clone, Debug)]
pub struct RoleDef {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub permissions: Permissions,
    pub position: i64,
    pub is_default: bool,
    pub is_owner: bool,
}

/// Reserved display name for the implicit baseline role.
pub const EVERYONE_ROLE_NAME: &str = "@everyone";

/// A built-in role seeded on first initialization. `position` fixes the
/// starting hierarchy; custom roles are created above `@everyone` and below
/// `Owner`.
pub struct BuiltinRole {
    pub name: &'static str,
    pub color: Option<&'static str>,
    pub permissions: Permissions,
    pub position: i64,
    pub is_default: bool,
    pub is_owner: bool,
}

/// The roles every server starts with. Ordered low to high by `position`.
pub const BUILTIN_ROLES: &[BuiltinRole] = &[
    BuiltinRole {
        name: EVERYONE_ROLE_NAME,
        color: None,
        permissions: permissions::DEFAULT_EVERYONE,
        position: 0,
        is_default: true,
        is_owner: false,
    },
    BuiltinRole {
        name: "Mod",
        color: Some("#10b981"),
        permissions: permissions::DEFAULT_MOD,
        position: 1,
        is_default: false,
        is_owner: false,
    },
    BuiltinRole {
        name: "Admin",
        color: Some("#eab308"),
        permissions: permissions::DEFAULT_ADMIN,
        position: 2,
        is_default: false,
        is_owner: false,
    },
    BuiltinRole {
        name: "Owner",
        color: Some("#3b82f6"),
        permissions: permissions::DEFAULT_OWNER,
        position: 3,
        is_default: false,
        is_owner: true,
    },
];

/// Return the default color for a built-in role name, if one is defined.
/// Used when the CLI/HTTP role endpoints create a role without an explicit
/// color.
pub fn default_color(role: &str) -> Option<String> {
    BUILTIN_ROLES
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(role))
        .and_then(|r| r.color)
        .map(str::to_string)
}
