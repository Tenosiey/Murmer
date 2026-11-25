//! `/health` answers 200 while the database thread serves queries and 503
//! once it cannot — the distinction an open port alone does not show.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use murmer_server::{AppState, db, health};
use tower::ServiceExt;

async fn get_health(state: Arc<AppState>) -> StatusCode {
    Router::new()
        .route("/health", get(health::health))
        .with_state(state)
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn healthy_database_answers_ok() {
    let state = Arc::new(AppState::new(db::init(":memory:").await.unwrap()));
    assert_eq!(get_health(state).await, StatusCode::OK);
}

#[tokio::test]
async fn closed_database_answers_unavailable() {
    let database = db::init(":memory:").await.unwrap();
    let state = Arc::new(AppState::new(database.clone()));
    database.close().await.unwrap();
    assert_eq!(get_health(state).await, StatusCode::SERVICE_UNAVAILABLE);
}
