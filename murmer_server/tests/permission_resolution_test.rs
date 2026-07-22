//! Integration tests for the in-memory permission resolver: effective
//! permissions (union of `@everyone` + assigned roles), the hierarchy position,
//! and the no-`ADMIN_TOKEN` channel/wiki fallback.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use murmer_server::permissions::{
    ADMINISTRATOR, DEFAULT_EVERYONE, MANAGE_CHANNELS, MANAGE_EMOJIS, SEND_MESSAGES, VIEW_CHANNELS,
};
use murmer_server::ws::helpers::{effective_permissions, has_permission, top_position};
use murmer_server::{AppState, RateLimiter, RoleDef, db};
use tokio::sync::{Mutex, broadcast};

async fn make_state(admin_token: Option<&str>) -> Arc<AppState> {
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
        admin_token: admin_token.map(str::to_string),
        rate_limiter: RateLimiter::new(),
    })
}

fn role(id: i64, permissions: u64, position: i64, is_default: bool, is_owner: bool) -> RoleDef {
    RoleDef {
        id,
        name: format!("role-{id}"),
        color: None,
        permissions,
        position,
        is_default,
        is_owner,
    }
}

async fn seed(state: &Arc<AppState>, defs: Vec<RoleDef>, assignments: &[(&str, Vec<i64>)]) {
    let mut map = state.role_defs.lock().await;
    for def in defs {
        map.insert(def.id, def);
    }
    drop(map);
    let mut ur = state.user_roles.lock().await;
    for (user, ids) in assignments {
        ur.insert((*user).to_string(), ids.clone());
    }
}

#[tokio::test]
async fn everyone_baseline_applies_without_assignment() {
    let state = make_state(Some("token")).await;
    seed(&state, vec![role(1, DEFAULT_EVERYONE, 0, true, false)], &[]).await;

    // A user with no explicit roles still inherits @everyone.
    assert!(has_permission(&state, "nobody", VIEW_CHANNELS).await);
    assert!(has_permission(&state, "nobody", SEND_MESSAGES).await);
    assert!(!has_permission(&state, "nobody", MANAGE_CHANNELS).await);
}

#[tokio::test]
async fn view_only_role_cannot_send_when_baseline_lacks_send() {
    let state = make_state(Some("token")).await;
    // The "Dude" scenario: @everyone is view-only, so a member with only a
    // view-only role cannot send, while a member holding a role that grants
    // SEND can.
    seed(
        &state,
        vec![
            role(1, VIEW_CHANNELS, 0, true, false),
            role(2, VIEW_CHANNELS, 1, false, false), // "Dude"
            role(3, VIEW_CHANNELS | SEND_MESSAGES, 2, false, false), // "Member"
        ],
        &[("dude", vec![2]), ("member", vec![3])],
    )
    .await;

    assert!(has_permission(&state, "dude", VIEW_CHANNELS).await);
    assert!(!has_permission(&state, "dude", SEND_MESSAGES).await);
    assert!(has_permission(&state, "member", SEND_MESSAGES).await);
}

#[tokio::test]
async fn permissions_stack_and_administrator_grants_all() {
    let state = make_state(Some("token")).await;
    seed(
        &state,
        vec![
            role(1, VIEW_CHANNELS, 0, true, false),
            role(2, SEND_MESSAGES, 1, false, false),
            role(3, MANAGE_EMOJIS, 2, false, false),
            role(4, ADMINISTRATOR, 5, false, true),
        ],
        &[("stacker", vec![2, 3]), ("owner", vec![4])],
    )
    .await;

    // Union of @everyone + both assigned roles.
    let stacker = effective_permissions(&state, "stacker").await;
    assert!(stacker & VIEW_CHANNELS != 0);
    assert!(stacker & SEND_MESSAGES != 0);
    assert!(stacker & MANAGE_EMOJIS != 0);
    assert!(stacker & MANAGE_CHANNELS == 0);

    // Administrator satisfies every check and sits above everyone.
    assert!(has_permission(&state, "owner", MANAGE_CHANNELS).await);
    assert_eq!(top_position(&state, "owner").await, i64::MAX);
    assert!(top_position(&state, "stacker").await >= top_position(&state, "dude-missing").await);
}

#[tokio::test]
async fn channel_management_is_open_without_admin_token() {
    let state = make_state(None).await;
    seed(&state, vec![role(1, VIEW_CHANNELS, 0, true, false)], &[]).await;

    // No ADMIN_TOKEN: channel and wiki management fall open, but other
    // capabilities stay role-gated.
    assert!(has_permission(&state, "anyone", MANAGE_CHANNELS).await);
    assert!(!has_permission(&state, "anyone", MANAGE_EMOJIS).await);
}
