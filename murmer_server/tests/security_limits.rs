use murmer_server::{
    Clock, RateLimiter,
    security::{
        FRAME_BURST_SECONDS, FrameBudget, MAX_ADMIN_FAILURES_PER_MINUTE, RATE_WINDOW,
        SWEEP_INTERVAL, admin_token_matches, check_auth_rate_limit, check_message_rate_limit,
        validate_channel_name, validate_user_name, voice_channel_has_room,
    },
};
use serial_test::serial;
use std::collections::HashSet;
use std::time::{Duration, Instant};
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

/// The mesh cost is quadratic in the room, so the cap is what stops the
/// twentieth joiner from being felt as CPU load rather than as an error.
#[test]
#[serial]
fn caps_voice_channel_occupancy() {
    let occupants: HashSet<String> = ["alice", "bob"].iter().map(|u| u.to_string()).collect();
    with_var("MAX_VOICE_CHANNEL_USERS", Some("2"), || {
        assert!(!voice_channel_has_room(&occupants, "carol", 0));
        // Already inside, so a repeated `voice-join` is never a lockout.
        assert!(voice_channel_has_room(&occupants, "alice", 0));
        assert!(voice_channel_has_room(&HashSet::new(), "carol", 0));
        // A channel's own limit can narrow the operator's cap, never widen it.
        assert!(!voice_channel_has_room(&occupants, "carol", 5));
    });
    // `0` means no cap at all.
    with_var("MAX_VOICE_CHANNEL_USERS", Some("0"), || {
        assert!(voice_channel_has_room(&occupants, "carol", 0));
        assert!(!voice_channel_has_room(&occupants, "carol", 2));
        assert!(voice_channel_has_room(&occupants, "carol", 3));
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
fn frame_budget_allows_a_burst_then_only_the_sustained_rate() {
    let start = Instant::now();
    let mut budget = FrameBudget::new(5, start);

    // A reconnect's worth of frames goes through at once...
    for _ in 0..5 * FRAME_BURST_SECONDS {
        assert!(budget.take(start));
    }
    // ...and then nothing until the bucket refills.
    assert!(!budget.take(start));

    let one_second = start + Duration::from_secs(1);
    for _ in 0..5 {
        assert!(budget.take(one_second));
    }
    assert!(!budget.take(one_second));
}

#[test]
fn frame_budget_refills_only_up_to_the_burst() {
    let start = Instant::now();
    let mut budget = FrameBudget::new(5, start);
    assert!(budget.take(start));

    // An hour idle must not bank an hour of frames.
    let later = start + Duration::from_secs(3600);
    for _ in 0..5 * FRAME_BURST_SECONDS {
        assert!(budget.take(later));
    }
    assert!(!budget.take(later));
}

#[test]
fn frame_budget_of_zero_is_unlimited() {
    let start = Instant::now();
    let mut budget = FrameBudget::new(0, start);
    for _ in 0..10_000 {
        assert!(budget.take(start));
    }
}

/// A forwarded address is only believed from a configured proxy: anyone can
/// send the header, and believing it from a client would give that client a
/// fresh rate-limit bucket per request.
#[test]
fn client_ip_believes_forwarded_for_only_from_a_trusted_proxy() {
    use axum::http::HeaderMap;
    use murmer_server::security::client_ip;
    use std::net::IpAddr;

    let ip = |s: &str| s.parse::<IpAddr>().unwrap();
    let trusted = vec!["10.0.0.0/8".parse().unwrap()];
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "6.6.6.6, 1.2.3.4, 10.0.0.2".parse().unwrap(),
    );

    // Not from a proxy: the header is the client's own claim.
    assert_eq!(client_ip(&trusted, ip("9.9.9.9"), &headers), ip("9.9.9.9"));
    // From a proxy: the rightmost hop that is not itself a proxy. The
    // leftmost entry was written by the client and is never believed.
    assert_eq!(client_ip(&trusted, ip("10.0.0.1"), &headers), ip("1.2.3.4"));
    // No proxies configured: nothing is believed.
    assert_eq!(client_ip(&[], ip("10.0.0.1"), &headers), ip("10.0.0.1"));
    // A proxy that forwarded nothing.
    assert_eq!(
        client_ip(&trusted, ip("10.0.0.1"), &HeaderMap::new()),
        ip("10.0.0.1")
    );
}

/// `ADMIN_TOKEN` grants Owner through `/role`. Wrong guesses are capped
/// server-wide, and once the cap is hit even the right token waits out the
/// window: answering it would tell a guesser which guess was right.
#[test]
fn wrong_admin_tokens_lock_the_admin_endpoints_for_a_minute() {
    with_runtime(|rt| {
        rt.block_on(async {
            let clock = Clock::manual();
            let limiter = RateLimiter::with_clock(clock.clone());
            let token = Some("right");

            // The right token never counts against anyone.
            for _ in 0..(MAX_ADMIN_FAILURES_PER_MINUTE * 2) {
                assert!(admin_token_matches(&limiter, token, "right").await);
            }
            for _ in 0..MAX_ADMIN_FAILURES_PER_MINUTE {
                assert!(!admin_token_matches(&limiter, token, "guess").await);
            }
            assert!(!admin_token_matches(&limiter, token, "right").await);

            clock.advance(RATE_WINDOW);
            assert!(admin_token_matches(&limiter, token, "right").await);
            // No token configured matches nothing, the empty string included.
            assert!(!admin_token_matches(&limiter, None, "").await);
        });
    });
}

#[test]
#[serial]
fn a_short_admin_token_stops_the_server_from_starting() {
    with_var("ADMIN_TOKEN", Some("short"), || {
        assert!(murmer_server::config::Config::from_env().is_err());
    });
    with_var("ADMIN_TOKEN", Some(&"x".repeat(32)), || {
        assert!(murmer_server::config::Config::from_env().is_ok());
    });
}
