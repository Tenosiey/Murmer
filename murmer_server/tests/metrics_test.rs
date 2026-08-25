//! Tests for the operator metrics counters.
//!
//! Its own test binary on purpose: the counters are process-global, so a file
//! that shared a binary with anything else touching them would be asserting
//! on somebody else's arithmetic. Within this file each test owns a disjoint
//! set of counters, which is what keeps them safe to run in parallel.
//!
//! What is worth pinning is the arithmetic that is invisible until it is
//! wrong. A gauge that never comes back down reads as a server under load
//! that is in fact idle; a peak that tracks the current value instead of the
//! maximum erases the spike it exists to record; a rejection counted under
//! the wrong limit sends an operator hunting for spam when the traffic was a
//! brute-force attempt.
//!
//! The permission on `get-server-metrics` is not covered here — nothing in
//! `tests/` opens `/ws` yet (see `TODO.md`), so the dispatch loop's gate is
//! still only exercised by hand.

use murmer_server::metrics::{self, Limit};
use std::time::Duration;

#[test]
fn the_connection_gauge_comes_back_down_and_the_peak_does_not() {
    let before = metrics::snapshot();

    let first = metrics::Connection::open();
    let second = metrics::Connection::open();
    let holding = metrics::snapshot();
    assert_eq!(holding.connections, before.connections + 2);
    assert!(holding.peak_connections >= before.connections + 2);

    drop(second);
    drop(first);
    let after = metrics::snapshot();
    assert_eq!(after.connections, before.connections);
    // The spike is the whole point of the peak: it must survive the drop.
    assert_eq!(after.peak_connections, holding.peak_connections);
}

#[test]
fn database_calls_accumulate_and_keep_the_worst_one() {
    let before = metrics::snapshot();

    metrics::db_call(Duration::from_millis(2));
    metrics::db_call(Duration::from_millis(8));
    metrics::db_call(Duration::from_millis(1));

    let after = metrics::snapshot();
    assert_eq!(after.db_calls, before.db_calls + 3);
    // Nanoseconds are summed and only converted on the way out, so a run of
    // sub-millisecond queries cannot round itself away to nothing.
    assert!((after.db_total_ms - before.db_total_ms - 11.0).abs() < 1.0);
    assert!(after.db_max_ms >= 8.0);
}

#[test]
fn each_rate_limit_is_counted_under_its_own_name() {
    let before = metrics::snapshot();

    metrics::rejected(Limit::Messages);
    metrics::rejected(Limit::Messages);
    metrics::rejected(Limit::Auth);
    metrics::rejected(Limit::Replays);

    let after = metrics::snapshot();
    assert_eq!(after.rejected_messages, before.rejected_messages + 2);
    assert_eq!(after.rejected_auth, before.rejected_auth + 1);
    assert_eq!(after.rejected_replays, before.rejected_replays + 1);
    assert_eq!(after.rejected_uploads, before.rejected_uploads);
}

#[test]
fn frames_are_counted_as_they_arrive() {
    let before = metrics::snapshot();
    metrics::frame_received();
    metrics::frame_received();
    assert_eq!(metrics::snapshot().frames, before.frames + 2);
}
