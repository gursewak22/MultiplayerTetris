use serde::{Deserialize, Serialize};

/// Actions the client can send to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputAction {
    MoveLeft,
    MoveRight,
    RotateCw,
    RotateCcw,
    HardDrop,
    SoftDrop,
    Swap,
}

/// Messages sent from a client to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Input { action: InputAction },
    JoinLobby { name: String },
    Challenge { target: String },
    AcceptChallenge { from: String },
    DeclineChallenge { from: String },
}

/// Piece type identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PieceType {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

/// Piece position and rotation snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceState {
    pub piece_type: PieceType,
    pub x: i32,
    pub y: i32,
    pub rotation: u8,
}

/// Full frame for the renderer describing both players' boards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawFrame {
    pub board_p1: [[u8; 10]; 20],
    pub board_p2: [[u8; 10]; 20],
    pub piece_p1: Option<PieceState>,
    pub piece_p2: Option<PieceState>,
    pub next_p1: PieceType,
    pub next_p2: PieceType,
    pub hold_p1: Option<PieceType>,
    pub hold_p2: Option<PieceType>,
    pub score_p1: u32,
    pub score_p2: u32,
    pub lines_p1: u32,
    pub lines_p2: u32,
    pub swap_cooldown_p1: f32,
    pub swap_cooldown_p2: f32,
    /// Which player lost (1 or 2), or None if the game is still going.
    pub game_over: Option<u8>,
}

/// Messages sent from the server to a client (lobby / match lifecycle).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    LobbyState { players: Vec<String> },
    ChallengeReceived { from: String },
    GameStart,
    GameOver { winner: String },
}

/// A rendered frame that the renderer pushes to connected clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameOutput {
    /// ANSI-coloured lines ready to print.
    pub lines: Vec<String>,
}
