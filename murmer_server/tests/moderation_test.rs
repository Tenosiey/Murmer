//! Permission-bitmask semantics that back moderation and every other
//! authorization check.

use murmer_server::permissions::{
    self, ADMINISTRATOR, ALL, DEFAULT_ADMIN, DEFAULT_EVERYONE, DEFAULT_MOD, DEFAULT_OWNER,
    KICK_MEMBERS, MANAGE_CHANNELS, MANAGE_SERVER, SEND_MESSAGES, VIEW_CHANNELS,
};

#[test]
fn administrator_grants_everything() {
    assert!(permissions::mask_allows(ADMINISTRATOR, MANAGE_CHANNELS));
    assert!(permissions::mask_allows(ADMINISTRATOR, KICK_MEMBERS));
    assert!(permissions::mask_allows(ADMINISTRATOR, MANAGE_SERVER));
}

#[test]
fn exact_flag_is_required_without_administrator() {
    assert!(permissions::mask_allows(MANAGE_CHANNELS, MANAGE_CHANNELS));
    assert!(!permissions::mask_allows(VIEW_CHANNELS, MANAGE_CHANNELS));
    assert!(!permissions::mask_allows(0, SEND_MESSAGES));
}

#[test]
fn unknown_bits_are_rejected() {
    assert!(permissions::is_valid_mask(ALL));
    assert!(permissions::is_valid_mask(DEFAULT_MOD));
    assert!(!permissions::is_valid_mask(1 << 60));
    assert!(!permissions::is_valid_mask(ALL | (1 << 40)));
}

#[test]
fn default_role_sets_match_expectations() {
    // Baseline: read and chat, but nothing privileged.
    assert!(permissions::mask_allows(DEFAULT_EVERYONE, VIEW_CHANNELS));
    assert!(permissions::mask_allows(DEFAULT_EVERYONE, SEND_MESSAGES));
    assert!(!permissions::mask_allows(DEFAULT_EVERYONE, MANAGE_CHANNELS));

    // Mods manage channels and moderate, but do not touch server settings.
    assert!(permissions::mask_allows(DEFAULT_MOD, MANAGE_CHANNELS));
    assert!(permissions::mask_allows(DEFAULT_MOD, KICK_MEMBERS));
    assert!(!permissions::mask_allows(DEFAULT_MOD, MANAGE_SERVER));

    // Admins additionally manage server settings.
    assert!(permissions::mask_allows(DEFAULT_ADMIN, MANAGE_SERVER));

    // Owner is a plain administrator mask.
    assert_eq!(DEFAULT_OWNER, ADMINISTRATOR);
}

#[test]
fn permission_union_stacks() {
    // Stacked roles union their permissions (the effective-permission model).
    let stacked = DEFAULT_EVERYONE | KICK_MEMBERS;
    assert!(permissions::mask_allows(stacked, SEND_MESSAGES));
    assert!(permissions::mask_allows(stacked, KICK_MEMBERS));
    assert!(!permissions::mask_allows(stacked, MANAGE_SERVER));
}
