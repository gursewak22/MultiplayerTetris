use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{accept_async, connect_async, MaybeTlsStream};
use tokio_tungstenite::tungstenite::Message;

use shared::{DrawFrame, FrameOutput, PieceState, PieceType};

// ---------------------------------------------------------------------------
// Shared state: connected frame-request clients and latest rendered frame
// ---------------------------------------------------------------------------

type ClientMap = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<FrameOutput>>>>;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

const SERVER_RENDER_URL: &str = "ws://127.0.0.1:9002";
const CLIENT_FRAME_PORT: &str = "0.0.0.0:9003";

#[tokio::main]
async fn main() {
    // :9003 — serve FrameOutputs to game clients
    let client_listener = TcpListener::bind(CLIENT_FRAME_PORT)
        .await
        .expect("Failed to bind :9003");

    println!("Renderer: serving frames to clients on :9003");
    println!("Renderer: connecting to server at {SERVER_RENDER_URL}...");

    let clients: ClientMap = Arc::new(Mutex::new(HashMap::new()));

    // Accept game-client connections on :9003.
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
// Accept incoming game clients on :9002
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
                eprintln!("Client accept error on :9002: {e}");
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
            eprintln!("Client WS handshake from {addr} failed: {e}");
            return;
        }
    };

    println!("Game client {id} connected from {addr}");

    let (mut ws_sink, mut ws_stream) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<FrameOutput>();

    // Register client.
    {
        let mut map = clients.lock().await;
        map.insert(id, tx);
    }

    // Forward FrameOutput to the client WebSocket.
    let forward = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let json = match serde_json::to_string(&frame) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Serialize FrameOutput error: {e}");
                    continue;
                }
            };
            if ws_sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Drain incoming messages (clients don't send anything to the renderer).
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
    println!("Game client {id} ({addr}) disconnected");
}

// ---------------------------------------------------------------------------
// Receive DrawFrames from the server and broadcast to clients
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

        let output = render_frame(&frame);

        // Broadcast to all connected game clients.
        let map = clients.lock().await;
        for tx in map.values() {
            let _ = tx.send(output.clone());
        }
    }

    println!("Server disconnected");
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// ANSI colour escape codes for each piece colour id (1–7).
fn piece_color_ansi(id: u8) -> &'static str {
    match id {
        1 => "\x1b[96m", // I — cyan
        2 => "\x1b[93m", // O — yellow
        3 => "\x1b[95m", // T — magenta
        4 => "\x1b[92m", // S — green
        5 => "\x1b[91m", // Z — red
        6 => "\x1b[94m", // J — blue
        7 => "\x1b[33m", // L — dark yellow / orange
        _ => "\x1b[37m",
    }
}

const RESET: &str = "\x1b[0m";
const FILLED: &str = "██";
const GHOST: &str = "░░";
const EMPTY: &str = "··";

/// Render a single board (20×10) as a Vec of line strings.
fn render_board(
    board: &[[u8; 10]; 20],
    piece: Option<&PieceState>,
    label: &str,
    score: u32,
    lines: u32,
    hold: Option<PieceType>,
    next: PieceType,
    swap_cooldown: f32,
) -> Vec<String> {
    // Compose a layer with the active piece and ghost.
    let mut overlay: [[u8; 10]; 20] = [[0u8; 10]; 20];
    let mut ghost_overlay: [[bool; 10]; 20] = [[false; 10]; 20];

    if let Some(ps) = piece {
        // Compute ghost Y.
        let mut ghost_y = ps.y;
        loop {
            if !is_valid_position(board, ps.piece_type, ps.x, ghost_y + 1, ps.rotation) {
                break;
            }
            ghost_y += 1;
        }
        // Draw ghost.
        let cells = piece_cells(ps.piece_type, ps.rotation);
        for (dx, dy) in cells {
            let gx = ps.x + dx;
            let gy = ghost_y + dy;
            if gx >= 0 && gx < 10 && gy >= 0 && gy < 20 {
                ghost_overlay[gy as usize][gx as usize] = true;
            }
        }
        // Draw active piece.
        let color_id = piece_type_color_id(ps.piece_type);
        for (dx, dy) in cells {
            let cx = ps.x + dx;
            let cy = ps.y + dy;
            if cx >= 0 && cx < 10 && cy >= 0 && cy < 20 {
                overlay[cy as usize][cx as usize] = color_id;
            }
        }
    }

    let mut result: Vec<String> = Vec::new();

    // Header.
    result.push(format!("  {label}"));
    result.push(format!("  Score: {score:<8}  Lines: {lines}"));
    result.push(format!("  ┌────────────────────┐"));

    for row in 0..20usize {
        let mut line = String::from("  │");
        for col in 0..10usize {
            if overlay[row][col] != 0 {
                let c = piece_color_ansi(overlay[row][col]);
                line.push_str(&format!("{c}{FILLED}{RESET}"));
            } else if board[row][col] != 0 {
                let c = piece_color_ansi(board[row][col]);
                line.push_str(&format!("{c}{FILLED}{RESET}"));
            } else if ghost_overlay[row][col] {
                line.push_str(&format!("\x1b[90m{GHOST}{RESET}"));
            } else {
                line.push_str(EMPTY);
            }
        }
        line.push('│');
        result.push(line);
    }

    result.push(format!("  └────────────────────┘"));

    // Hold piece display.
    let hold_str = match hold {
        Some(p) => format!("{:?}", p),
        None => "None".to_string(),
    };
    let swap_pct = (swap_cooldown * 100.0) as u32;
    let swap_str = if swap_cooldown > 0.0 {
        format!("cooldown {swap_pct}%")
    } else {
        "ready".to_string()
    };
    result.push(format!("  Hold: {hold_str:<4}  Swap: {swap_str}"));

    // Next piece display.
    let next_str = format!("{:?}", next);
    result.push(format!("  Next: {next_str}"));
    result.push(String::new());

    result
}

/// Count the visible (printable) columns in a string, ignoring ANSI escape sequences.
fn visual_width(s: &str) -> usize {
    let mut width = 0usize;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume the escape sequence up to and including the final byte (letter).
            for c2 in chars.by_ref() {
                if c2.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            // Every char counts as 1 column (block/dot chars used here are all 1-wide).
            width += 1;
        }
    }
    width
}

/// Pad `s` with trailing spaces so its visual width reaches `target`.
fn pad_visual(s: &str, target: usize) -> String {
    let vw = visual_width(s);
    if vw >= target {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(target - vw))
    }
}

fn render_frame(frame: &DrawFrame) -> FrameOutput {
    let left = render_board(
        &frame.board_p1,
        frame.piece_p1.as_ref(),
        "Player 1",
        frame.score_p1,
        frame.lines_p1,
        frame.hold_p1,
        frame.next_p1,
        frame.swap_cooldown_p1,
    );

    let right = render_board(
        &frame.board_p2,
        frame.piece_p2.as_ref(),
        "Player 2",
        frame.score_p2,
        frame.lines_p2,
        frame.hold_p2,
        frame.next_p2,
        frame.swap_cooldown_p2,
    );

    let max_rows = left.len().max(right.len());

    let mut lines: Vec<String> = Vec::new();

    // Move cursor to top-left without clearing (avoids flicker).
    lines.push("\x1b[H".to_string());

    // Left column visual width: 2 indent + 1 border + 10*2 cells + 1 border = 24.
    // Pad to 28 so there is a 4-space gap between boards.
    const LEFT_COL_WIDTH: usize = 28;

    for i in 0..max_rows {
        let l = left.get(i).map(String::as_str).unwrap_or("");
        let r = right.get(i).map(String::as_str).unwrap_or("");
        // Pad left column using visual width (strip ANSI before measuring).
        let padded_l = pad_visual(l, LEFT_COL_WIDTH);
        // Erase to end of line after each row to clear leftover characters.
        lines.push(format!("{}{}\x1b[K", padded_l, r));
    }
    // Erase everything below the last row.
    lines.push("\x1b[J".to_string());

    if let Some(loser) = frame.game_over {
        let winner = if loser == 1 { 2 } else { 1 };
        lines.push(String::new());
        lines.push(format!(
            "  \x1b[1;93m*** GAME OVER — Player {winner} wins! ***\x1b[0m"
        ));
    }

    FrameOutput { lines }
}

// ---------------------------------------------------------------------------
// Helpers mirroring game.rs (renderer does not depend on server crate)
// ---------------------------------------------------------------------------

fn piece_cells(piece: PieceType, rotation: u8) -> [(i32, i32); 4] {
    let rot = (rotation % 4) as usize;
    match piece {
        PieceType::I => [
            [(0, 1), (1, 1), (2, 1), (3, 1)],
            [(2, 0), (2, 1), (2, 2), (2, 3)],
            [(0, 2), (1, 2), (2, 2), (3, 2)],
            [(1, 0), (1, 1), (1, 2), (1, 3)],
        ][rot],
        PieceType::O => [
            [(1, 0), (2, 0), (1, 1), (2, 1)],
            [(1, 0), (2, 0), (1, 1), (2, 1)],
            [(1, 0), (2, 0), (1, 1), (2, 1)],
            [(1, 0), (2, 0), (1, 1), (2, 1)],
        ][rot],
        PieceType::T => [
            [(1, 0), (0, 1), (1, 1), (2, 1)],
            [(1, 0), (1, 1), (2, 1), (1, 2)],
            [(0, 1), (1, 1), (2, 1), (1, 2)],
            [(1, 0), (0, 1), (1, 1), (1, 2)],
        ][rot],
        PieceType::S => [
            [(1, 0), (2, 0), (0, 1), (1, 1)],
            [(1, 0), (1, 1), (2, 1), (2, 2)],
            [(1, 1), (2, 1), (0, 2), (1, 2)],
            [(0, 0), (0, 1), (1, 1), (1, 2)],
        ][rot],
        PieceType::Z => [
            [(0, 0), (1, 0), (1, 1), (2, 1)],
            [(2, 0), (1, 1), (2, 1), (1, 2)],
            [(0, 1), (1, 1), (1, 2), (2, 2)],
            [(1, 0), (0, 1), (1, 1), (0, 2)],
        ][rot],
        PieceType::J => [
            [(0, 0), (0, 1), (1, 1), (2, 1)],
            [(1, 0), (2, 0), (1, 1), (1, 2)],
            [(0, 1), (1, 1), (2, 1), (2, 2)],
            [(1, 0), (1, 1), (0, 2), (1, 2)],
        ][rot],
        PieceType::L => [
            [(2, 0), (0, 1), (1, 1), (2, 1)],
            [(1, 0), (1, 1), (1, 2), (2, 2)],
            [(0, 1), (1, 1), (2, 1), (0, 2)],
            [(0, 0), (1, 0), (1, 1), (1, 2)],
        ][rot],
    }
}

fn piece_type_color_id(piece: PieceType) -> u8 {
    match piece {
        PieceType::I => 1,
        PieceType::O => 2,
        PieceType::T => 3,
        PieceType::S => 4,
        PieceType::Z => 5,
        PieceType::J => 6,
        PieceType::L => 7,
    }
}

fn is_valid_position(board: &[[u8; 10]; 20], piece: PieceType, px: i32, py: i32, rot: u8) -> bool {
    for (dx, dy) in piece_cells(piece, rot) {
        let x = px + dx;
        let y = py + dy;
        if x < 0 || x >= 10 || y >= 20 {
            return false;
        }
        if y >= 0 && board[y as usize][x as usize] != 0 {
            return false;
        }
    }
    true
}
