//! Role utilities used by the server.
//!
//! Defines the [`RoleInfo`] struct and helpers for resolving default role colors.

#[derive(Clone)]
pub struct RoleInfo {
    pub role: String,
    pub color: Option<String>,
}

const DEFAULT_ROLES: &[(&str, &str)] = &[
    ("Admin", "#eab308"),
    ("Mod", "#10b981"),
    ("Owner", "#3b82f6"),
];

/// Return the default color for a role, if one is defined.
pub fn default_color(role: &str) -> Option<String> {
    DEFAULT_ROLES
        .iter()
        .find(|(r, _)| r.eq_ignore_ascii_case(role))
        .map(|(_, c)| c.to_string())
}
