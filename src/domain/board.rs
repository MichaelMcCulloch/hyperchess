use crate::domain::coordinate::Coordinate;
use crate::domain::models::{GameResult, Move, Piece, PieceType, Player};
use crate::domain::zobrist::ZobristKeys;
use std::fmt;
use std::sync::Arc;

// BitBoard implementation as Domain Primitive for Board Representation
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BitBoard {
    Small(u32),
    Medium(u128),
    Large { data: Vec<u64> },
}

#[derive(Clone, Debug)]
pub struct Board {
    pub dimension: usize,
    pub side: usize,
    pub total_cells: usize,

    // Occupancy (fast collision check)
    pub white_occupancy: BitBoard,
    pub black_occupancy: BitBoard,

    pub pawns: BitBoard,
    pub rooks: BitBoard,
    pub knights: BitBoard,
    pub bishops: BitBoard,
    pub queens: BitBoard,
    pub kings: BitBoard,

    // Hashing and History
    pub zobrist: Arc<ZobristKeys>,
    pub hash: u64,
    pub history: Vec<u64>,
    pub en_passant_target: Option<usize>,
    pub castling_rights: u8,
}

impl Board {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let empty = BitBoard::new_empty(dimension, side);

        let zobrist = Arc::new(ZobristKeys::new(total_cells));
        let hash = 0;

        Board {
            dimension,
            side,
            total_cells,
            white_occupancy: empty.clone(),
            black_occupancy: empty.clone(),
            pawns: empty.clone(),
            rooks: empty.clone(),
            knights: empty.clone(),
            bishops: empty.clone(),
            queens: empty.clone(),
            kings: empty.clone(),
            zobrist,
            hash,
            history: Vec::new(),
            en_passant_target: None,
            castling_rights: 0,
        }
    }

    pub fn new(dimension: usize, side: usize) -> Self {
        let mut board = Self::new_empty(dimension, side);
        board.castling_rights = 0xF; // Assume standard start rights
        board.setup_standard_chess();
        board
    }

    // NOTE: Temporarily duplicating helper for now to avoid circular dep if needed,
    // but ideally we move coords_to_index utility to domain too?
    // Actually, persistence.rs will be DELETED, so we need to move the utility functions!
    // I will add them to this file at the bottom or as methods.

    pub fn coords_to_index(&self, coords: &[usize]) -> Option<usize> {
        let mut index = 0;
        let mut multiplier = 1;
        for &c in coords {
            if c >= self.side {
                return None;
            }
            index += c * multiplier;
            multiplier *= self.side;
        }
        Some(index)
    }

    pub fn index_to_coords(&self, index: usize) -> Vec<usize> {
        let mut coords = vec![0; self.dimension];
        let mut temp = index;
        for i in 0..self.dimension {
            coords[i] = temp % self.side;
            temp /= self.side;
        }
        coords
    }

    fn remove_piece_at_index(&mut self, index: usize) {
        self.white_occupancy.clear_bit(index);
        self.black_occupancy.clear_bit(index);
        self.pawns.clear_bit(index);
        self.rooks.clear_bit(index);
        self.knights.clear_bit(index);
        self.bishops.clear_bit(index);
        self.queens.clear_bit(index);
        self.kings.clear_bit(index);
    }

    fn place_piece_at_index(&mut self, index: usize, piece: Piece) {
        match piece.owner {
            Player::White => self.white_occupancy.set_bit(index),
            Player::Black => self.black_occupancy.set_bit(index),
        }

        match piece.piece_type {
            PieceType::Pawn => self.pawns.set_bit(index),
            PieceType::Rook => self.rooks.set_bit(index),
            PieceType::Knight => self.knights.set_bit(index),
            PieceType::Bishop => self.bishops.set_bit(index),
            PieceType::Queen => self.queens.set_bit(index),
            PieceType::King => self.kings.set_bit(index),
        }
    }

    pub fn setup_standard_chess(&mut self) {
        for file_y in 0..self.side {
            // --- White Pieces (z=0, w=0, ...) ---
            let mut white_coords = vec![0; self.dimension];
            white_coords[1] = file_y;

            // Place Pawn at Rank 1
            white_coords[0] = 1;
            if let Some(idx) = self.coords_to_index(&white_coords) {
                self.place_piece_at_index(
                    idx,
                    Piece {
                        piece_type: PieceType::Pawn,
                        owner: Player::White,
                    },
                );
            }

            // Place Backrank at Rank 0
            white_coords[0] = 0;
            if let Some(idx) = self.coords_to_index(&white_coords) {
                let piece_type = self.determine_backrank_piece(file_y, self.side);
                self.place_piece_at_index(
                    idx,
                    Piece {
                        piece_type,
                        owner: Player::White,
                    },
                );
            }

            // --- Black Pieces (z=side-1, w=side-1, ...) ---
            // Initialize all coords to side-1 (for z, w, ...)
            let mut black_coords = vec![self.side - 1; self.dimension];
            // But 'y' varies
            black_coords[1] = file_y;

            // Place Pawn at Rank side-2
            if self.side > 3 {
                black_coords[0] = self.side - 2;
                if let Some(idx) = self.coords_to_index(&black_coords) {
                    self.place_piece_at_index(
                        idx,
                        Piece {
                            piece_type: PieceType::Pawn,
                            owner: Player::Black,
                        },
                    );
                }
            }

            // Place Backrank at Rank side-1
            black_coords[0] = self.side - 1;
            if let Some(idx) = self.coords_to_index(&black_coords) {
                let piece_type = self.determine_backrank_piece(file_y, self.side);
                self.place_piece_at_index(
                    idx,
                    Piece {
                        piece_type,
                        owner: Player::Black,
                    },
                );
            }
        }
        self.hash = self.zobrist.get_hash(self, Player::White);
    }

    fn determine_backrank_piece(&self, file_idx: usize, total_files: usize) -> PieceType {
        // Special case for 2D 8x8 standard chess
        if self.dimension == 2 && self.side == 8 {
            return match file_idx {
                0 | 7 => PieceType::Rook,
                1 | 6 => PieceType::Knight,
                2 | 5 => PieceType::Bishop,
                3 => PieceType::Queen,
                4 => PieceType::King,
                _ => PieceType::Pawn,
            };
        }

        let king_pos = total_files / 2;
        if file_idx == king_pos {
            return PieceType::King;
        }
        if file_idx + 1 == king_pos {
            return PieceType::Queen;
        }

        if file_idx == 0 || file_idx == total_files - 1 {
            return PieceType::Rook;
        }
        if file_idx == 1 || file_idx == total_files - 2 {
            return PieceType::Knight;
        }

        PieceType::Bishop
    }

    pub fn is_repetition(&self) -> bool {
        let count = self.history.iter().filter(|&&h| h == self.hash).count();
        count >= 1
    }

    pub fn update_hash(&mut self, player_to_move: Player) {
        self.hash = self.zobrist.get_hash(self, player_to_move);
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn side(&self) -> usize {
        self.side
    }

    pub fn total_cells(&self) -> usize {
        self.total_cells
    }

    pub fn get_piece(&self, coord: &Coordinate) -> Option<Piece> {
        let index = self.coords_to_index(&coord.values)?;

        let owner = if self.white_occupancy.get_bit(index) {
            Some(Player::White)
        } else if self.black_occupancy.get_bit(index) {
            Some(Player::Black)
        } else {
            None
        }?;

        let piece_type = if self.pawns.get_bit(index) {
            PieceType::Pawn
        } else if self.rooks.get_bit(index) {
            PieceType::Rook
        } else if self.knights.get_bit(index) {
            PieceType::Knight
        } else if self.bishops.get_bit(index) {
            PieceType::Bishop
        } else if self.queens.get_bit(index) {
            PieceType::Queen
        } else if self.kings.get_bit(index) {
            PieceType::King
        } else {
            return None;
        };

        Some(Piece { piece_type, owner })
    }

    pub fn apply_move(&mut self, mv: &Move) -> Result<(), String> {
        let from_idx = self
            .coords_to_index(&mv.from.values)
            .ok_or("Invalid From coord")?;
        let to_idx = self
            .coords_to_index(&mv.to.values)
            .ok_or("Invalid To coord")?;

        let moving_piece = self.get_piece(&mv.from).ok_or("No piece at origin")?;

        self.history.push(self.hash);

        // --- En Passant Logic ---
        // 1. Check if this is an EP capture
        if moving_piece.piece_type == PieceType::Pawn {
            // If moving to EP target ..
            if let Some(ep_idx) = self.en_passant_target {
                if to_idx == ep_idx {
                    // Capture the pawn "behind" the target.
                    // The target is "behind" the pawn relative to its movement?
                    // No, "En Passant Target" is the square skipped over by the enemy pawn.
                    // The enemy pawn is actually at `to - forward`.
                    // Wait, standard convention: EP target is the square BEHIND the pawn that just moved.
                    // So if White Pawn moves A2 -> A4, EP target is A3.
                    // Black Pawn captures on A3. Black Pawn ends up on A3.
                    // The captured pawn is on A4.
                    // Relation: Captured Pawn is at `to - forward_step_of_capturer`?
                    // No. `to` is A3. Capturer (Black) moves B4 -> A3 (Forward is -1).
                    // Captured White Pawn is at A4. A4 = A3 - (-1) = A3 + 1.
                    // Correct. Captured Pawn is at `to - (forward_dir)` of the moving pawn.

                    let capture_dir = match moving_piece.owner {
                        Player::White => 1,
                        Player::Black => -1,
                    };
                    // We need to calculate the index of the captured pawn.
                    // `to` coords - (dir in dim 0).
                    let mut captured_coords = mv.to.values.clone();
                    // Note: stored dim is variable but standard chess is dim 0 for ranks.
                    let rank = captured_coords[0] as isize - capture_dir;
                    if rank >= 0 && rank < self.side as isize {
                        captured_coords[0] = rank as usize;
                        if let Some(cap_idx) = self.coords_to_index(&captured_coords) {
                            self.remove_piece_at_index(cap_idx);
                        }
                    }
                }
            }
        }

        // 2. Clear EP target for the next turn
        self.en_passant_target = None;

        // 3. Set EP target if double push
        if moving_piece.piece_type == PieceType::Pawn {
            let dist = (mv.from.values[0] as isize - mv.to.values[0] as isize).abs();
            if dist == 2 {
                // Set target to the square skipped.
                let dir = if mv.to.values[0] > mv.from.values[0] {
                    1
                } else {
                    -1
                };
                let mut target_vals = mv.from.values.clone();
                target_vals[0] = (target_vals[0] as isize + dir) as usize;
                self.en_passant_target = self.coords_to_index(&target_vals);
            }
        }

        // --- Castling Logic ---
        let mut castling_rook_move = None;

        // Update Rights
        // If King moves, lose all rights for that player
        if moving_piece.piece_type == PieceType::King {
            match moving_piece.owner {
                Player::White => self.castling_rights &= !0x3, // Clear 0, 1 (White Kingside/Queenside)
                Player::Black => self.castling_rights &= !0xC, // Clear 2, 3 (Black Kingside/Queenside)
            }
        }

        // If Rook moves or is captured, logic is trickier without tracking which rook is which.
        // Simplified: Check typical rook starting squares.
        // White KS Rook: (0, 7) -> 0x1
        // White QS Rook: (0, 0) -> 0x2
        // Black KS Rook: (7, 7) -> 0x4
        // Black QS Rook: (7, 0) -> 0x8
        // Note: Assuming standard board setup
        if self.dimension == 2 && self.side == 8 {
            // Check From (Rook move) and To (Rook capture)
            for idx in [from_idx, to_idx] {
                match idx {
                    7 => self.castling_rights &= !0x1,  // White KS Rook (H1)
                    0 => self.castling_rights &= !0x2,  // White QS Rook (A1)
                    63 => self.castling_rights &= !0x4, // Black KS Rook (H8)
                    56 => self.castling_rights &= !0x8, // Black QS Rook (A8) - Wait, A8 is sq 56?
                    // 0-7 is rank 0. 56-63 is rank 7.
                    // A1=0, H1=7. A8=56, H8=63. Correct.
                    _ => {}
                }
            }
        }

        // Check if this IS a castling move (King moves 2 squares)
        if moving_piece.piece_type == PieceType::King {
            let dist = (mv.from.values[1] as isize - mv.to.values[1] as isize).abs();
            if dist == 2 {
                // Identify Rook and Move it.
                // Kingside: y increases. Queenside: y decreases.
                let is_kingside = mv.to.values[1] > mv.from.values[1];
                let _rank = mv.from.values[0]; // 0 or 7
                let (rook_from_y, rook_to_y) = if is_kingside {
                    (self.side - 1, mv.to.values[1] - 1) // H -> F
                } else {
                    (0, mv.to.values[1] + 1) // A -> D
                };

                let mut rook_from_coords = mv.from.values.clone();
                rook_from_coords[1] = rook_from_y;
                let rook_from_idx = self
                    .coords_to_index(&rook_from_coords)
                    .ok_or("Invalid rook from")?;

                let mut rook_to_coords = mv.from.values.clone();
                rook_to_coords[1] = rook_to_y;
                let rook_to_idx = self
                    .coords_to_index(&rook_to_coords)
                    .ok_or("Invalid rook to")?;

                // Remove Rook from old pos, Place in new.
                // Need to know WHO owns it (same as king)
                let rook_piece = Piece {
                    piece_type: PieceType::Rook,
                    owner: moving_piece.owner,
                };
                castling_rook_move = Some((rook_from_idx, rook_to_idx, rook_piece));
            }
        }

        // --- Apply Main Move ---
        self.remove_piece_at_index(from_idx);
        self.remove_piece_at_index(to_idx);

        let piece_to_place = if let Some(promo_type) = mv.promotion {
            Piece {
                piece_type: promo_type,
                owner: moving_piece.owner,
            }
        } else {
            moving_piece
        };

        self.place_piece_at_index(to_idx, piece_to_place);

        // --- Apply Castling Rook Move (Secondary) ---
        if let Some((r_from, r_to, r_piece)) = castling_rook_move {
            self.remove_piece_at_index(r_from);
            self.place_piece_at_index(r_to, r_piece);
        }

        self.hash = self.zobrist.get_hash(self, moving_piece.owner.opponent());

        Ok(())
    }

    pub fn get_king_coordinate(&self, player: Player) -> Option<Coordinate> {
        let occupancy = match player {
            Player::White => &self.white_occupancy,
            Player::Black => &self.black_occupancy,
        };

        for i in 0..self.total_cells {
            if occupancy.get_bit(i) && self.kings.get_bit(i) {
                return Some(Coordinate::new(self.index_to_coords(i)));
            }
        }
        None
    }

    pub fn set_piece(&mut self, coord: &Coordinate, piece: Piece) -> Result<(), String> {
        let index = self.coords_to_index(&coord.values).ok_or("Invalid coord")?;
        self.remove_piece_at_index(index);
        self.place_piece_at_index(index, piece);
        self.hash = self.zobrist.get_hash(self, Player::White);
        Ok(())
    }

    pub fn clear_cell(&mut self, coord: &Coordinate) {
        if let Some(index) = self.coords_to_index(&coord.values) {
            self.remove_piece_at_index(index);
            self.hash = self.zobrist.get_hash(self, Player::White);
        }
    }

    pub fn check_status(&self, _player_to_move: Player) -> GameResult {
        GameResult::InProgress
    }
}

// Display trait implementation - might move to presentation/display?
// For now, keep generic Debug or Display
impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Needs display.rs logic.. will refactor later.
        write!(f, "Board(dim={}, side={})", self.dimension, self.side)
    }
}

impl BitBoard {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        if total_cells <= 32 {
            BitBoard::Small(0)
        } else if total_cells <= 128 {
            BitBoard::Medium(0)
        } else {
            let len = (total_cells + 63) / 64;
            BitBoard::Large { data: vec![0; len] }
        }
    }

    pub fn set_bit(&mut self, index: usize) {
        match self {
            BitBoard::Small(b) => *b |= 1 << index,
            BitBoard::Medium(b) => {
                *b |= 1 << index;
            }
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if vec_idx < data.len() {
                    data[vec_idx] |= 1 << (index % 64);
                }
            }
        }
    }

    pub fn clear_bit(&mut self, index: usize) {
        match self {
            BitBoard::Small(b) => *b &= !(1 << index),
            BitBoard::Medium(b) => *b &= !(1 << index),
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if vec_idx < data.len() {
                    data[vec_idx] &= !(1 << (index % 64));
                }
            }
        }
    }

    pub fn get_bit(&self, index: usize) -> bool {
        match self {
            BitBoard::Small(b) => (*b & (1 << index)) != 0,
            BitBoard::Medium(b) => (*b & (1 << index)) != 0,
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if let Some(chunk) = data.get(vec_idx) {
                    (chunk & (1 << (index % 64))) != 0
                } else {
                    false
                }
            }
        }
    }

    pub fn count_ones(&self) -> u32 {
        match self {
            BitBoard::Small(b) => b.count_ones(),
            BitBoard::Medium(b) => b.count_ones(),
            BitBoard::Large { data } => data.iter().map(|c| c.count_ones()).sum(),
        }
    }

    pub fn or_with(self, other: &Self) -> Self {
        match (self, other) {
            (BitBoard::Small(a), BitBoard::Small(b)) => BitBoard::Small(a | b),
            (BitBoard::Medium(a), BitBoard::Medium(b)) => BitBoard::Medium(a | b),
            (BitBoard::Large { mut data }, BitBoard::Large { data: other_data }) => {
                for (i, x) in data.iter_mut().enumerate() {
                    if i < other_data.len() {
                        *x |= other_data[i];
                    }
                }
                BitBoard::Large { data }
            }
            _ => panic!("Mismatched BitBoard types"),
        }
    }
}
