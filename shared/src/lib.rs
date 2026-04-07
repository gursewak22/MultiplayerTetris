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
    Spectate { target: String },
    JoinTournament,
    LeaveTournament,
    StartTournament,
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
    GameStart { session_id: u64 },
    GameOver { winner: String },
    Scoreboard { scores: Vec<(String, u32)> },
    SpectateInfo { session_id: u64 },
    ServerError { msg: String },
    TournamentState { queue: Vec<String>, bracket: Option<TournamentBracket> },
}

/// A rendered frame that the renderer pushes to connected clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameOutput {
    /// ANSI-coloured lines ready to print.
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RenderMsg {
    Frames { frames: Vec<(u64, DrawFrame)> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererSub {
    pub session_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentBracket {
    pub players: Vec<String>,
    pub matches: Vec<(String, String)>,
    pub winners: Vec<String>,
    pub round: u32,
    pub champion: Option<String>,
}

impl TournamentBracket {
    pub fn new(players: Vec<String>) -> Self {
        let matches = Self::make_matches(&players);
        Self {
            players,
            matches,
            winners: Vec::new(),
            round: 1,
            champion: None,
        }
    }

    pub fn make_matches(players: &[String]) -> Vec<(String, String)> {
        players.chunks(2).map(|c| {
            if c.len() == 2 {
                (c[0].clone(), c[1].clone())
            } else {
                (c[0].clone(), "BYE".to_string())
            }
        }).collect()
    }

    pub fn record_winner(&mut self, winner: String) -> bool {
        if !self.winners.contains(&winner) {
            self.winners.push(winner);
        }
        if self.winners.len() == self.matches.len() {
            if self.winners.len() == 1 {
                self.champion = Some(self.winners[0].clone());
            } else {
                self.players = self.winners.drain(..).collect();
                self.matches = Self::make_matches(&self.players);
                self.round += 1;
            }
            return true;
        }
        false
    }
    
    // We can remove is_round_complete() or keep it, doesn't matter.
}
