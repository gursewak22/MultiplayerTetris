mod game;
mod lobby;
mod session;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use lobby::Lobby;
use session::GameSession;
use shared::{ClientMsg, DrawFrame, ServerMsg};

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

type LobbyRef = Arc<Mutex<Lobby>>;
/// All active game sessions indexed by an incrementing id.
type Sessions = Arc<Mutex<HashMap<u64, GameSession>>>;
/// Channel for pushing DrawFrames to the renderer task.
type RendererTx = Arc<Mutex<Option<mpsc::UnboundedSender<shared::RenderMsg>>>>;
/// Maps player name → (session_id, player_number) once a game has been assigned.
type GameAssignments = Arc<Mutex<HashMap<String, (u64, u8)>>>;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let lobby: LobbyRef = Arc::new(Mutex::new(Lobby::new()));
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let renderer_tx: RendererTx = Arc::new(Mutex::new(None));
    let game_assignments: GameAssignments = Arc::new(Mutex::new(HashMap::new()));

    let client_listener = TcpListener::bind("127.0.0.1:9001")
        .await
        .expect("Failed to bind :9001");
    let renderer_listener = TcpListener::bind("127.0.0.1:9002")
        .await
        .expect("Failed to bind :9002");

    println!("Server listening — clients on :9001, renderer on :9002");

    // Spawn the tick loop.
    {
        let sessions = sessions.clone();
        let lobby = lobby.clone();
        let renderer_tx = renderer_tx.clone();
        let game_assignments_clone = game_assignments.clone();
        tokio::spawn(tick_loop(sessions, lobby, renderer_tx, game_assignments_clone));
    }

    // Accept renderer connections on :9002.
    {
        let renderer_tx = renderer_tx.clone();
        tokio::spawn(accept_renderer_connections(renderer_listener, renderer_tx));
    }

    // Accept client connections on :9001.
    loop {
        match client_listener.accept().await {
            Ok((stream, addr)) => {
                let lobby = lobby.clone();
                let sessions = sessions.clone();
                let game_assignments = game_assignments.clone();
                tokio::spawn(handle_client(stream, addr, lobby, sessions, game_assignments));
            }
            Err(e) => {
                eprintln!("Accept error on :9001: {e}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Game tick loop (~60 fps)
// ---------------------------------------------------------------------------

async fn tick_loop(
    sessions: Sessions,
    lobby: LobbyRef,
    renderer_tx: RendererTx,
    game_assignments: GameAssignments,
) {
    let tick_duration = Duration::from_millis(16);
    let mut last = Instant::now();

    loop {
        tokio::time::sleep(tick_duration).await;
        let elapsed = last.elapsed();
        last = Instant::now();
        let elapsed_ms = elapsed.as_millis() as u64;

        let mut frames = Vec::new();
        {
            let mut sess_map = sessions.lock().await;
            for (&id, session) in sess_map.iter_mut() {
                session.tick(elapsed_ms);
                frames.push((id, session.to_draw_frame()));
            }
        }

        // Check for game-over events and update lobby rankings.
        {
            let mut sess_map = sessions.lock().await;
            let mut finished = Vec::new();
            for (&id, session) in sess_map.iter() {
                if let Some(loser_num) = session.game_over {
                    finished.push((id, loser_num));
                }
            }
            if !finished.is_empty() {
                let mut assignments = game_assignments.lock().await;
                let mut lob = lobby.lock().await;
                for (id, loser_num) in finished {
                    sess_map.remove(&id);
                    
                    let mut p1_name = None;
                    let mut p2_name = None;
                    for (name, &(sid, pnum)) in assignments.iter() {
                        if sid == id {
                            if pnum == 1 { p1_name = Some(name.clone()); }
                            if pnum == 2 { p2_name = Some(name.clone()); }
                        }
                    }
                    
                    if let (Some(p1), Some(p2)) = (p1_name, p2_name) {
                        let winner = if loser_num == 1 { &p2 } else { &p1 };
                        let loser = if loser_num == 1 { &p1 } else { &p2 };
                        lob.mark_available(winner, loser);
                        let (round_complete, tourney_finished) = lob.record_game_result(winner, loser);
                        
                        if round_complete {
                            start_tournament_round_timer(sessions.clone(), game_assignments.clone(), lobby.clone());
                        }
                        
                        if tourney_finished {
                            let lob_ref = lobby.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_secs(10)).await;
                                let mut l = lob_ref.lock().await;
                                l.tournament = None;
                                l.broadcast_tournament_state();
                            });
                        }
                    }
                    
                    assignments.retain(|_, v| v.0 != id);
                }
                lob.broadcast_lobby_state();
            }
        }

        // Push the latest frames to the renderer if connected.
        if !frames.is_empty() {
            let tx_guard = renderer_tx.lock().await;
            if let Some(tx) = tx_guard.as_ref() {
                let _ = tx.send(shared::RenderMsg::Frames { frames });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Renderer connection handler
// ---------------------------------------------------------------------------

async fn accept_renderer_connections(listener: TcpListener, renderer_tx: RendererTx) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let renderer_tx = renderer_tx.clone();
                tokio::spawn(handle_renderer_push(stream, addr, renderer_tx));
            }
            Err(e) => {
                eprintln!("Renderer accept error: {e}");
            }
        }
    }
}

/// Accepts a WebSocket from the renderer process. The server pushes DrawFrames
/// over this connection; it does not read from it.
async fn handle_renderer_push(stream: TcpStream, addr: SocketAddr, renderer_tx: RendererTx) {
    let ws = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("Renderer WS handshake from {addr} failed: {e}");
            return;
        }
    };

    println!("Renderer connected from {addr}");

    let (mut ws_sink, mut ws_stream) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<shared::RenderMsg>();

    // Register this as the active renderer sender.
    {
        let mut guard = renderer_tx.lock().await;
        *guard = Some(tx);
    }

    // Forward frames from the channel to the WebSocket.
    let forward_task = tokio::spawn(async move {
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

    // Drain any messages sent by the renderer (we don't expect any, but we
    // must drive the stream to detect disconnection).
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(_) => {}
            Err(_) => break,
        }
    }

    forward_task.abort();
    println!("Renderer {addr} disconnected");

    // Clear the renderer sender so frames are discarded until reconnection.
    let mut guard = renderer_tx.lock().await;
    *guard = None;
}

// ---------------------------------------------------------------------------
// Tournament Helper
// ---------------------------------------------------------------------------
fn start_tournament_round_timer(
    sessions: Sessions,
    game_assignments: GameAssignments,
    lobby: LobbyRef,
) {
    tokio::spawn(async move {
        // Wait 3 seconds for players to see the bracket UI
        tokio::time::sleep(Duration::from_secs(3)).await;

        let lob = lobby.lock().await;
        let mut sess_map = sessions.lock().await;
        let mut assignments = game_assignments.lock().await;

        if let Some(ref bracket) = lob.tournament {
            for (p1, p2) in &bracket.matches {
                let new_session = session::GameSession::new();
                let mut id = sess_map.len() as u64;
                while sess_map.contains_key(&id) { id += 1; }
                
                sess_map.insert(id, new_session);
                assignments.insert(p1.clone(), (id, 1));
                assignments.insert(p2.clone(), (id, 2));

                lob.send_to(p1, shared::ServerMsg::GameStart { session_id: id });
                lob.send_to(p2, shared::ServerMsg::GameStart { session_id: id });
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Client connection handler
// ---------------------------------------------------------------------------

async fn handle_client(
    stream: TcpStream,
    addr: SocketAddr,
    lobby: LobbyRef,
    sessions: Sessions,
    game_assignments: GameAssignments,
) {
    let ws = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("Client WS handshake from {addr} failed: {e}");
            return;
        }
    };

    println!("Client connected from {addr}");

    let (mut ws_sink, mut ws_stream) = ws.split();
    let (server_tx, mut server_rx) = mpsc::unbounded_channel::<ServerMsg>();

    let mut player_name: Option<String> = None;
    let mut player_number: u8 = 0;
    let mut session_id: Option<u64> = None;

    // Task: forward ServerMsg → WebSocket text frames.
    let send_task = tokio::spawn(async move {
        while let Some(msg) = server_rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Serialize ServerMsg error: {e}");
                    continue;
                }
            };
            if ws_sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(raw) = ws_stream.next().await {
        let raw = match raw {
            Ok(r) => r,
            Err(e) => {
                eprintln!("WS recv error from {addr}: {e}");
                break;
            }
        };

        let text = match raw {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        let client_msg: ClientMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Parse ClientMsg from {addr}: {e}");
                continue;
            }
        };

        match client_msg {
            ClientMsg::JoinLobby { name } => {
                player_name = Some(name.clone());
                let mut lob = lobby.lock().await;
                lob.add_player(name, server_tx.clone());
            }

            ClientMsg::Challenge { target } => {
                if let Some(ref name) = player_name {
                    let mut lob = lobby.lock().await;
                    lob.handle_challenge(name, &target);
                }
            }

            ClientMsg::AcceptChallenge { from } => {
                if let Some(ref name) = player_name {
                    let mut lob = lobby.lock().await;

                    // Block if either player is already in a match.
                    if lob.is_in_game(name) || lob.is_in_game(&from) {
                        lob.send_to(name, ServerMsg::ServerError {
                            msg: "One of the players is already in a match.".to_string(),
                        });
                        lob.send_to(&from, ServerMsg::ServerError {
                            msg: format!("{name} is already in a match."),
                        });
                    } else {
                        let accepted = lob.accept_challenge(name, &from);
                        if accepted {
                            // Clear all other pending challenges involving either player.
                            lob.challenges.retain(|c| {
                                c.from != *name && c.to != *name
                                    && c.from != from && c.to != from
                            });
                            // Mark both as in-game immediately.
                            lob.mark_in_game(name, &from);
                            lob.broadcast_lobby_state();

                            // Create a new game session.
                            let new_session = GameSession::new();
                            let id = {
                                let mut sess_map = sessions.lock().await;
                                let mut id = sess_map.len() as u64;
                                while sess_map.contains_key(&id) { id += 1; }
                                sess_map.insert(id, new_session);
                                id
                            };
                            // Acceptor is player 1, challenger is player 2.
                            session_id = Some(id);
                            player_number = 1;
                            // Register both players so the challenger's task can look up its assignment.
                            let mut assignments = game_assignments.lock().await;
                            assignments.insert(name.clone(), (id, 1));
                            assignments.insert(from.clone(), (id, 2));

                            // Notify both players that the game is starting.
                            lob.send_to(name, ServerMsg::GameStart { session_id: id });
                            lob.send_to(&from, ServerMsg::GameStart { session_id: id });
                        }
                    }
                }
            }

            ClientMsg::DeclineChallenge { from } => {
                if let Some(ref name) = player_name {
                    let mut lob = lobby.lock().await;
                    lob.decline_challenge(name, &from);
                }
            }

            ClientMsg::JoinTournament => {
                if let Some(ref name) = player_name {
                    let mut lob = lobby.lock().await;
                    let _ = lob.join_tournament(name);
                }
            }

            ClientMsg::LeaveTournament => {
                if let Some(ref name) = player_name {
                    let mut lob = lobby.lock().await;
                    lob.leave_tournament(name);
                }
            }

            ClientMsg::StartTournament => {
                let started = {
                    let mut lob = lobby.lock().await;
                    lob.start_tournament().is_ok()
                };
                if started {
                    start_tournament_round_timer(sessions.clone(), game_assignments.clone(), lobby.clone());
                }
            }

            ClientMsg::Spectate { target } => {
                let assignments = game_assignments.lock().await;
                if let Some(&(sid, _)) = assignments.get(target.as_str()) {
                    let _ = server_tx.send(ServerMsg::SpectateInfo { session_id: sid });
                } else {
                    let _ = server_tx.send(ServerMsg::ServerError {
                        msg: format!("Player '{target}' is not in an active game."),
                    });
                }
            }

            ClientMsg::Input { action } => {
                if let Some(ref name) = player_name {
                    let assignments = game_assignments.lock().await;
                    if let Some(&(sid, pnum)) = assignments.get(name.as_str()) {
                        let mut sess_map = sessions.lock().await;
                        if let Some(session) = sess_map.get_mut(&sid) {
                            session.apply_input(pnum, action);
                        }
                    }
                }
            }
        }
    }

    // Client disconnected — clean up.
    if let Some(ref name) = player_name {
        let mut lob = lobby.lock().await;
        lob.remove_player(name);
    }
    send_task.abort();
    println!("Client {addr} disconnected");
}
