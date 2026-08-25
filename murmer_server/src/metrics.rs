//! Live counters an operator can watch to see a server degrading before its
//! users report it: open connections, frames taken in, how long the database
//! is taking and how many requests the rate limits turned away.
//!
//! Everything here is a process-global atomic, never a row. These numbers
//! describe *this run* — they are meaningless after a restart, and a metric
//! that cost a database write per frame would be part of the load it claims
//! to measure. They are written on every frame and every query and read once
//! per dashboard poll, so every ordering is `Relaxed`: this is a gauge, not
//! a synchronisation point, and a count that is one behind for a microsecond
//! is a count that is right.
//!
//! The counters are **cumulative, not rates**. A rate needs a window, a
//! window needs a sampling task, and two dashboards polling at different
//! intervals would each want a different one. Cumulative counters plus the
//! uptime let every viewer subtract two of its own samples and get the rate
//! over exactly the interval it was watching.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

/// The instant every counter below is measured from; see [`mark_start`].
static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);

/// WebSocket connections open right now.
static CONNECTIONS: AtomicI64 = AtomicI64::new(0);
/// The most that were ever open at once during this run.
static PEAK_CONNECTIONS: AtomicI64 = AtomicI64::new(0);
/// Frames accepted from clients, valid JSON or not — this is load, not work.
static FRAMES: AtomicU64 = AtomicU64::new(0);
static DB_CALLS: AtomicU64 = AtomicU64::new(0);
static DB_NANOS: AtomicU64 = AtomicU64::new(0);
static DB_MAX_NANOS: AtomicU64 = AtomicU64::new(0);
/// Requests refused by a rate limit, indexed by [`Limit`].
static REJECTED: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// Fix the instant uptime is measured from. Call once, at startup.
///
/// A cumulative counter only means something next to the interval it
/// accumulated over. Left to initialise itself lazily, that interval would
/// begin at whatever first happened to touch a counter — which, for a server
/// nobody has connected to yet, is the dashboard poll asking for the numbers.
pub fn mark_start() {
    LazyLock::force(&STARTED);
}

/// One live connection, counted for exactly as long as the guard is held.
///
/// A guard rather than a decrement at the end of the socket loop: a task that
/// panics or is cancelled never reaches that end, and a connection count that
/// only ever climbs is worse than none at all — it is the number an operator
/// reads to decide the server is in trouble.
pub struct Connection;

impl Connection {
    /// Count a connection as open until the returned guard is dropped.
    pub fn open() -> Self {
        let live = CONNECTIONS.fetch_add(1, Relaxed) + 1;
        PEAK_CONNECTIONS.fetch_max(live, Relaxed);
        Self
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        CONNECTIONS.fetch_sub(1, Relaxed);
    }
}

/// Count one frame received from a client.
pub fn frame_received() {
    FRAMES.fetch_add(1, Relaxed);
}

/// Record how long one database call took, from the caller's point of view.
///
/// That includes the wait for the single connection thread, which is the
/// point: every query in the process queues behind every other one, so the
/// wait *is* the degradation signal. A number that timed only the statement
/// would stay flat while the queue in front of it grew without bound.
pub fn db_call(elapsed: Duration) {
    let nanos = elapsed.as_nanos() as u64;
    DB_CALLS.fetch_add(1, Relaxed);
    DB_NANOS.fetch_add(nanos, Relaxed);
    DB_MAX_NANOS.fetch_max(nanos, Relaxed);
}

/// Which limit turned a request away.
#[derive(Clone, Copy)]
pub enum Limit {
    /// Chat messages from one account.
    Messages,
    /// Authentication attempts from one address.
    Auth,
    /// Uploads from one address.
    Uploads,
    /// A signature presented twice — its nonce was already spent.
    Replays,
}

/// Count one request refused by a rate limit.
pub fn rejected(limit: Limit) {
    REJECTED[limit as usize].fetch_add(1, Relaxed);
}

/// Every counter, read in one pass for the `server-metrics` frame.
///
/// Durations are milliseconds because that is the unit the numbers are read
/// in; the counters keep nanoseconds so a run of sub-millisecond queries does
/// not sum to zero.
pub struct Snapshot {
    pub uptime_secs: u64,
    pub connections: i64,
    pub peak_connections: i64,
    pub frames: u64,
    pub db_calls: u64,
    pub db_total_ms: f64,
    pub db_max_ms: f64,
    pub rejected_messages: u64,
    pub rejected_auth: u64,
    pub rejected_uploads: u64,
    pub rejected_replays: u64,
}

/// Take a snapshot of every counter.
pub fn snapshot() -> Snapshot {
    let ms = |nanos: u64| nanos as f64 / 1_000_000.0;
    Snapshot {
        uptime_secs: STARTED.elapsed().as_secs(),
        connections: CONNECTIONS.load(Relaxed),
        peak_connections: PEAK_CONNECTIONS.load(Relaxed),
        frames: FRAMES.load(Relaxed),
        db_calls: DB_CALLS.load(Relaxed),
        db_total_ms: ms(DB_NANOS.load(Relaxed)),
        db_max_ms: ms(DB_MAX_NANOS.load(Relaxed)),
        rejected_messages: REJECTED[Limit::Messages as usize].load(Relaxed),
        rejected_auth: REJECTED[Limit::Auth as usize].load(Relaxed),
        rejected_uploads: REJECTED[Limit::Uploads as usize].load(Relaxed),
        rejected_replays: REJECTED[Limit::Replays as usize].load(Relaxed),
    }
}
