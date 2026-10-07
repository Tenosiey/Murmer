//! Security utilities: rate limiting, key proofs and input validation.
//!
//! Every refusal below is counted in [`crate::metrics`] as well as logged.
//! A rejection is the one event here an operator has to see *while* it is
//! happening — a burst of them is a brute-force attempt, a spam run or a
//! limit set too low, and none of those are noticed by reading the log
//! afterwards.

use crate::{Clock, RateLimiter, SlidingWindows, VoiceMode, metrics};
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

/// Get the sustained number of WebSocket frames one connection may send per
/// second.
///
/// Reads from the `MAX_FRAMES_PER_SECOND` environment variable, defaulting to
/// 20; `0` disables the limit. Resolved once per [`RateLimiter`], like the
/// limits above.
pub fn get_max_frames_per_second() -> u32 {
    std::env::var("MAX_FRAMES_PER_SECOND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
}

/// The server-wide cap on one voice channel's headcount
/// (`MAX_VOICE_CHANNEL_USERS`, default 10, `0` for none).
///
/// Resolved per call rather than cached because it is read on a join, which
/// is a human action — one environment lookup per join costs nothing.
pub fn max_voice_channel_users() -> usize {
    std::env::var("MAX_VOICE_CHANNEL_USERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
}

/// Whether a voice channel holding `occupants` has room for `user`.
///
/// Voice is a full mesh: one peer connection per pair, so the work every
/// client does grows with the square of the room. Without a cap the first
/// symptom of a crowded channel is everyone's CPU rather than an error, which
/// is why the limit exists at all.
///
/// Someone already in the channel is never refused: a client re-sending
/// `voice-join` for the channel it is already in must not lock itself out.
///
/// `channel_limit` is the channel's own limit (`0` for none); the stricter of
/// it and [`max_voice_channel_users`] wins, so a moderator can narrow the
/// operator's cap but never widen it.
pub fn voice_channel_has_room(occupants: &HashSet<String>, user: &str, channel_limit: u32) -> bool {
    let limit = [max_voice_channel_users(), channel_limit as usize]
        .into_iter()
        .filter(|&n| n > 0)
        .min();
    occupants.contains(user) || limit.is_none_or(|limit| occupants.len() < limit)
}

/// How far below the SFU threshold a channel must shrink before it returns
/// to the mesh. Without the gap a channel sitting at the threshold would
/// tear down and rebuild every connection each time someone's Wi-Fi
/// blinked. A constant, because nobody tunes two numbers.
pub const SFU_HYSTERESIS: usize = 3;

/// The mode a voice channel of `headcount` members should be in.
///
/// `threshold` is `None` while the SFU is off, which keeps every channel on
/// the mesh. An empty channel always resets to the mesh; the caller clears
/// `sticky` with it.
pub fn next_mode(
    current: VoiceMode,
    headcount: usize,
    sticky: bool,
    threshold: Option<usize>,
) -> VoiceMode {
    let Some(threshold) = threshold else {
        return VoiceMode::Mesh;
    };
    let sfu = match current {
        _ if headcount == 0 => false,
        _ if sticky => true,
        VoiceMode::Mesh => headcount >= threshold,
        VoiceMode::Sfu => headcount > threshold.saturating_sub(SFU_HYSTERESIS),
    };
    if sfu { VoiceMode::Sfu } else { VoiceMode::Mesh }
}

/// How often the sliding-window maps are swept end to end to drop entries for
/// users/IPs that went quiet. The per-key window is always pruned on access;
/// this only bounds the memory held by keys nobody touches any more, so it can
/// run far less often than once per request.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// The sliding window the message and authentication limits are measured over.
pub const RATE_WINDOW: Duration = Duration::from_secs(60);

/// How many seconds of the sustained frame rate a connection may spend at
/// once. See [`FrameBudget`].
pub const FRAME_BURST_SECONDS: u32 = 10;

/// A token bucket over every frame one connection sends.
///
/// The per-account message limit covers only the frames that post something.
/// Everything else — a full-text search, a history page, a reaction — still
/// queues on the one database thread, so a single client in a loop slowed the
/// server down for everyone. This caps all of them at once.
///
/// A bucket rather than the sliding window the other limits use, because a
/// well-behaved client is bursty: connecting sends a dozen requests, joining a
/// full voice mesh sends an offer and a run of ICE candidates to every peer,
/// and both can coincide on a reconnect. The bucket lets
/// [`FRAME_BURST_SECONDS`] worth of frames through at once and only caps the
/// sustained rate. It lives in the socket loop, one per connection, so it
/// needs no lock and disappears with the connection.
pub struct FrameBudget {
    tokens: f64,
    capacity: f64,
    per_second: f64,
    last: Instant,
}

impl FrameBudget {
    /// A full bucket refilling at `per_second`; `0` never refuses a frame.
    pub fn new(per_second: u32, now: Instant) -> Self {
        let capacity = f64::from(per_second.saturating_mul(FRAME_BURST_SECONDS));
        Self {
            tokens: capacity,
            capacity,
            per_second: f64::from(per_second),
            last: now,
        }
    }

    /// Spend one frame's worth of budget, reporting whether there was any.
    pub fn take(&mut self, now: Instant) -> bool {
        if self.per_second == 0.0 {
            return true;
        }
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.per_second).min(self.capacity);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

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

/// Wrong `ADMIN_TOKEN`s accepted per minute across the whole server before
/// every admin request is refused until the window clears.
pub const MAX_ADMIN_FAILURES_PER_MINUTE: usize = 10;

/// Whether `given` is the configured admin token, refusing outright while
/// too many wrong ones have been presented in the last minute.
///
/// The token grants Owner through `/role`, so an online guess at a weak one
/// is a full takeover. Only failures count, so scripts using the right token
/// never trip it, and the window is server-wide rather than per IP because a
/// guesser rotating addresses is exactly who it is for. The cost is that a
/// flood of wrong guesses locks the admin endpoints for everyone for a
/// minute, which is the right way round for them to fail.
pub async fn admin_token_matches(
    rate_limiter: &RateLimiter,
    expected: Option<&str>,
    given: &str,
) -> bool {
    use subtle::ConstantTimeEq;
    let now = rate_limiter.clock.now();
    let mut failures = rate_limiter.admin_failures.lock().await;
    cleanup_old_timestamps(&mut failures, now, RATE_WINDOW);
    if failures.len() >= MAX_ADMIN_FAILURES_PER_MINUTE {
        metrics::rejected(metrics::Limit::Auth);
        warn!("Refusing admin request: too many wrong admin tokens");
        return false;
    }
    let matches = expected.is_some_and(|e| e.as_bytes().ct_eq(given.as_bytes()).into());
    if !matches {
        failures.push_back(now);
    }
    matches
}

/// Link previews one IP may have fetched per minute. Cached previews are
/// free; this caps the pages a caller can make the server go and fetch, which
/// is what keeps `/link-preview` from being a free anonymous fetch proxy.
pub const MAX_PREVIEWS_PER_MINUTE: usize = 30;

/// Check if an IP may make the server fetch another link preview.
pub async fn check_preview_rate_limit(rate_limiter: &RateLimiter, ip: &str) -> bool {
    let allowed = check_window(
        &rate_limiter.preview_attempts,
        &rate_limiter.clock,
        ip,
        MAX_PREVIEWS_PER_MINUTE,
    )
    .await;
    if !allowed {
        metrics::rejected(metrics::Limit::Previews);
        warn!("Rate limit exceeded for link previews from IP: {}", ip);
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

/// The address a request really came from, which every per-IP limit keys on.
///
/// Behind a reverse proxy — which TLS all but requires — the socket peer is
/// the proxy, so without this every user shares one bucket and a handful of
/// failed logins locks the whole server out. `X-Forwarded-For` is only
/// believed when the peer is one of `trusted` (`TRUSTED_PROXIES`): anyone can
/// send the header, so reading it from an untrusted peer would let a client
/// pick a fresh address per request. The list is walked from the right, the
/// end each proxy appends to, and the first hop that is not itself a trusted
/// proxy is the client; everything left of it was written by the client.
pub fn client_ip(
    trusted: &[ipnet::IpNet],
    peer: std::net::IpAddr,
    headers: &axum::http::HeaderMap,
) -> std::net::IpAddr {
    let is_trusted = |ip: &std::net::IpAddr| trusted.iter().any(|net| net.contains(ip));
    if !is_trusted(&peer) {
        return peer;
    }
    let hops: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .collect();
    let mut client = peer;
    for hop in hops.into_iter().rev() {
        let Ok(ip) = hop.trim().parse() else { break };
        client = ip;
        if !is_trusted(&ip) {
            break;
        }
    }
    client
}

/// Why a signed key proof was rejected, so the presence handler can answer
/// with a specific error code.
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
/// This only proves possession of the private key. Binding the message to
/// something only this server could have issued is the caller's job — a
/// signature alone can be replayed forever, on any server.
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

#[cfg(test)]
mod tests {
    use super::*;
    use VoiceMode::{Mesh, Sfu};

    #[test]
    fn next_mode_switches_with_hysteresis() {
        let t = Some(7);
        // (current, headcount, sticky) -> expected
        let table = [
            ((Mesh, 6, false), Mesh),
            ((Mesh, 7, false), Sfu),
            ((Sfu, 7, false), Sfu),
            // Inside the band the channel keeps whatever it had.
            ((Sfu, 5, false), Sfu),
            ((Mesh, 5, false), Mesh),
            ((Sfu, 4, false), Mesh),
            // A sticky channel stays on the SFU at any size but empty.
            ((Mesh, 2, true), Sfu),
            ((Sfu, 1, true), Sfu),
            ((Sfu, 0, true), Mesh),
            ((Sfu, 0, false), Mesh),
        ];
        for ((current, headcount, sticky), want) in table {
            assert_eq!(
                next_mode(current, headcount, sticky, t),
                want,
                "{current:?} with {headcount} people, sticky {sticky}"
            );
        }
    }

    #[test]
    fn next_mode_is_always_mesh_without_the_sfu() {
        for (current, headcount, sticky) in [(Mesh, 50, false), (Sfu, 8, true)] {
            assert_eq!(next_mode(current, headcount, sticky, None), Mesh);
        }
    }
}
