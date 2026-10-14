//! `/health`, the endpoint for uptime monitors and the container health check.
//!
//! An open port only proves the HTTP listener runs. Every request the server
//! serves goes through the one SQLite connection thread, so a server whose
//! database is wedged accepts connections and answers nothing useful — this
//! endpoint round-trips a query through that thread to catch it. The answer
//! carries no data, so it needs no authentication.

use axum::{extract::State, http::StatusCode};
use std::{sync::Arc, time::Duration};
use tracing::warn;

use crate::AppState;
use crate::db::DbCall;

/// Longer than any healthy queue for the connection thread, shorter than the
/// Docker health check's own 5 s timeout so a stall reports as 503 rather
/// than as a dropped request.
const DB_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn health(State(state): State<Arc<AppState>>) -> (StatusCode, &'static str) {
    let probe = state.db.call_db(|conn| {
        conn.prepare_cached("SELECT 1")?
            .query_row([], |row| row.get::<_, i64>(0))
    });
    match tokio::time::timeout(DB_TIMEOUT, probe).await {
        Ok(Ok(_)) => (StatusCode::OK, "ok"),
        Ok(Err(e)) => {
            warn!(error = %e, "health check: database query failed");
            (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
        }
        Err(_) => {
            warn!("health check: database did not answer in time");
            (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
        }
    }
}
