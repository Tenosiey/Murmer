use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use std::{env, net::SocketAddr, sync::Arc};
use tokio::sync::broadcast;
use tokio_postgres::{Client, NoTls};
use tracing_subscriber;
use tokio::net::TcpListener;

struct AppState {
    tx: broadcast::Sender<String>,
    db: Arc<Client>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let (tx, _rx) = broadcast::channel::<String>(100);

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let (db_client, connection) = tokio_postgres::connect(&db_url, NoTls).await.expect("connect db");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("db connection error: {e}");
        }
    });
    db_client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS messages (id SERIAL PRIMARY KEY, content TEXT NOT NULL)",
        )
        .await
        .unwrap();
    let state = Arc::new(AppState { tx: tx.clone(), db: Arc::new(db_client) });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    println!("WebSocket server running on {addr}");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .await
        .unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    // send previous messages from DB
    if let Ok(rows) = state
        .db
        .query("SELECT content FROM messages ORDER BY id", &[])
        .await
    {
        for row in rows {
            let content: String = row.get(0);
            if sender.send(Message::Text(content)).await.is_err() {
                return;
            }
        }
    }

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(Message::Text(text))) = receiver.next().await {
        let _ = state
            .db
            .execute("INSERT INTO messages (content) VALUES ($1)", &[&text])
            .await;
        let _ = state.tx.send(text);
    }

    send_task.abort();
}
