//! Security utilities for rate limiting and replay attack prevention.
//!
//! Every refusal below is counted in [`crate::metrics`] as well as logged.
//! A rejection is the one event here an operator has to see *while* it is
//! happening — a burst of them is a brute-force attempt, a spam run or a
//! limit set too low, and none of those are noticed by reading the log
//! afterwards.

use crate::{Clock, RateLimiter, SlidingWindows, metrics};
use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::{
    collections::{HashSet, VecDeque},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tracing::warn;

/// Get the maximum number of messages allowed per user per minute.
///
/// Reads from the `MAX_MESSAGES_PER_MINUTE` environment variable, defaulting to 30.
///
/// Resolved once per [`RateLimiter`] at construction rather than per call —
/// see [`RateLimiter::new`].
pub fn get_max_messages_per_minute() -> usize {
    std::env::var("MAX_MESSAGES_PER_MINUTE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
}

/// Get the maximum number of authentication attempts allowed per IP per minute.
///
/// Reads from the `MAX_AUTH_ATTEMPTS_PER_MINUTE` environment variable, defaulting to 5.
pub fn get_max_auth_attempts_per_minute() -> usize {
    std::env::var("MAX_AUTH_ATTEMPTS_PER_MINUTE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
}

/// Get the maximum number of upload attempts allowed per IP per minute.
///
/// Reads from the `MAX_UPLOADS_PER_MINUTE` environment variable, defaulting to
/// 20. Every accepted upload writes a file to disk, so this is the limit that
/// bounds how fast one client can consume the operator's storage.
pub fn get_max_uploads_per_minute() -> usize {
    std::env::var("MAX_UPLOADS_PER_MINUTE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
}

/// Get the nonce expiry time in seconds for replay attack prevention.
///
/// Reads from the `NONCE_EXPIRY_SECONDS` environment variable, defaulting to 300 (5 minutes).
pub fn get_nonce_expiry_seconds() -> u64 {
    std::env::var("NONCE_EXPIRY_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300) // 5 minutes
}

/// Whether a voice channel holding `occupants` has room for `user`.
///
/// Voice is a full mesh: one peer connection per pair, so the work every
/// client does grows with the square of the room. Without a cap the first
/// symptom of a crowded channel is everyone's CPU rather than an error, which
/// is why the limit exists at all.
///
/// Reads `MAX_VOICE_CHANNEL_USERS`, defaulting to 10; `0` disables the cap.
/// Resolved per call rather than cached because a join is a human action —
/// one environment lookup per join costs nothing.
///
/// Someone already in the channel is never refused: a client re-sending
/// `voice-join` for the channel it is already in must not lock itself out.
pub fn voice_channel_has_room(occupants: &HashSet<String>, user: &str) -> bool {
    let limit: usize = std::env::var("MAX_VOICE_CHANNEL_USERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    limit == 0 || occupants.contains(user) || occupants.len() < limit
}

/// How often the sliding-window maps are swept end to end to drop entries for
/// users/IPs that went quiet. The per-key window is always pruned on access;
/// this only bounds the memory held by keys nobody touches any more, so it can
/// run far less often than once per request.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// The sliding window the message and authentication limits are measured over.
pub const RATE_WINDOW: Duration = Duration::from_secs(60);

/// Drop timestamps older than `max_age` from the front of a window.
///
/// Ages are compared as durations rather than against a precomputed cutoff
/// `Instant`: on Linux `Instant` counts from boot, so subtracting a window
/// from "now" underflows on a server that started less than a window ago.
/// `saturating_duration_since` has no such edge.
fn cleanup_old_timestamps(timestamps: &mut VecDeque<Instant>, now: Instant, max_age: Duration) {
    while let Some(&front) = timestamps.front() {
        if now.saturating_duration_since(front) >= max_age {
            timestamps.pop_front();
        } else {
            break;
        }
    }
}

/// Borrow one key's window, creating it if this is the first time the key is
/// seen.
///
/// `entry()` would be the obvious call, but it needs an owned `String` up
/// front — allocating a key on every check even though the key almost always
/// exists already. Hashing a short name twice is cheaper than that allocation.
fn window_mut<'a>(
    windows: &'a mut std::collections::HashMap<String, VecDeque<Instant>>,
    key: &str,
) -> &'a mut VecDeque<Instant> {
    if !windows.contains_key(key) {
        windows.insert(key.to_string(), VecDeque::new());
    }
    windows
        .get_mut(key)
        .expect("window was just inserted if missing")
}

/// Record one attempt in a sliding window and report whether it fits inside
/// `max` attempts per [`RATE_WINDOW`].
///
/// Shared by the authentication, message and upload limits: all three are the
/// same sliding window over different keys, and one implementation is one
/// place where the pruning can be got wrong.
async fn check_window(
    windows: &Mutex<SlidingWindows<VecDeque<Instant>>>,
    clock: &Clock,
    key: &str,
    max: usize,
) -> bool {
    let now = clock.now();
    let mut windows = windows.lock().await;

    // Sweeping the whole map keeps entries for keys that went quiet from
    // accumulating forever, but it costs O(tracked keys) and nothing about an
    // untouched entry changes between calls — so it runs on a timer instead of
    // on every check. The window for *this* key is always pruned below, so the
    // limit itself is still exact.
    if now.duration_since(windows.last_sweep) >= SWEEP_INTERVAL {
        windows.entries.retain(|_, timestamps| {
            cleanup_old_timestamps(timestamps, now, RATE_WINDOW);
            !timestamps.is_empty()
        });
        windows.last_sweep = now;
    }

    let timestamps = window_mut(&mut windows.entries, key);
    cleanup_old_timestamps(timestamps, now, RATE_WINDOW);

    if timestamps.len() >= max {
        return false;
    }

    timestamps.push_back(now);
    true
}

/// Check if an IP address is rate limited for authentication attempts.
///
/// This function implements a sliding window rate limiter that allows up to
/// `MAX_AUTH_ATTEMPTS_PER_MINUTE` authentication attempts per IP address within
/// a 60-second window. This helps prevent brute force attacks on user accounts.
///
/// # Arguments
/// * `rate_limiter` - The shared rate limiter state
/// * `ip` - The IP address to check (should be the real client IP)
///
/// # Returns
/// * `true` if the request should be allowed
/// * `false` if the rate limit has been exceeded
pub async fn check_auth_rate_limit(rate_limiter: &RateLimiter, ip: &str) -> bool {
    let allowed = check_window(
        &rate_limiter.auth_attempts,
        &rate_limiter.clock,
        ip,
        rate_limiter.max_auth_attempts_per_minute,
    )
    .await;
    if !allowed {
        metrics::rejected(metrics::Limit::Auth);
        warn!("Rate limit exceeded for auth attempts from IP: {}", ip);
    }
    allowed
}

/// Check if an IP address is rate limited for file uploads.
///
/// Uploads are limited per IP rather than per account because the limit exists
/// to bound how fast disk can be filled, and one machine may hold any number
/// of self-generated identities.
///
/// # Arguments
/// * `rate_limiter` - The shared rate limiter state
/// * `ip` - The IP address to check (should be the real client IP)
///
/// # Returns
/// * `true` if the upload should be allowed
/// * `false` if the rate limit has been exceeded
pub async fn check_upload_rate_limit(rate_limiter: &RateLimiter, ip: &str) -> bool {
    let allowed = check_window(
        &rate_limiter.upload_attempts,
        &rate_limiter.clock,
        ip,
        rate_limiter.max_uploads_per_minute,
    )
    .await;
    if !allowed {
        metrics::rejected(metrics::Limit::Uploads);
        warn!("Rate limit exceeded for uploads from IP: {}", ip);
    }
    allowed
}

/// Check if a user is rate limited for messages.
///
/// This function implements a sliding window rate limiter that allows up to
/// `MAX_MESSAGES_PER_MINUTE` messages per user within a 60-second window.
///
/// # Arguments
/// * `rate_limiter` - The shared rate limiter state
/// * `user` - The username to check
///
/// # Returns
/// * `true` if the message should be allowed
/// * `false` if the rate limit has been exceeded
pub async fn check_message_rate_limit(rate_limiter: &RateLimiter, user: &str) -> bool {
    let allowed = check_window(
        &rate_limiter.message_times,
        &rate_limiter.clock,
        user,
        rate_limiter.max_messages_per_minute,
    )
    .await;
    if !allowed {
        metrics::rejected(metrics::Limit::Messages);
        warn!("Rate limit exceeded for messages from user: {}", user);
    }
    allowed
}

/// Check if a nonce has been used and store it for replay attack prevention.
///
/// This function implements a sliding window for nonce validation. Nonces expire after
/// `NONCE_EXPIRY_SECONDS` (default: 300 seconds). If a nonce is already in the cache,
/// it indicates a replay attack attempt.
///
/// # Arguments
/// * `rate_limiter` - The shared rate limiter state containing used nonces
/// * `nonce` - The nonce string to check (typically derived from user's public key and timestamp)
///
/// # Returns
/// * `true` if the nonce is valid and has been stored
/// * `false` if the nonce has already been used (potential replay attack)
pub async fn check_and_store_nonce(rate_limiter: &RateLimiter, nonce: &str) -> bool {
    let now = rate_limiter.clock.now();
    let mut used_nonces = rate_limiter.used_nonces.lock().await;

    let expiry = rate_limiter.nonce_expiry;

    // Expiry is checked per nonce below, so the sweep exists purely to release
    // memory and can run on a timer instead of on every authentication.
    if now.duration_since(used_nonces.last_sweep) >= SWEEP_INTERVAL {
        used_nonces
            .entries
            .retain(|_, &mut seen_at| now.saturating_duration_since(seen_at) < expiry);
        used_nonces.last_sweep = now;
    }

    // An entry that is still present but older than the expiry window has
    // already lapsed: the nonce may be used again, so treat it as unseen.
    if used_nonces
        .entries
        .get(nonce)
        .is_some_and(|seen_at| now.saturating_duration_since(*seen_at) < expiry)
    {
        metrics::rejected(metrics::Limit::Replays);
        warn!("Replay attack detected - nonce already used: {}", nonce);
        return false;
    }

    used_nonces.entries.insert(nonce.to_string(), now);
    true
}

/// Validate that a timestamp string is within an acceptable time range.
///
/// Parses a timestamp (milliseconds since Unix epoch) and requires it to be
/// within ±60 seconds of the current time. Combined with the nonce store this
/// bounds the replay window: a signature older than the window is rejected
/// here, a fresh one can only be used once.
///
/// # Arguments
/// * `timestamp_str` - The timestamp string to validate (milliseconds since Unix epoch)
///
/// # Returns
/// * `Ok(i64)` - The parsed timestamp if valid
/// * `Err(&str)` - Error message if validation fails
pub fn validate_timestamp(timestamp_str: &str) -> Result<i64, &'static str> {
    let timestamp = timestamp_str
        .parse::<i64>()
        .map_err(|_| "Invalid timestamp format")?;

    let now = chrono::Utc::now().timestamp_millis();
    if (now - timestamp).abs() > 60_000 {
        return Err("Timestamp outside acceptable window");
    }

    Ok(timestamp)
}

/// Why a signed key proof was rejected.
///
/// The WebSocket presence frame and the `/upload` endpoint prove ownership of
/// a public key the same way, so the verification lives in one place and each
/// caller maps the reason onto its own error vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofError {
    /// The public key or the signature was not valid base64.
    Encoding,
    /// The decoded public key was not the length of an Ed25519 key.
    KeyLength,
    /// The decoded bytes are not a valid Ed25519 public key.
    PublicKey,
    /// The decoded signature is not a well-formed Ed25519 signature.
    SignatureFormat,
    /// The signature does not verify against the key and message.
    Signature,
}

/// Verify that `signature` is `public_key`'s signature over `message`, with
/// both the key and the signature given as base64.
///
/// This only proves possession of the private key. Freshness (a recent
/// timestamp) and single use (the nonce store) are the caller's job — a
/// signature alone can be replayed forever.
pub fn verify_key_signature(
    public_key: &str,
    signature: &str,
    message: &str,
) -> Result<(), ProofError> {
    let (Ok(pk_bytes), Ok(sig_bytes)) = (
        general_purpose::STANDARD.decode(public_key),
        general_purpose::STANDARD.decode(signature),
    ) else {
        return Err(ProofError::Encoding);
    };

    let Ok(pk_array) = pk_bytes.as_slice().try_into() else {
        return Err(ProofError::KeyLength);
    };

    let key = VerifyingKey::from_bytes(&pk_array).map_err(|_| ProofError::PublicKey)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| ProofError::SignatureFormat)?;

    key.verify(message.as_bytes(), &signature)
        .map_err(|_| ProofError::Signature)
}

/// Generic name validator for security.
///
/// Validates that a name:
/// - Is not empty and within the specified maximum length
/// - Contains only alphanumeric characters, dashes, underscores, and spaces
/// - Has no leading or trailing whitespace
/// - Is not composed entirely of whitespace
fn validate_name(name: &str, max_length: usize) -> bool {
    if name.is_empty() || name.len() > max_length {
        return false;
    }

    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed != name {
        return false;
    }

    name.chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
}

/// Maximum length in bytes of a text, voice or category channel name.
pub const MAX_CHANNEL_NAME_LENGTH: usize = 50;

/// Validate channel name for security (max [`MAX_CHANNEL_NAME_LENGTH`] bytes).
///
/// Channel names must:
/// - Be non-empty and no longer than [`MAX_CHANNEL_NAME_LENGTH`] bytes
/// - Contain only alphanumeric characters, dashes, underscores, and spaces
/// - Not have leading or trailing whitespace
/// - Not be composed entirely of whitespace
pub fn validate_channel_name(name: &str) -> bool {
    validate_name(name, MAX_CHANNEL_NAME_LENGTH)
}

/// Validate user name for security (max 32 characters).
///
/// User names must:
/// - Be non-empty and no longer than 32 characters
/// - Contain only alphanumeric characters, dashes, underscores, and spaces
/// - Not have leading or trailing whitespace
/// - Not be composed entirely of whitespace
pub fn validate_user_name(name: &str) -> bool {
    validate_name(name, 32)
}
