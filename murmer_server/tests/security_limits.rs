use murmer_server::{
    RateLimiter,
    security::{
        check_and_store_nonce, check_auth_rate_limit, check_message_rate_limit,
        validate_channel_name, validate_timestamp, validate_user_name,
    },
};
use serial_test::serial;
use std::time::Duration;
use temp_env::with_var;
use tokio::runtime::Runtime;
use tokio::time::sleep;

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
                let limiter = RateLimiter::new();
                assert!(check_and_store_nonce(&limiter, "nonce-1").await);
                assert!(!check_and_store_nonce(&limiter, "nonce-1").await);

                sleep(Duration::from_secs(2)).await;
                assert!(check_and_store_nonce(&limiter, "nonce-1").await);
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
