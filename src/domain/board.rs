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
    pub en_passant_target: Option<(usize, usize)>, // (target_index, victim_index)
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
            .ok_or("Invalid from".to_string())?;
        let to_idx = self
            .coords_to_index(&mv.to.values)
            .ok_or("Invalid to".to_string())?;

        let moving_piece = self
            .get_piece(&mv.from)
            .ok_or("No piece at from".to_string())?;

        self.history.push(self.hash);

        // 1. Check if this is an EP capture
        if moving_piece.piece_type == PieceType::Pawn {
            if let Some((target, victim)) = self.en_passant_target {
                if to_idx == target {
                    // EP Capture: Remove the victim pawn
                    self.remove_piece_at_index(victim);
                }
            }
        }

        // 2. Clear EP target for the next turn
        self.en_passant_target = None;

        // 3. Set EP target if double push
        // Note: We need to detect "Double Push" on ANY axis for Phase 2.
        // For now, assuming standard rank-based double push, but keeping generic distance check.
        if moving_piece.piece_type == PieceType::Pawn {
            // Check distance on all axes. If exactly ONE axis has distance 2, and others 0.
            let mut diffs = Vec::new();
            for i in 0..self.dimension {
                let d = (mv.from.values[i] as isize - mv.to.values[i] as isize).abs();
                diffs.push(d);
            }

            // A double push must be distance 2 on movement axis and 0 on others?
            // Yes, strict double push.
            // Identify the movement axis.
            let double_step_axis = diffs.iter().position(|&d| d == 2);
            let any_other_movement = diffs
                .iter()
                .enumerate()
                .any(|(i, &d)| i != double_step_axis.unwrap_or(999) && d != 0);

            if let Some(axis) = double_step_axis {
                if !any_other_movement {
                    // It is a double push!
                    // Target is the skipped square.
                    // Victim is `to_idx` (the pawn that moved).
                    let dir = if mv.to.values[axis] > mv.from.values[axis] {
                        1
                    } else {
                        -1
                    };
                    let mut target_vals = mv.from.values.clone();
                    target_vals[axis] = (target_vals[axis] as isize + dir) as usize;
                    if let Some(target_idx) = self.coords_to_index(&target_vals) {
                        self.en_passant_target = Some((target_idx, to_idx));
                    }
                }
            }
        }

        // --- Castling Logic ---
        let mut castling_rook_move = None;

        // Update Rights if King Moves
        if moving_piece.piece_type == PieceType::King {
            match moving_piece.owner {
                Player::White => self.castling_rights &= !0x3,
                Player::Black => self.castling_rights &= !0xC,
            }
        }

        // Update Rights if Rook Moves/Captured (Side=8 enforced)
        if self.side == 8 {
            // White Rooks: A1 (0,0), H1 (0,7). Indices depend on dimension.
            // We need to construct coords to check indices.
            // But we can check "corner" properties if standard setup.
            // Let's safely calculate rook indices.

            // Base coords: Rank=0 (White) or 7 (Black).
            // File=0 or 7.
            // Other Axes: 0 (White) or 7 (Black).
            let w_rank = 0;
            let b_rank = 7;

            let mut w_qs_c = vec![w_rank; self.dimension];
            w_qs_c[1] = 0;
            let mut w_ks_c = vec![w_rank; self.dimension];
            w_ks_c[1] = 7;

            let mut b_qs_c = vec![b_rank; self.dimension];
            b_qs_c[1] = 0;
            let mut b_ks_c = vec![b_rank; self.dimension];
            b_ks_c[1] = 7;

            let w_qs = self.coords_to_index(&w_qs_c);
            let w_ks = self.coords_to_index(&w_ks_c);
            let b_qs = self.coords_to_index(&b_qs_c);
            let b_ks = self.coords_to_index(&b_ks_c);

            for idx in [from_idx, to_idx] {
                if Some(idx) == w_qs {
                    self.castling_rights &= !0x2;
                } else if Some(idx) == w_ks {
                    self.castling_rights &= !0x1;
                } else if Some(idx) == b_qs {
                    self.castling_rights &= !0x8;
                } else if Some(idx) == b_ks {
                    self.castling_rights &= !0x4;
                }
            }
        }

        // Check Execution (King moves 2 sq on Axis 1)
        if moving_piece.piece_type == PieceType::King {
            let dist_file = (mv.from.values[1] as isize - mv.to.values[1] as isize).abs();

            let mut other_axes_moved = false;
            for i in 0..self.dimension {
                if i != 1 && mv.from.values[i] != mv.to.values[i] {
                    other_axes_moved = true;
                    break;
                }
            }

            if dist_file == 2 && !other_axes_moved {
                // Must be castling.
                // Kingside: 4 -> 6. Queenside: 4 -> 2.
                let is_kingside = mv.to.values[1] > mv.from.values[1];
                let rook_file_from = if is_kingside { 7 } else { 0 };
                let rook_file_to = if is_kingside { 5 } else { 3 };

                let mut rook_from_coords = mv.from.values.clone();
                rook_from_coords[1] = rook_file_from;

                let mut rook_to_coords = mv.from.values.clone();
                rook_to_coords[1] = rook_file_to;

                let r_from_idx = self.coords_to_index(&rook_from_coords);
                let r_to_idx = self.coords_to_index(&rook_to_coords);
                if let (Some(r_from), Some(r_to)) = (r_from_idx, r_to_idx) {
                    let rook_piece = Piece {
                        piece_type: PieceType::Rook,
                        owner: moving_piece.owner,
                    };
                    castling_rook_move = Some((r_from, r_to, rook_piece));
                }
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
