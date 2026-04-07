use std::io::{self, Write};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use shared::{ClientMsg, FrameOutput, InputAction, ServerMsg};

const SERVER_URL: &str = "ws://127.0.0.1:9001";
const RENDERER_URL: &str = "ws://127.0.0.1:9003";

#[tokio::main]
async fn main() {
    // ------------------------------------------------------------------
    // Connect to server
    // ------------------------------------------------------------------
    let (server_ws, _) = match connect_async(SERVER_URL).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot connect to server at {SERVER_URL}: {e}");
            eprintln!("Make sure the server is running first.");
            std::process::exit(1);
        }
    };

    let (mut sink, mut stream) = server_ws.split();

    // ------------------------------------------------------------------
    // Ask for player name
    // ------------------------------------------------------------------
    let name = std::env::args().nth(1).unwrap_or_else(|| {
        print!("Enter your name: ");
        io::stdout().flush().ok();
        let mut n = String::new();
        io::stdin().read_line(&mut n).ok();
        n.trim().to_string()
    });

    let name = if name.is_empty() { "Player".to_string() } else { name };

    // Send JoinLobby
    let join_msg = serde_json::to_string(&ClientMsg::JoinLobby { name: name.clone() }).unwrap();
    let _ = sink.send(Message::Text(join_msg.into())).await;

    // ------------------------------------------------------------------
    // Phase 1: Lobby — normal text input
    // ------------------------------------------------------------------
    println!();
    println!("=== Multiplayer Tetris ===");
    println!("Joined lobby as '{name}'");
    println!();
    println!("Lobby commands:");
    println!("  challenge <name>   — challenge a player");
    println!("  accept <name>      — accept a challenge");
    println!("  decline <name>     — decline a challenge");
    println!("  quit               — exit");
    println!();

    // Channel to hand the sink across the phase boundary
    let (sink_tx, mut sink_rx) = mpsc::unbounded_channel::<ClientMsg>();

    // Forward outgoing messages to the server
    let send_task = tokio::spawn(async move {
        while let Some(msg) = sink_rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Channel: server sends GameStart or SpectateInfo → unblock lobby loop
    let (game_start_tx, mut game_start_rx) = mpsc::unbounded_channel::<u64>();

    // Receive server messages in background during lobby
    let game_start_tx2 = game_start_tx.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(raw) = stream.next().await {
            let text = match raw {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) | Err(_) => break,
                _ => continue,
            };
            let msg: ServerMsg = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(_) => continue,
            };
            match msg {
                ServerMsg::LobbyState { players } => {
                    println!("[Lobby] Online: {}", players.join(", "));
                }
                ServerMsg::ChallengeReceived { from } => {
                    println!("[Lobby] Challenge from '{from}'! Type: accept {from}");
                }
                ServerMsg::GameStart { session_id } => {
                    println!("[Game] Game starting! Connecting to renderer...");
                    let _ = game_start_tx2.send(session_id);
                    break;
                }
                ServerMsg::SpectateInfo { session_id } => {
                    println!("[Game] Spectating match! Connecting to renderer...");
                    let _ = game_start_tx2.send(session_id);
                    break;
                }
                ServerMsg::ServerError { msg } => {
                    println!("[Server Error] {msg}");
                }
                ServerMsg::Scoreboard { scores } => {
                    // Do not print scoreboard endlessly here unless we want to, wait, sure!
                    println!("\n--- Global Rankings ---");
                    for (i, (player, score)) in scores.iter().enumerate() {
                        println!(" {}. {player:-<15} {score}", i + 1);
                    }
                    println!("-----------------------\n");
                }
                ServerMsg::GameOver { winner } => {
                    println!("[Game] Game over. Winner: {winner}");
                }
            }
        }
    });

    // Read lobby commands from stdin
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let sink_tx2 = sink_tx.clone();

    let mut game_session: u64 = 0;

    loop {
        tokio::select! {
            session = game_start_rx.recv() => {
                if let Some(s) = session {
                    game_session = s;
                }
                break;
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(input)) => {
                        let input = input.trim().to_string();
                        if input.eq_ignore_ascii_case("quit") {
                            println!("Goodbye!");
                            std::process::exit(0);
                        }
                        let mut parts = input.splitn(2, ' ');
                        let cmd = parts.next().unwrap_or("").to_lowercase();
                        let arg = parts.next().unwrap_or("").trim().to_string();
                        match cmd.as_str() {
                            "challenge" if !arg.is_empty() => {
                                let _ = sink_tx2.send(ClientMsg::Challenge { target: arg });
                            }
                            "accept" if !arg.is_empty() => {
                                let _ = sink_tx2.send(ClientMsg::AcceptChallenge { from: arg });
                            }
                            "decline" if !arg.is_empty() => {
                                let _ = sink_tx2.send(ClientMsg::DeclineChallenge { from: arg });
                            }
                            "spectate" if !arg.is_empty() => {
                                let _ = sink_tx2.send(ClientMsg::Spectate { target: arg });
                            }
                            "" => {}
                            _ => {
                                println!("Unknown command. Try: challenge <name>, accept <name>, decline <name>, spectate <name>");
                            }
                        }
                    }
                    _ => break,
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Phase 2: Game — raw mode + renderer
    // ------------------------------------------------------------------
    recv_task.abort();

    // Connect to renderer
    let (renderer_ws, _) = match connect_async(RENDERER_URL).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot connect to renderer at {RENDERER_URL}: {e}");
            eprintln!("Make sure the renderer is running.");
            send_task.abort();
            std::process::exit(1);
        }
    };

    let (mut renderer_sink, mut renderer_stream) = renderer_ws.split();

    // Send our subscription right away.
    let sub = shared::RendererSub { session_id: game_session };
    let json = serde_json::to_string(&sub).unwrap_or_default();
    let _ = renderer_sink.send(Message::Text(json.into())).await;

    // Task: receive frames from renderer and print them
    let render_task = tokio::spawn(async move {
        while let Some(raw) = renderer_stream.next().await {
            let text = match raw {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) | Err(_) => break,
                _ => continue,
            };
            let output: FrameOutput = match serde_json::from_str(&text) {
                Ok(o) => o,
                Err(_) => continue,
            };
            let mut stdout = io::stdout();
            for line in &output.lines {
                let _ = write!(stdout, "{}\r\n", line);
            }
            let _ = stdout.flush();
        }
    });

    println!();
    println!("Controls: ← → move | ↑ rotate CW | Z rotate CCW | Space hard drop | ↓ soft drop | C hold/swap | Q quit");
    println!();

    // Enter raw mode for key capture
    terminal::enable_raw_mode().expect("Failed to enable raw mode");

    loop {
        match crossterm::event::read() {
            Ok(Event::Key(KeyEvent { code, modifiers, .. })) => {
                if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
                if code == KeyCode::Char('q') || code == KeyCode::Char('Q') {
                    break;
                }
                let action = match code {
                    KeyCode::Left        => Some(InputAction::MoveLeft),
                    KeyCode::Right       => Some(InputAction::MoveRight),
                    KeyCode::Up          => Some(InputAction::RotateCw),
                    KeyCode::Char('z') | KeyCode::Char('Z') => Some(InputAction::RotateCcw),
                    KeyCode::Down        => Some(InputAction::SoftDrop),
                    KeyCode::Char(' ')   => Some(InputAction::HardDrop),
                    KeyCode::Char('c') | KeyCode::Char('C') => Some(InputAction::Swap),
                    _ => None,
                };
                if let Some(a) = action {
                    let _ = sink_tx.send(ClientMsg::Input { action: a });
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Event read error: {e}");
                break;
            }
        }
    }

    terminal::disable_raw_mode().ok();
    send_task.abort();
    render_task.abort();
    println!("\r\nGoodbye!");
}
