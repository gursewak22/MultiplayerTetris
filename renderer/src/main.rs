use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{accept_async, connect_async, MaybeTlsStream};
use tokio_tungstenite::tungstenite::Message;

use shared::DrawFrame;

// ---------------------------------------------------------------------------
// Shared state: connected browser clients and latest DrawFrame channel
// ---------------------------------------------------------------------------

type ClientMap = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<DrawFrame>>>>;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

const SERVER_RENDER_URL: &str = "ws://127.0.0.1:9002";
const CLIENT_FRAME_PORT: &str = "0.0.0.0:9003";
const HTTP_PORT: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() {
    // :9003 — push DrawFrames to browser clients
    let client_listener = TcpListener::bind(CLIENT_FRAME_PORT)
        .await
        .expect("Failed to bind :9003");

    println!("Renderer: serving frames to browsers on  :9003");
    println!("Renderer: serving game UI on              http://localhost:8080");
    println!("Renderer: connecting to server at         {SERVER_RENDER_URL}...");

    let clients: ClientMap = Arc::new(Mutex::new(HashMap::new()));

    // Serve static index.html over HTTP on :8080.
    tokio::spawn(serve_http());

    // Accept browser WebSocket connections on :9003.
    {
        let clients = clients.clone();
        tokio::spawn(accept_client_connections(client_listener, clients));
    }

    // Connect to the server's renderer endpoint and receive DrawFrames.
    // Retry indefinitely until the server is available.
    loop {
        match connect_async(SERVER_RENDER_URL).await {
            Ok((ws, _)) => {
                println!("Renderer connected to server.");
                handle_server_stream(ws, clients.clone()).await;
                eprintln!("Renderer: server disconnected, retrying in 2s...");
            }
            Err(e) => {
                eprintln!("Renderer: cannot reach server ({e}), retrying in 2s...");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

// ---------------------------------------------------------------------------
// HTTP server — serves index.html for any GET request
// ---------------------------------------------------------------------------

async fn serve_http() {
    let listener = TcpListener::bind(HTTP_PORT)
        .await
        .expect("Failed to bind HTTP :8080");

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(handle_http(stream));
        }
    }
}

async fn handle_http(mut stream: TcpStream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Drain the request headers (we always serve the same page).
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf).await;

    let html = include_str!("../index.html");
    let body = html.as_bytes();
    let header = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(body).await;
}

// ---------------------------------------------------------------------------
// Accept incoming browser WebSocket clients on :9003
// ---------------------------------------------------------------------------

async fn accept_client_connections(listener: TcpListener, clients: ClientMap) {
    let mut next_id: u64 = 0;
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let clients = clients.clone();
                let id = next_id;
                next_id += 1;
                tokio::spawn(handle_client_connection(stream, addr, id, clients));
            }
            Err(e) => {
                eprintln!("Client accept error on :9003: {e}");
            }
        }
    }
}

async fn handle_client_connection(
    stream: TcpStream,
    addr: SocketAddr,
    id: u64,
    clients: ClientMap,
) {
    let ws = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("Browser WS handshake from {addr} failed: {e}");
            return;
        }
    };

    println!("Browser client {id} connected from {addr}");

    let (mut ws_sink, mut ws_stream) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<DrawFrame>();

    // Register client.
    {
        let mut map = clients.lock().await;
        map.insert(id, tx);
    }

    // Forward DrawFrames to the browser WebSocket.
    let forward = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let json = match serde_json::to_string(&frame) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Serialize DrawFrame error: {e}");
                    continue;
                }
            };
            if ws_sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Drain incoming messages (browsers don't send anything here).
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(_) => {}
            Err(_) => break,
        }
    }

    forward.abort();

    // Unregister.
    {
        let mut map = clients.lock().await;
        map.remove(&id);
    }
    println!("Browser client {id} ({addr}) disconnected");
}

// ---------------------------------------------------------------------------
// Receive DrawFrames from the server and broadcast to all browser clients
// ---------------------------------------------------------------------------

async fn handle_server_stream(
    ws: tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    clients: ClientMap,
) {
    let (_ws_sink, mut ws_stream) = ws.split();

    while let Some(msg) = ws_stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Server stream error: {e}");
                break;
            }
        };

        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        let frame: DrawFrame = match serde_json::from_str(&text) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Parse DrawFrame error: {e}");
                continue;
            }
        };

        // Broadcast raw DrawFrame JSON to all connected browser clients.
        let map = clients.lock().await;
        for tx in map.values() {
            let _ = tx.send(frame.clone());
        }
    }

    println!("Server disconnected");
}
