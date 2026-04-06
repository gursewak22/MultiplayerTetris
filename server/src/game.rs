use rand::seq::SliceRandom;
use rand::thread_rng;
use shared::PieceType;

// ---------------------------------------------------------------------------
// Rotation tables (SRS-inspired, 4 rotations × up-to-4 cells)
// Each entry is [(col_offset, row_offset); 4]
// ---------------------------------------------------------------------------

pub fn piece_cells(piece: PieceType, rotation: u8) -> [(i32, i32); 4] {
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

// ---------------------------------------------------------------------------
// Board
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Board {
    pub cells: [[u8; 10]; 20],
}

impl Board {
    pub fn new() -> Self {
        Self {
            cells: [[0u8; 10]; 20],
        }
    }

    /// Returns true if the piece at (px, py) with the given rotation fits inside
    /// the board without overlapping filled cells.
    pub fn is_valid(&self, piece: PieceType, px: i32, py: i32, rotation: u8) -> bool {
        for (dx, dy) in piece_cells(piece, rotation) {
            let x = px + dx;
            let y = py + dy;
            if x < 0 || x >= 10 || y >= 20 {
                return false;
            }
            if y >= 0 && self.cells[y as usize][x as usize] != 0 {
                return false;
            }
        }
        true
    }

    /// Locks a piece onto the board with its colour id.
    pub fn lock_piece(&mut self, piece: PieceType, px: i32, py: i32, rotation: u8) {
        let color = piece_color_id(piece);
        for (dx, dy) in piece_cells(piece, rotation) {
            let x = (px + dx) as usize;
            let y = (py + dy) as usize;
            if y < 20 && x < 10 {
                self.cells[y][x] = color;
            }
        }
    }

    /// Removes full rows, returns the number cleared.
    pub fn clear_lines(&mut self) -> u32 {
        let mut new_board = [[0u8; 10]; 20];
        let mut dst = 19i32;
        let mut cleared = 0u32;
        for row in (0..20i32).rev() {
            let full = self.cells[row as usize].iter().all(|&c| c != 0);
            if full {
                cleared += 1;
            } else {
                new_board[dst as usize] = self.cells[row as usize];
                dst -= 1;
            }
        }
        self.cells = new_board;
        cleared
    }
}

/// Maps a piece type to a non-zero colour id used in the board array.
pub fn piece_color_id(piece: PieceType) -> u8 {
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

// ---------------------------------------------------------------------------
// Seven-bag randomizer
// ---------------------------------------------------------------------------

pub struct SevenBag {
    bag: Vec<PieceType>,
    index: usize,
}

impl SevenBag {
    pub fn new() -> Self {
        let mut bag = Self {
            bag: Vec::new(),
            index: 7,
        };
        bag.refill();
        bag
    }

    fn refill(&mut self) {
        let mut pieces = [
            PieceType::I,
            PieceType::O,
            PieceType::T,
            PieceType::S,
            PieceType::Z,
            PieceType::J,
            PieceType::L,
        ];
        pieces.shuffle(&mut thread_rng());
        self.bag = pieces.to_vec();
        self.index = 0;
    }

    pub fn next(&mut self) -> PieceType {
        if self.index >= self.bag.len() {
            self.refill();
        }
        let p = self.bag[self.index];
        self.index += 1;
        p
    }

    /// Peek at the next piece without consuming it.
    pub fn peek(&self) -> PieceType {
        if self.index < self.bag.len() {
            self.bag[self.index]
        } else {
            // Would come from a fresh bag; just return I as a placeholder.
            PieceType::I
        }
    }
}

// ---------------------------------------------------------------------------
// Gravity helpers
// ---------------------------------------------------------------------------

/// Returns the number of milliseconds between gravity drops for the given level.
/// Level 1 → ~800 ms (≈48 frames @ 60fps), decreasing to ~100 ms at level 10+.
pub fn gravity_interval_ms(level: u32) -> u64 {
    let level = level.min(20);
    // Rough approximation of NES Tetris gravity
    let frames: u64 = match level {
        1 => 48,
        2 => 43,
        3 => 38,
        4 => 33,
        5 => 28,
        6 => 23,
        7 => 18,
        8 => 13,
        9 => 8,
        10..=12 => 6,
        13..=15 => 5,
        16..=18 => 4,
        19..=28 => 3,
        _ => 1,
    };
    // 60 fps → 1 frame ≈ 16.67 ms
    (frames * 1000) / 60
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Nintendo scoring: lines cleared × level multiplier.
pub fn score_for_lines(lines: u32, level: u32) -> u32 {
    let base = match lines {
        1 => 100,
        2 => 300,
        3 => 500,
        4 => 800,
        _ => 0,
    };
    base * level.max(1)
}
