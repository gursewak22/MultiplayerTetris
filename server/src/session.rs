use shared::{DrawFrame, InputAction, PieceState, PieceType};

use crate::game::{gravity_interval_ms, score_for_lines, Board, SevenBag};

const LOCK_DELAY_MS: u64 = 500;
const SWAP_COOLDOWN_MS: u64 = 10_000;

// ---------------------------------------------------------------------------
// Per-player state
// ---------------------------------------------------------------------------

struct PlayerState {
    board: Board,
    current_piece: PieceType,
    current_x: i32,
    current_y: i32,
    current_rot: u8,
    next_piece: PieceType,
    hold_piece: Option<PieceType>,   // last piece sent to opponent (display only)
    queued_piece: Option<PieceType>, // piece injected by opponent swap; consumed on next spawn
    swap_cooldown_ms: u64,
    bag: SevenBag,
    score: u32,
    lines: u32,
    level: u32,
    gravity_accum_ms: u64,
    lock_accum_ms: u64,
    on_ground: bool,
    /// True once the player has topped out.
    topped_out: bool,
}

impl PlayerState {
    fn new() -> Self {
        let mut bag = SevenBag::new();
        let current_piece = bag.next();
        let next_piece = bag.peek();
        let (spawn_x, spawn_y) = spawn_position(current_piece);
        Self {
            board: Board::new(),
            current_piece,
            current_x: spawn_x,
            current_y: spawn_y,
            current_rot: 0,
            next_piece,
            hold_piece: None,
            queued_piece: None,
            swap_cooldown_ms: 0,
            bag,
            score: 0,
            lines: 0,
            level: 1,
            gravity_accum_ms: 0,
            lock_accum_ms: 0,
            on_ground: false,
            topped_out: false,
        }
    }

    /// Spawn a new piece from the bag. Returns false if the spawn position is
    /// blocked (top-out).
    fn spawn_next(&mut self) -> bool {
        self.current_piece = self.queued_piece.take().unwrap_or_else(|| self.bag.next());
        self.next_piece = self.bag.peek();
        let (sx, sy) = spawn_position(self.current_piece);
        self.current_x = sx;
        self.current_y = sy;
        self.current_rot = 0;
        self.gravity_accum_ms = 0;
        self.lock_accum_ms = 0;
        self.on_ground = false;
        // Check if the spawn position is already blocked.
        if !self.board.is_valid(self.current_piece, self.current_x, self.current_y, self.current_rot) {
            self.topped_out = true;
            return false;
        }
        true
    }

    /// Advance gravity and lock delay by `elapsed_ms`. Returns true if a line
    /// clear happened (useful for sending updates).
    fn tick(&mut self, elapsed_ms: u64) {
        if self.topped_out {
            return;
        }

        // Tick swap cooldown.
        self.swap_cooldown_ms = self.swap_cooldown_ms.saturating_sub(elapsed_ms);

        let gravity_ms = gravity_interval_ms(self.level);

        self.gravity_accum_ms += elapsed_ms;

        // Check if the piece is resting on the ground.
        let below_valid = self.board.is_valid(
            self.current_piece,
            self.current_x,
            self.current_y + 1,
            self.current_rot,
        );

        if below_valid {
            self.on_ground = false;
            self.lock_accum_ms = 0;
            // Drop by gravity.
            if self.gravity_accum_ms >= gravity_ms {
                self.gravity_accum_ms = 0;
                self.current_y += 1;
            }
        } else {
            self.on_ground = true;
            self.lock_accum_ms += elapsed_ms;
            if self.lock_accum_ms >= LOCK_DELAY_MS {
                self.lock_current();
            }
        }
    }

    /// Lock the current piece, clear lines, update score, spawn next.
    fn lock_current(&mut self) {
        self.board.lock_piece(
            self.current_piece,
            self.current_x,
            self.current_y,
            self.current_rot,
        );
        let cleared = self.board.clear_lines();
        if cleared > 0 {
            self.lines += cleared;
            self.level = (self.lines / 10) + 1;
            self.score += score_for_lines(cleared, self.level);
        }
        self.lock_accum_ms = 0;
        self.gravity_accum_ms = 0;
        self.on_ground = false;
        self.spawn_next();
    }

    fn apply_input(&mut self, action: InputAction) {
        if self.topped_out {
            return;
        }
        match action {
            InputAction::MoveLeft => {
                if self.board.is_valid(
                    self.current_piece,
                    self.current_x - 1,
                    self.current_y,
                    self.current_rot,
                ) {
                    self.current_x -= 1;
                    // Reset lock delay on successful move.
                    self.lock_accum_ms = 0;
                }
            }
            InputAction::MoveRight => {
                if self.board.is_valid(
                    self.current_piece,
                    self.current_x + 1,
                    self.current_y,
                    self.current_rot,
                ) {
                    self.current_x += 1;
                    self.lock_accum_ms = 0;
                }
            }
            InputAction::RotateCw => {
                let new_rot = (self.current_rot + 1) % 4;
                if let Some((nx, ny)) = self.try_rotate(new_rot) {
                    self.current_x = nx;
                    self.current_y = ny;
                    self.current_rot = new_rot;
                    self.lock_accum_ms = 0;
                }
            }
            InputAction::RotateCcw => {
                let new_rot = (self.current_rot + 3) % 4;
                if let Some((nx, ny)) = self.try_rotate(new_rot) {
                    self.current_x = nx;
                    self.current_y = ny;
                    self.current_rot = new_rot;
                    self.lock_accum_ms = 0;
                }
            }
            InputAction::SoftDrop => {
                if self.board.is_valid(
                    self.current_piece,
                    self.current_x,
                    self.current_y + 1,
                    self.current_rot,
                ) {
                    self.current_y += 1;
                    self.gravity_accum_ms = 0;
                }
            }
            InputAction::HardDrop => {
                // Move piece all the way down.
                while self.board.is_valid(
                    self.current_piece,
                    self.current_x,
                    self.current_y + 1,
                    self.current_rot,
                ) {
                    self.current_y += 1;
                }
                self.lock_current();
            }
            InputAction::Swap => {
                // Handled at GameSession level via do_swap().
            }
        }
    }

    /// Basic wall-kick: try up to 2 offsets (0, ±1 on x) for the new rotation.
    fn try_rotate(&self, new_rot: u8) -> Option<(i32, i32)> {
        let kicks: &[(i32, i32)] = &[(0, 0), (-1, 0), (1, 0), (-2, 0), (2, 0), (0, -1)];
        for &(kx, ky) in kicks {
            if self.board.is_valid(
                self.current_piece,
                self.current_x + kx,
                self.current_y + ky,
                new_rot,
            ) {
                return Some((self.current_x + kx, self.current_y + ky));
            }
        }
        None
    }

    fn swap_cooldown_f32(&self) -> f32 {
        self.swap_cooldown_ms as f32 / SWAP_COOLDOWN_MS as f32
    }

    fn to_piece_state(&self) -> Option<PieceState> {
        if self.topped_out {
            return None;
        }
        Some(PieceState {
            piece_type: self.current_piece,
            x: self.current_x,
            y: self.current_y,
            rotation: self.current_rot,
        })
    }
}

// ---------------------------------------------------------------------------
// Spawn positions
// ---------------------------------------------------------------------------

fn spawn_position(piece: PieceType) -> (i32, i32) {
    // Place the piece horizontally centred, just above the visible board.
    match piece {
        PieceType::I => (3, -1),
        PieceType::O => (3, -1),
        _ => (3, -1),
    }
}

// ---------------------------------------------------------------------------
// GameSession
// ---------------------------------------------------------------------------

pub struct GameSession {
    p1: PlayerState,
    p2: PlayerState,
    pub game_over: Option<u8>,
}

impl GameSession {
    pub fn new() -> Self {
        Self {
            p1: PlayerState::new(),
            p2: PlayerState::new(),
            game_over: None,
        }
    }

    pub fn tick(&mut self, elapsed_ms: u64) {
        if self.game_over.is_some() {
            return;
        }
        self.p1.tick(elapsed_ms);
        self.p2.tick(elapsed_ms);

        if self.p1.topped_out && self.game_over.is_none() {
            self.game_over = Some(1);
        } else if self.p2.topped_out && self.game_over.is_none() {
            self.game_over = Some(2);
        }
    }

    pub fn apply_input(&mut self, player: u8, action: InputAction) {
        if self.game_over.is_some() {
            return;
        }
        if matches!(action, InputAction::Swap) {
            self.do_swap(player);
            return;
        }
        match player {
            1 => self.p1.apply_input(action),
            2 => self.p2.apply_input(action),
            _ => {}
        }
    }

    /// Trade the requesting player's current piece with the opponent's next piece.
    fn do_swap(&mut self, player: u8) {
        let (cooldown, my_current, their_next) = match player {
            1 => (self.p1.swap_cooldown_ms, self.p1.current_piece, self.p2.next_piece),
            2 => (self.p2.swap_cooldown_ms, self.p2.current_piece, self.p1.next_piece),
            _ => return,
        };

        if cooldown != 0 {
            return;
        }

        match player {
            1 => {
                // P1 keeps their current position; only the piece type and rotation change.
                self.p1.current_piece = their_next;
                self.p1.current_rot = 0;
                self.p1.lock_accum_ms = 0;
                self.p1.on_ground = false;
                self.p1.next_piece = self.p1.bag.peek();
                self.p1.hold_piece = Some(my_current); // "last piece sent" display
                self.p1.swap_cooldown_ms = SWAP_COOLDOWN_MS;

                // P2 also gets a cooldown — prevents immediate swap-back.
                self.p2.swap_cooldown_ms = SWAP_COOLDOWN_MS;

                // P2 will receive P1's old piece as their next spawn.
                self.p2.queued_piece = Some(my_current);
                self.p2.next_piece = my_current; // update next preview immediately
            }
            2 => {
                self.p2.current_piece = their_next;
                self.p2.current_rot = 0;
                self.p2.lock_accum_ms = 0;
                self.p2.on_ground = false;
                self.p2.next_piece = self.p2.bag.peek();
                self.p2.hold_piece = Some(my_current);
                self.p2.swap_cooldown_ms = SWAP_COOLDOWN_MS;

                // P1 also gets a cooldown — prevents immediate swap-back.
                self.p1.swap_cooldown_ms = SWAP_COOLDOWN_MS;

                self.p1.queued_piece = Some(my_current);
                self.p1.next_piece = my_current;
            }
            _ => {}
        }
    }

    pub fn to_draw_frame(&self) -> DrawFrame {
        DrawFrame {
            board_p1: self.p1.board.cells,
            board_p2: self.p2.board.cells,
            piece_p1: self.p1.to_piece_state(),
            piece_p2: self.p2.to_piece_state(),
            next_p1: self.p1.next_piece,
            next_p2: self.p2.next_piece,
            hold_p1: self.p1.hold_piece,
            hold_p2: self.p2.hold_piece,
            score_p1: self.p1.score,
            score_p2: self.p2.score,
            lines_p1: self.p1.lines,
            lines_p2: self.p2.lines,
            swap_cooldown_p1: self.p1.swap_cooldown_f32(),
            swap_cooldown_p2: self.p2.swap_cooldown_f32(),
            game_over: self.game_over,
        }
    }

}

