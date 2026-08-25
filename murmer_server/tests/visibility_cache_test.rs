//! Integration tests for [`VisibilityCache`], the per-connection memo the
//! broadcast fan-out uses instead of re-resolving channel permissions for
//! every recipient of every channel-scoped frame.
//!
//! A memo of an authorization decision is only as safe as its invalidation,
//! and nothing about a stale answer is visible in a smoke test: the frames a
//! demoted member should no longer receive keep arriving, silently, until the
//! connection drops. So the assertions here are about the *transitions* — a
//! channel turning private, a role losing View, a member losing the role, an
//! anonymous connection becoming a named one — rather than about a single
//! resolved answer, which `channel_overrides_test.rs` already covers.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use murmer_server::channel_overrides::{ChannelKind, OverridePair, OverrideSet};
use murmer_server::permissions::{DEFAULT_EVERYONE, SEND_MESSAGES, VIEW_CHANNELS};
use murmer_server::ws::helpers::{
    VisibilityCache, broadcast_channels_refresh, broadcast_user_roles,
};
use murmer_server::{AppState, RateLimiter, RoleDef, db};
use tokio::sync::{Mutex, broadcast};

async fn make_state() -> Arc<AppState> {
    let database = db::init(":memory:").await.expect("in-memory db");
    let (tx, _) = broadcast::channel(64);
    Arc::new(AppState {
        tx,
        channels: Arc::new(Mutex::new(HashMap::new())),
        direct: Arc::new(Mutex::new(HashMap::new())),
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
        active_webcams: Arc::new(Mutex::new(HashMap::new())),
        voice_mutes: Arc::new(Mutex::new(HashMap::new())),
        connection_stats: Arc::new(Mutex::new(HashMap::new())),
        voice_session_starts: Arc::new(Mutex::new(HashMap::new())),
        screenshare_session_starts: Arc::new(Mutex::new(HashMap::new())),
        soundboard_cooldowns: Arc::new(Mutex::new(HashMap::new())),
        upload_dir: PathBuf::from("uploads"),
        password: None,
        admin_token: Some("token".to_string()),
        rate_limiter: RateLimiter::new(),
        stats_enabled: std::sync::atomic::AtomicBool::new(false),
        chat_settings: Arc::new(Mutex::new(murmer_server::db::ChatSettings::default())),
        slow_mode_sends: Arc::new(Mutex::new(HashMap::new())),
        visibility_epoch: std::sync::atomic::AtomicU64::new(0),
    })
}

fn role(id: i64, permissions: u64, is_default: bool) -> RoleDef {
    RoleDef {
        id,
        name: format!("role-{id}"),
        color: None,
        icon: None,
        permissions,
        position: id,
        is_default,
        is_owner: false,
    }
}

const CH: i32 = 1;

/// @everyone (view + send) plus a "member" role holding the same.
async fn seed_roles(state: &Arc<AppState>) {
    let mut defs = state.role_defs.lock().await;
    defs.insert(1, role(1, DEFAULT_EVERYONE, true));
    defs.insert(2, role(2, DEFAULT_EVERYONE, false));
}

/// Deny `@everyone` View and grant it to role 2 — the shape a private channel
/// with one permitted role takes.
fn private_to_role_two() -> OverrideSet {
    OverrideSet {
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
    }
}

async fn set_overrides(state: &Arc<AppState>, set: OverrideSet) {
    state
        .channel_overrides
        .lock()
        .await
        .insert((ChannelKind::Text, CH), set);
}

#[tokio::test]
async fn repeated_asks_agree_with_the_uncached_answer() {
    let state = make_state().await;
    seed_roles(&state).await;
    let mut cache = VisibilityCache::default();

    // A channel with no overrides reaches everyone, named or not, and stays
    // that way however many frames arrive for it.
    for _ in 0..3 {
        assert!(
            cache
                .can_receive(&state, Some("anyone"), ChannelKind::Text, CH)
                .await
        );
    }

    set_overrides(&state, private_to_role_two()).await;
    broadcast_channels_refresh(&state).await;

    for _ in 0..3 {
        assert!(
            !cache
                .can_receive(&state, Some("anyone"), ChannelKind::Text, CH)
                .await
        );
    }
}

#[tokio::test]
async fn making_a_channel_private_stops_frames_reaching_a_cached_member() {
    let state = make_state().await;
    seed_roles(&state).await;
    let mut cache = VisibilityCache::default();

    // Resolve the public answer first, so the flip has a memo to invalidate.
    assert!(
        cache
            .can_receive(&state, Some("stranger"), ChannelKind::Text, CH)
            .await
    );

    set_overrides(&state, private_to_role_two()).await;
    broadcast_channels_refresh(&state).await;

    assert!(
        !cache
            .can_receive(&state, Some("stranger"), ChannelKind::Text, CH)
            .await
    );
}

#[tokio::test]
async fn losing_the_permitted_role_stops_frames_reaching_a_cached_member() {
    let state = make_state().await;
    seed_roles(&state).await;
    set_overrides(&state, private_to_role_two()).await;
    state
        .user_roles
        .lock()
        .await
        .insert("member".to_string(), vec![2]);

    let mut cache = VisibilityCache::default();
    assert!(
        cache
            .can_receive(&state, Some("member"), ChannelKind::Text, CH)
            .await
    );

    // A role reassignment announces `user-roles`, which is where the memo is
    // invalidated — no `channels-refresh` is broadcast for this.
    state
        .user_roles
        .lock()
        .await
        .insert("member".to_string(), Vec::new());
    broadcast_user_roles(&state, "member", &[]).await;

    assert!(
        !cache
            .can_receive(&state, Some("member"), ChannelKind::Text, CH)
            .await
    );
}

#[tokio::test]
async fn naming_an_anonymous_connection_discards_the_anonymous_answer() {
    let state = make_state().await;
    seed_roles(&state).await;

    // Not private — @everyone keeps View — but role 2 is denied it. An
    // anonymous viewer sees the channel; a member of role 2 must not, and the
    // transition happens on this connection with nothing broadcast.
    set_overrides(
        &state,
        OverrideSet {
            everyone: OverridePair::default(),
            roles: HashMap::from([(
                2,
                OverridePair {
                    allow: 0,
                    deny: VIEW_CHANNELS,
                },
            )]),
            users: HashMap::new(),
        },
    )
    .await;
    state
        .user_roles
        .lock()
        .await
        .insert("member".to_string(), vec![2]);

    let mut cache = VisibilityCache::default();
    assert!(cache.can_receive(&state, None, ChannelKind::Text, CH).await);
    assert!(
        !cache
            .can_receive(&state, Some("member"), ChannelKind::Text, CH)
            .await
    );
}

#[tokio::test]
async fn each_channel_is_answered_separately() {
    let state = make_state().await;
    seed_roles(&state).await;
    set_overrides(&state, private_to_role_two()).await;

    let mut cache = VisibilityCache::default();
    // The text channel is private to role 2; the voice channel sharing its id
    // has no overrides at all and must not inherit the denial.
    assert!(
        !cache
            .can_receive(&state, Some("stranger"), ChannelKind::Text, CH)
            .await
    );
    assert!(
        cache
            .can_receive(&state, Some("stranger"), ChannelKind::Voice, CH)
            .await
    );
}
