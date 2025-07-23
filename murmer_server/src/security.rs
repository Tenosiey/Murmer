//! Security utilities for rate limiting and replay attack prevention.

use std::time::{Duration, Instant};
use tracing::warn;
use crate::RateLimiter;

/// Get rate limiting configuration from environment variables with defaults
pub fn get_max_messages_per_minute() -> usize {
    std::env::var("MAX_MESSAGES_PER_MINUTE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
}

pub fn get_max_auth_attempts_per_minute() -> usize {
    std::env::var("MAX_AUTH_ATTEMPTS_PER_MINUTE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
}

pub fn get_nonce_expiry_seconds() -> u64 {
    std::env::var("NONCE_EXPIRY_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300) // 5 minutes
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
pub async fn check_auth_rate_limit(
    rate_limiter: &RateLimiter, 
    ip: &str
) -> bool {
    let now = Instant::now();
    let mut attempts = rate_limiter.auth_attempts.lock().await;
    
    // Clean up old entries outside the rate limiting window
    let cutoff = now - Duration::from_secs(60);
    if let Some(timestamps) = attempts.get_mut(ip) {
        while let Some(&front) = timestamps.front() {
            if front < cutoff {
                timestamps.pop_front();
            } else {
                break;
            }
        }
    }
    
    // Check if this IP has exceeded the rate limit
    let current_attempts = attempts.get(ip).map_or(0, |v| v.len());
    if current_attempts >= get_max_auth_attempts_per_minute() {
        warn!("Rate limit exceeded for auth attempts from IP: {}", ip);
        return false;
    }
    
    // Record this attempt
    attempts.entry(ip.to_string()).or_default().push_back(now);
    true
}

/// Check if a user is rate limited for messages
pub async fn check_message_rate_limit(
    rate_limiter: &RateLimiter,
    user: &str
) -> bool {
    let now = Instant::now();
    let mut message_times = rate_limiter.message_times.lock().await;
    
    // Clean up old entries
    let cutoff = now - Duration::from_secs(60);
    if let Some(timestamps) = message_times.get_mut(user) {
        while let Some(&front) = timestamps.front() {
            if front < cutoff {
                timestamps.pop_front();
            } else {
                break;
            }
        }
    }
    
    // Check current message count
    let current_messages = message_times.get(user).map_or(0, |v| v.len());
    if current_messages >= get_max_messages_per_minute() {
        warn!("Rate limit exceeded for messages from user: {}", user);
        return false;
    }
    
    // Add current message
    message_times.entry(user.to_string()).or_default().push_back(now);
    true
}

/// Check if a nonce has been used (replay attack prevention)
pub async fn check_and_store_nonce(
    rate_limiter: &RateLimiter,
    nonce: &str
) -> bool {
    let now = Instant::now();
    let mut used_nonces = rate_limiter.used_nonces.lock().await;
    
    // Clean up expired nonces
    let expiry_cutoff = now - Duration::from_secs(get_nonce_expiry_seconds());
    used_nonces.retain(|_, &mut expiry_time| expiry_time > expiry_cutoff);
    
    // Check if nonce is already used
    if used_nonces.contains_key(nonce) {
        warn!("Replay attack detected - nonce already used: {}", nonce);
        return false;
    }
    
    // Store the nonce with expiry time
    used_nonces.insert(nonce.to_string(), now);
    true
}

/// Validate timestamp is within acceptable range
pub fn validate_timestamp(timestamp_str: &str) -> Result<i64, &'static str> {
    let timestamp = timestamp_str.parse::<i64>()
        .map_err(|_| "Invalid timestamp format")?;
    
    let now = chrono::Utc::now().timestamp_millis();
    let diff = (now - timestamp).abs();
    
    // Allow 60 second window
    if diff > 60_000 {
        return Err("Timestamp outside acceptable window");
    }
    
    // Prevent timestamps too far in the future (1 hour)
    if timestamp > now + 3_600_000 {
        return Err("Timestamp too far in future");
    }
    
    // Prevent very old timestamps (24 hours)
    if timestamp < now - 86_400_000 {
        return Err("Timestamp too old");
    }
    
    Ok(timestamp)
}

/// Validate channel name for security
pub fn validate_channel_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 50 {
        return false;
    }
    
    // Allow alphanumeric, dash, underscore, and spaces
    // Reject names that are only whitespace or have leading/trailing spaces
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed != name {
        return false;
    }
    
    name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
}

/// Validate user name for security  
pub fn validate_user_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 32 {
        return false;
    }
    
    // Allow alphanumeric, dash, underscore, and spaces
    // Reject names that are only whitespace or have leading/trailing spaces
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed != name {
        return false;
    }
    
    name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
}