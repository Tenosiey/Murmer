//! Integration tests for point-to-point frame delivery.
//!
//! WebRTC signaling is addressed to one peer rather than broadcast, so the
//! failure modes here are silent: a frame delivered to the wrong connection
//! leaks a peer's session details, and one delivered to nobody stalls a call
//! at "Connecting…" with nothing in the logs. Neither shows up in the UI as
//! an error, so they are covered here.

use std::sync::Arc;

use murmer_server::ws::helpers::{register_direct, send_to_user, unregister_direct};
use murmer_server::{AppState, DIRECT_MAILBOX_CAPACITY, Frame, db};
use tokio::sync::mpsc;

async fn make_state() -> Arc<AppState> {
    let database = db::init(":memory:").await.expect("in-memory db");
    Arc::new(AppState {
        admin_token: Some("token".to_string()),
        ..AppState::new(database)
    })
}

/// Register a connection for `user` and hand back its mailbox.
async fn connect(state: &Arc<AppState>, user: &str, conn_id: u64) -> mpsc::Receiver<Frame> {
    let (tx, rx) = mpsc::channel::<Frame>(DIRECT_MAILBOX_CAPACITY);
    register_direct(state, user, conn_id, tx).await;
    rx
}

#[tokio::test]
async fn delivers_only_to_the_addressed_user() {
    let state = make_state().await;
    let mut alice = connect(&state, "alice", 1).await;
    let mut bob = connect(&state, "bob", 2).await;

    assert!(send_to_user(&state, "alice", Frame::from("offer-for-alice")).await);

    assert_eq!(alice.recv().await.as_deref(), Some("offer-for-alice"));
    // The whole point of the change: an unrelated peer sees nothing at all.
    assert!(bob.try_recv().is_err());
}

#[tokio::test]
async fn reaches_every_connection_of_one_user() {
    let state = make_state().await;
    // The same account signed in twice; both sessions are legitimate peers.
    let mut first = connect(&state, "alice", 1).await;
    let mut second = connect(&state, "alice", 2).await;

    assert!(send_to_user(&state, "alice", Frame::from("candidate")).await);

    assert_eq!(first.recv().await.as_deref(), Some("candidate"));
    assert_eq!(second.recv().await.as_deref(), Some("candidate"));
}

#[tokio::test]
async fn unregistering_one_connection_leaves_the_others() {
    let state = make_state().await;
    let mut first = connect(&state, "alice", 1).await;
    let mut second = connect(&state, "alice", 2).await;

    unregister_direct(&state, "alice", 1).await;
    assert!(send_to_user(&state, "alice", Frame::from("candidate")).await);

    // The departed connection's mailbox is closed, the surviving one is not.
    assert!(first.try_recv().is_err());
    assert_eq!(second.recv().await.as_deref(), Some("candidate"));
}

#[tokio::test]
async fn sending_to_an_absent_user_is_not_an_error() {
    let state = make_state().await;
    let _alice = connect(&state, "alice", 1).await;

    // A peer that already left: nothing to deliver to, and nothing to blow up.
    assert!(!send_to_user(&state, "nobody", Frame::from("offer")).await);

    unregister_direct(&state, "alice", 1).await;
    assert!(!send_to_user(&state, "alice", Frame::from("offer")).await);
}

#[tokio::test]
async fn a_full_mailbox_drops_frames_instead_of_blocking() {
    let state = make_state().await;
    // Held so the mailbox is never drained, simulating a stalled client.
    let _stalled = connect(&state, "alice", 1).await;

    for _ in 0..DIRECT_MAILBOX_CAPACITY {
        assert!(send_to_user(&state, "alice", Frame::from("filler")).await);
    }

    // Overflowing must return promptly rather than wait for room; the test
    // hanging here is the failure this guards against.
    assert!(!send_to_user(&state, "alice", Frame::from("overflow")).await);
}

#[tokio::test]
async fn dropping_a_connection_frees_its_registry_entry() {
    let state = make_state().await;
    {
        let _alice = connect(&state, "alice", 1).await;
        assert!(state.direct.lock().await.contains_key("alice"));
    }
    unregister_direct(&state, "alice", 1).await;
    // The user's entry goes with their last connection, so the registry does
    // not accumulate empty maps for everyone who ever signed in.
    assert!(!state.direct.lock().await.contains_key("alice"));
}
