use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::sync::broadcast;
use tracing_subscriber;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let (tx, _rx) = broadcast::channel::<String>(100);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(tx);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    println!("WebSocket server running on {addr}");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .await
        .unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, State(tx): State<broadcast::Sender<String>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, tx))
}

async fn handle_socket(socket: WebSocket, tx: broadcast::Sender<String>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = tx.subscribe();

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(Message::Text(text))) = receiver.next().await {
        let _ = tx.send(text);
    }

    send_task.abort();
}
