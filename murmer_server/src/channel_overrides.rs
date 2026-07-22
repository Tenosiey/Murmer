//! In-memory representation of per-channel permission overrides.
//!
//! Each channel may layer allow/deny overrides on top of a user's server-wide
//! permissions, for `@everyone`, for individual roles, and for individual
//! users. Only the [`CHANNEL_OVERRIDABLE`](crate::permissions::CHANNEL_OVERRIDABLE)
//! bits (see + write/talk) are ever set. A channel is considered *private* when
//! its `@everyone` override denies `VIEW_CHANNELS`.

use crate::permissions::{self, Permissions};
use std::collections::HashMap;

/// Text and voice channels use separate id spaces, so an override is keyed by
/// both a kind and an id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ChannelKind {
    Text,
    Voice,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Text => "text",
            ChannelKind::Voice => "voice",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "text" => Some(ChannelKind::Text),
            "voice" => Some(ChannelKind::Voice),
            _ => None,
        }
    }

    /// Map the `voice` boolean carried by many channel frames to a kind.
    pub fn from_voice(voice: bool) -> Self {
        if voice {
            ChannelKind::Voice
        } else {
            ChannelKind::Text
        }
    }
}

/// One override target's allow/deny masks.
#[derive(Clone, Copy, Default, Debug)]
pub struct OverridePair {
    pub allow: Permissions,
    pub deny: Permissions,
}

impl OverridePair {
    pub fn is_empty(&self) -> bool {
        self.allow == 0 && self.deny == 0
    }
}

/// Every override configured on a single channel.
#[derive(Clone, Default, Debug)]
pub struct OverrideSet {
    pub everyone: OverridePair,
    /// Role id → override.
    pub roles: HashMap<i64, OverridePair>,
    /// User public key → override.
    pub users: HashMap<String, OverridePair>,
}

impl OverrideSet {
    /// Resolve a user's effective mask for this channel from a server-wide
    /// `base`, applying overrides in Discord order: `@everyone`, then the union
    /// of the user's role overrides (denies then allows), then the
    /// user-specific override.
    pub fn apply(
        &self,
        base: Permissions,
        role_ids: &[i64],
        user_key: Option<&str>,
    ) -> Permissions {
        let mut mask = (base & !self.everyone.deny) | self.everyone.allow;

        let mut role_deny: Permissions = 0;
        let mut role_allow: Permissions = 0;
        for id in role_ids {
            if let Some(pair) = self.roles.get(id) {
                role_deny |= pair.deny;
                role_allow |= pair.allow;
            }
        }
        mask = (mask & !role_deny) | role_allow;

        if let Some(key) = user_key
            && let Some(pair) = self.users.get(key)
        {
            mask = (mask & !pair.deny) | pair.allow;
        }
        mask
    }

    /// Whether the channel hides itself from `@everyone` (denies View).
    pub fn restricts_view(&self) -> bool {
        self.everyone.deny & permissions::VIEW_CHANNELS != 0
    }

    pub fn is_empty(&self) -> bool {
        self.everyone.is_empty() && self.roles.is_empty() && self.users.is_empty()
    }
}
