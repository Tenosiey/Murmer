//! Tests for the wrapped-key store behind end-to-end encrypted channels.
//!
//! The rules worth pinning down are the ones that decide who can read a
//! channel after a membership change, and they all live in
//! `insert_channel_keys`: which epoch a write may target, who may extend an
//! existing one, and that an existing wrap is never replaced. Getting any of
//! them wrong fails silently — a removed member keeps reading, or a member's
//! wrap is swapped for one sealed to somebody else's key.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use murmer_server::channel_overrides::{ChannelKind, OverridePair, OverrideSet};
use murmer_server::permissions::{DEFAULT_EVERYONE, VIEW_CHANNELS};
use murmer_server::ws::helpers::channel_members;
use murmer_server::{AppState, RateLimiter, RoleDef, db};
use tokio::sync::{Mutex, broadcast};

const CH: i32 = 1;

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
    })
}

fn entry(recipient: &str) -> db::ChannelKeyEntry {
    db::ChannelKeyEntry {
        recipient_key: recipient.to_string(),
        nonce: "nonce".to_string(),
        wrapped_key: format!("wrapped-for-{recipient}"),
    }
}

/// Insert wraps and unwrap the result to the number of rows actually stored.
async fn put(
    database: &db::Db,
    epoch: i64,
    author: &str,
    recipients: &[&str],
) -> db::ChannelKeyWrite {
    db::insert_channel_keys(
        database,
        CH,
        epoch,
        author,
        recipients.iter().copied().map(entry).collect(),
    )
    .await
    .expect("insert")
}

#[tokio::test]
async fn first_epoch_starts_at_one_and_is_readable_by_its_recipients() {
    let state = make_state().await;
    assert_eq!(
        db::latest_channel_epoch(&state.db, CH).await.unwrap(),
        None,
        "a channel with no key has no epoch"
    );

    assert_eq!(
        put(&state.db, 1, "key-alice", &["key-alice", "key-bob"]).await,
        db::ChannelKeyWrite::Stored(2)
    );
    assert_eq!(
        db::latest_channel_epoch(&state.db, CH).await.unwrap(),
        Some(1)
    );

    let bobs = db::get_channel_keys_for(&state.db, CH, "key-bob")
        .await
        .unwrap();
    assert_eq!(bobs.len(), 1);
    assert_eq!(bobs[0].epoch, 1);
    assert_eq!(bobs[0].sender_key, "key-alice");
    assert_eq!(bobs[0].wrapped_key, "wrapped-for-key-bob");

    // Carol was not wrapped for, so the channel is unreadable to her.
    assert!(
        db::get_channel_keys_for(&state.db, CH, "key-carol")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_epoch_may_only_be_opened_in_sequence() {
    let state = make_state().await;
    // Nothing but epoch 1 may open a fresh channel.
    assert_eq!(
        put(&state.db, 2, "key-alice", &["key-alice"]).await,
        db::ChannelKeyWrite::EpochConflict
    );
    assert_eq!(
        put(&state.db, 1, "key-alice", &["key-alice"]).await,
        db::ChannelKeyWrite::Stored(1)
    );
    // Skipping an epoch would leave a hole no member could ever fill.
    assert_eq!(
        put(&state.db, 3, "key-alice", &["key-alice"]).await,
        db::ChannelKeyWrite::EpochConflict
    );
    assert_eq!(
        put(&state.db, 2, "key-alice", &["key-alice"]).await,
        db::ChannelKeyWrite::Stored(1)
    );
}

#[tokio::test]
async fn extending_an_epoch_requires_holding_it() {
    let state = make_state().await;
    put(&state.db, 1, "key-alice", &["key-alice"]).await;

    // Mallory holds no wrap at epoch 1, so she cannot be in possession of its
    // key and anything she claims to have wrapped with it is a fabrication.
    assert_eq!(
        put(&state.db, 1, "key-mallory", &["key-mallory"]).await,
        db::ChannelKeyWrite::NotAKeyHolder
    );
    assert!(
        db::get_channel_keys_for(&state.db, CH, "key-mallory")
            .await
            .unwrap()
            .is_empty()
    );

    // Alice does hold it, so she can admit Bob to the existing epoch.
    assert_eq!(
        put(&state.db, 1, "key-alice", &["key-bob"]).await,
        db::ChannelKeyWrite::Stored(1)
    );
}

#[tokio::test]
async fn an_existing_wrap_is_never_replaced() {
    let state = make_state().await;
    put(&state.db, 1, "key-alice", &["key-alice", "key-bob"]).await;

    // Bob holds epoch 1, so this write is authorized — but re-wrapping Alice's
    // slot must not overwrite it. Otherwise any member could swap another
    // member's wrap for one sealed to a key they control and read the channel
    // as them from then on.
    let overwrite = db::insert_channel_keys(
        &state.db,
        CH,
        1,
        "key-bob",
        vec![db::ChannelKeyEntry {
            recipient_key: "key-alice".to_string(),
            nonce: "nonce".to_string(),
            wrapped_key: "attacker-controlled".to_string(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(overwrite, db::ChannelKeyWrite::Stored(0));

    let alices = db::get_channel_keys_for(&state.db, CH, "key-alice")
        .await
        .unwrap();
    assert_eq!(alices[0].wrapped_key, "wrapped-for-key-alice");
    assert_eq!(alices[0].sender_key, "key-alice");
}

#[tokio::test]
async fn rotation_leaves_the_removed_member_on_the_old_epoch_only() {
    let state = make_state().await;
    put(&state.db, 1, "key-alice", &["key-alice", "key-bob"]).await;
    // Bob is removed: a new epoch is opened and simply not wrapped for him.
    put(&state.db, 2, "key-alice", &["key-alice"]).await;

    let bobs = db::get_channel_keys_for(&state.db, CH, "key-bob")
        .await
        .unwrap();
    assert_eq!(
        bobs.iter().map(|k| k.epoch).collect::<Vec<_>>(),
        vec![1],
        "the old epoch stays readable to him, the new one does not"
    );

    let holders = db::channel_epoch_recipients(&state.db, CH, 2)
        .await
        .unwrap();
    assert_eq!(holders, vec!["key-alice".to_string()]);
}

#[tokio::test]
async fn disabling_encryption_drops_every_epoch() {
    let state = make_state().await;
    put(&state.db, 1, "key-alice", &["key-alice"]).await;
    put(&state.db, 2, "key-alice", &["key-alice"]).await;

    db::delete_channel_keys(&state.db, CH).await.unwrap();
    assert_eq!(db::latest_channel_epoch(&state.db, CH).await.unwrap(), None);
    assert!(
        db::get_channel_keys_for(&state.db, CH, "key-alice")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_e2ee_flag_round_trips_and_only_private_channels_are_meant_to_carry_it() {
    let state = make_state().await;
    let channel = db::get_channel_by_id(&state.db, CH)
        .await
        .expect("the seeded general channel");
    assert!(!channel.e2ee, "channels start unencrypted");

    assert!(db::set_channel_e2ee(&state.db, CH, true).await.unwrap());
    assert!(db::get_channel_by_id(&state.db, CH).await.unwrap().e2ee);
    assert!(
        db::get_channels(&state.db)
            .await
            .iter()
            .any(|c| c.id == CH && c.e2ee),
        "the flag survives the list query, which is what the channel list sends"
    );

    assert!(db::set_channel_e2ee(&state.db, CH, false).await.unwrap());
    assert!(!db::get_channel_by_id(&state.db, CH).await.unwrap().e2ee);
}

#[tokio::test]
async fn the_key_roster_is_exactly_who_may_see_the_channel() {
    let state = make_state().await;
    {
        let mut defs = state.role_defs.lock().await;
        defs.insert(
            1,
            RoleDef {
                id: 1,
                name: "@everyone".to_string(),
                color: None,
                icon: None,
                permissions: DEFAULT_EVERYONE,
                position: 0,
                is_default: true,
                is_owner: false,
            },
        );
    }
    for (user, key) in [
        ("alice", "key-alice"),
        ("bob", "key-bob"),
        ("botty", "key-botty"),
    ] {
        state.known_users.lock().await.insert(user.to_string());
        if user != "botty" {
            state
                .user_keys
                .lock()
                .await
                .insert(user.to_string(), key.to_string());
        }
    }

    // Private channel: @everyone loses View, Alice is granted it back.
    state.channel_overrides.lock().await.insert(
        (ChannelKind::Text, CH),
        OverrideSet {
            everyone: OverridePair {
                allow: 0,
                deny: VIEW_CHANNELS,
            },
            roles: HashMap::new(),
            users: HashMap::from([(
                "key-alice".to_string(),
                OverridePair {
                    allow: VIEW_CHANNELS,
                    deny: 0,
                },
            )]),
        },
    );

    let members = channel_members(&state, CH).await;
    assert_eq!(
        members,
        vec![("alice".to_string(), "key-alice".to_string())],
        "Bob cannot see the channel and botty has no key to encrypt to"
    );
}
