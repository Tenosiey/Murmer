//! Integration tests for per-channel override resolution: private channels,
//! role/user allows, deny of Write, and the manager/administrator bypass.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use murmer_server::channel_overrides::{ChannelKind, OverridePair, OverrideSet};
use murmer_server::permissions::{
    ADMINISTRATOR, DEFAULT_EVERYONE, MANAGE_CHANNELS, SEND_MESSAGES, VIEW_CHANNELS,
};
use murmer_server::ws::helpers::{can_view_channel, has_channel_permission};
use murmer_server::{AppState, RateLimiter, RoleDef, db};
use tokio::sync::{Mutex, broadcast};

async fn make_state() -> Arc<AppState> {
    let database = db::init(":memory:").await.expect("in-memory db");
    let (tx, _) = broadcast::channel(64);
    Arc::new(AppState {
        tx,
        channels: Arc::new(Mutex::new(HashMap::new())),
        db: database,
        users: Arc::new(Mutex::new(Default::default())),
        known_users: Arc::new(Mutex::new(Default::default())),
        voice_channels: Arc::new(Mutex::new(HashMap::new())),
        role_defs: Arc::new(Mutex::new(HashMap::new())),
        user_roles: Arc::new(Mutex::new(HashMap::new())),
        channel_overrides: Arc::new(Mutex::new(HashMap::new())),
        statuses: Arc::new(Mutex::new(HashMap::new())),
        user_keys: Arc::new(Mutex::new(HashMap::new())),
        mutes: Arc::new(Mutex::new(HashMap::new())),
        active_screen_shares: Arc::new(Mutex::new(HashMap::new())),
        voice_mutes: Arc::new(Mutex::new(HashMap::new())),
        connection_stats: Arc::new(Mutex::new(HashMap::new())),
        voice_session_starts: Arc::new(Mutex::new(HashMap::new())),
        screenshare_session_starts: Arc::new(Mutex::new(HashMap::new())),
        upload_dir: PathBuf::from("uploads"),
        password: None,
        admin_token: Some("token".to_string()),
        rate_limiter: RateLimiter::new(),
    })
}

fn role(id: i64, permissions: u64, is_default: bool, is_owner: bool) -> RoleDef {
    RoleDef {
        id,
        name: format!("role-{id}"),
        color: None,
        permissions,
        position: id,
        is_default,
        is_owner,
    }
}

const CH: i32 = 1;

/// Seed base roles: @everyone (view+send), a "member" role, an admin role.
async fn seed_roles(state: &Arc<AppState>) {
    let mut defs = state.role_defs.lock().await;
    defs.insert(1, role(1, DEFAULT_EVERYONE, true, false));
    defs.insert(2, role(2, DEFAULT_EVERYONE, false, false)); // member
    defs.insert(3, role(3, ADMINISTRATOR, false, true)); // owner
}

async fn set_overrides(state: &Arc<AppState>, set: OverrideSet) {
    state
        .channel_overrides
        .lock()
        .await
        .insert((ChannelKind::Text, CH), set);
}

#[tokio::test]
async fn public_channel_visible_to_everyone() {
    let state = make_state().await;
    seed_roles(&state).await;
    assert!(can_view_channel(&state, "anyone", ChannelKind::Text, CH).await);
    assert!(has_channel_permission(&state, "anyone", ChannelKind::Text, CH, SEND_MESSAGES).await);
}

#[tokio::test]
async fn private_channel_hides_from_non_members_but_shows_to_granted() {
    let state = make_state().await;
    seed_roles(&state).await;
    // Grant role 2 (member) an explicit user; assign it to "member".
    state
        .user_roles
        .lock()
        .await
        .insert("member".to_string(), vec![2]);

    // Private: @everyone denies View; role 2 is allowed View+Send.
    let set = OverrideSet {
        everyone: OverridePair {
            allow: 0,
            deny: VIEW_CHANNELS,
        },
        roles: HashMap::from([(
            2,
            OverridePair {
                allow: VIEW_CHANNELS | SEND_MESSAGES,
                deny: 0,
            },
        )]),
        users: HashMap::new(),
    };
    set_overrides(&state, set).await;

    // A user with only @everyone cannot see it.
    assert!(!can_view_channel(&state, "stranger", ChannelKind::Text, CH).await);
    // A member of the granted role can.
    assert!(can_view_channel(&state, "member", ChannelKind::Text, CH).await);
    assert!(has_channel_permission(&state, "member", ChannelKind::Text, CH, SEND_MESSAGES).await);
}

#[tokio::test]
async fn user_override_can_grant_view_but_deny_write() {
    let state = make_state().await;
    seed_roles(&state).await;
    state
        .user_keys
        .lock()
        .await
        .insert("guest".to_string(), "guest-key".to_string());

    // The guest may see the channel but not write in it.
    let set = OverrideSet {
        everyone: OverridePair {
            allow: 0,
            deny: VIEW_CHANNELS,
        },
        roles: HashMap::new(),
        users: HashMap::from([(
            "guest-key".to_string(),
            OverridePair {
                allow: VIEW_CHANNELS,
                deny: SEND_MESSAGES,
            },
        )]),
    };
    set_overrides(&state, set).await;

    assert!(can_view_channel(&state, "guest", ChannelKind::Text, CH).await);
    assert!(!has_channel_permission(&state, "guest", ChannelKind::Text, CH, SEND_MESSAGES).await);
}

#[tokio::test]
async fn managers_and_admins_bypass_a_private_channel() {
    let state = make_state().await;
    seed_roles(&state).await;
    // Give "boss" a manager role and "owner" the administrator role.
    {
        let mut defs = state.role_defs.lock().await;
        defs.insert(4, role(4, DEFAULT_EVERYONE | MANAGE_CHANNELS, false, false));
    }
    state
        .user_roles
        .lock()
        .await
        .insert("boss".to_string(), vec![4]);
    state
        .user_roles
        .lock()
        .await
        .insert("owner".to_string(), vec![3]);

    let set = OverrideSet {
        everyone: OverridePair {
            allow: 0,
            deny: VIEW_CHANNELS,
        },
        roles: HashMap::new(),
        users: HashMap::new(),
    };
    set_overrides(&state, set).await;

    // Managers see every channel so they can administer it; admins bypass.
    assert!(can_view_channel(&state, "boss", ChannelKind::Text, CH).await);
    assert!(can_view_channel(&state, "owner", ChannelKind::Text, CH).await);
    // A plain member still cannot.
    assert!(!can_view_channel(&state, "nobody", ChannelKind::Text, CH).await);
}
