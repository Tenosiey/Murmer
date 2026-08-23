use murmer_server::{
    Clock, RateLimiter,
    security::{
        RATE_WINDOW, SWEEP_INTERVAL, check_and_store_nonce, check_auth_rate_limit,
        check_message_rate_limit, validate_channel_name, validate_timestamp, validate_user_name,
    },
};
use serial_test::serial;
use std::time::Duration;
use temp_env::with_var;
use tokio::runtime::Runtime;

fn with_runtime<F>(f: F)
where
    F: FnOnce(&Runtime),
{
    let runtime = Runtime::new().expect("failed to create tokio runtime");
    f(&runtime);
}

#[test]
#[serial]
fn rejects_message_when_limit_reached() {
    with_var("MAX_MESSAGES_PER_MINUTE", Some("2"), || {
        with_runtime(|rt| {
            rt.block_on(async {
                let limiter = RateLimiter::new();
                assert!(check_message_rate_limit(&limiter, "alice").await);
                assert!(check_message_rate_limit(&limiter, "alice").await);
                assert!(!check_message_rate_limit(&limiter, "alice").await);
            });
        });
    });
}

#[test]
#[serial]
fn rejects_auth_when_limit_reached() {
    with_var("MAX_AUTH_ATTEMPTS_PER_MINUTE", Some("1"), || {
        with_runtime(|rt| {
            rt.block_on(async {
                let limiter = RateLimiter::new();
                assert!(check_auth_rate_limit(&limiter, "127.0.0.1").await);
                assert!(!check_auth_rate_limit(&limiter, "127.0.0.1").await);
            });
        });
    });
}

#[test]
#[serial]
fn allows_nonce_reuse_after_expiry() {
    with_var("NONCE_EXPIRY_SECONDS", Some("1"), || {
        with_runtime(|rt| {
            rt.block_on(async {
                let clock = Clock::manual();
                let limiter = RateLimiter::with_clock(clock.clone());
                assert!(check_and_store_nonce(&limiter, "nonce-1").await);
                assert!(!check_and_store_nonce(&limiter, "nonce-1").await);

                clock.advance(Duration::from_secs(2));
                assert!(check_and_store_nonce(&limiter, "nonce-1").await);
            });
        });
    });
}

/// The sliding window is pruned per key on every access, so a user who hit the
/// limit is allowed again as soon as their oldest message falls out of it —
/// and only the messages that fell out are forgiven.
#[test]
#[serial]
fn frees_the_limit_as_the_window_slides() {
    with_var("MAX_MESSAGES_PER_MINUTE", Some("2"), || {
        with_runtime(|rt| {
            rt.block_on(async {
                let clock = Clock::manual();
                let limiter = RateLimiter::with_clock(clock.clone());

                assert!(check_message_rate_limit(&limiter, "alice").await);
                clock.advance(RATE_WINDOW - Duration::from_secs(1));
                assert!(check_message_rate_limit(&limiter, "alice").await);
                assert!(!check_message_rate_limit(&limiter, "alice").await);

                // Two seconds on, only the first of the two messages has aged
                // out of the window, so exactly one more is allowed.
                clock.advance(Duration::from_secs(2));
                assert!(check_message_rate_limit(&limiter, "alice").await);
                assert!(!check_message_rate_limit(&limiter, "alice").await);
            });
        });
    });
}

/// The map sweep is what stops the limiter from holding an entry for every
/// name or IP it has ever seen. It runs on a timer, so it is invisible to a
/// test that cannot move the clock: without one, a sweep that never fires
/// looks exactly like a working rate limiter.
#[test]
#[serial]
fn sweeps_keys_that_went_quiet() {
    with_var("MAX_MESSAGES_PER_MINUTE", Some("5"), || {
        with_runtime(|rt| {
            rt.block_on(async {
                let clock = Clock::manual();
                let limiter = RateLimiter::with_clock(clock.clone());

                assert!(check_message_rate_limit(&limiter, "alice").await);
                assert!(check_message_rate_limit(&limiter, "bob").await);
                assert_eq!(limiter.message_times.lock().await.entries.len(), 2);

                // Not due yet: bob is still tracked even though nobody has
                // asked about him since.
                clock.advance(SWEEP_INTERVAL - Duration::from_secs(1));
                assert!(check_message_rate_limit(&limiter, "alice").await);
                assert_eq!(limiter.message_times.lock().await.entries.len(), 2);

                // Past the interval, the next check sweeps bob out entirely
                // while alice — who is still active — keeps her window.
                clock.advance(Duration::from_secs(2));
                assert!(check_message_rate_limit(&limiter, "alice").await);
                let windows = limiter.message_times.lock().await;
                assert_eq!(windows.entries.keys().collect::<Vec<_>>(), vec!["alice"]);
            });
        });
    });
}

/// The nonce store sweeps on the same timer, and its entries are the ones that
/// grow with every authentication rather than with every distinct user.
#[test]
#[serial]
fn sweeps_expired_nonces() {
    with_var("NONCE_EXPIRY_SECONDS", Some("1"), || {
        with_runtime(|rt| {
            rt.block_on(async {
                let clock = Clock::manual();
                let limiter = RateLimiter::with_clock(clock.clone());
                assert!(check_and_store_nonce(&limiter, "nonce-1").await);
                assert!(check_and_store_nonce(&limiter, "nonce-2").await);
                assert_eq!(limiter.used_nonces.lock().await.entries.len(), 2);

                clock.advance(SWEEP_INTERVAL + Duration::from_secs(1));
                assert!(check_and_store_nonce(&limiter, "nonce-3").await);

                let nonces = limiter.used_nonces.lock().await;
                assert_eq!(nonces.entries.keys().collect::<Vec<_>>(), vec!["nonce-3"]);
            });
        });
    });
}

#[test]
fn validates_channel_names() {
    assert!(validate_channel_name("general"));
    assert!(validate_channel_name("alpha-num_01"));
    assert!(!validate_channel_name(""));
    assert!(!validate_channel_name(" leading"));
    assert!(!validate_channel_name("trailing "));
}

#[test]
fn validates_user_names() {
    assert!(validate_user_name("Alice"));
    assert!(validate_user_name("Bob 42"));
    assert!(!validate_user_name(""));
    assert!(!validate_user_name("   "));
    assert!(!validate_user_name(
        "TooLongNameThatExceedsThirtyTwoCharacters"
    ));
}

#[test]
fn validates_timestamps() {
    let now = chrono::Utc::now().timestamp_millis();
    assert!(validate_timestamp(&now.to_string()).is_ok());
    assert!(validate_timestamp(&(now + 30_000).to_string()).is_ok());
    assert!(validate_timestamp(&(now - 30_000).to_string()).is_ok());
    assert!(validate_timestamp("not-a-number").is_err());
}
