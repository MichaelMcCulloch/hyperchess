use crate::domain::coordinate::Coordinate;
use crate::domain::models::{GameResult, Move, Piece, PieceType, Player};
use crate::domain::zobrist::ZobristKeys;
use smallvec::{smallvec, SmallVec};
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
pub struct UnmakeInfo {
    pub captured: Option<(usize, Piece)>,
    pub en_passant_target: Option<(usize, usize)>,
    pub castling_rights: u8,
}

pub struct BitIterator<'a> {
    board: &'a BitBoard,
    current_chunk_idx: usize,
    current_chunk: u64,
}

impl<'a> BitIterator<'a> {
    pub fn new(board: &'a BitBoard) -> Self {
        let (first_chunk, start_idx) = match board {
            BitBoard::Small(b) => (*b as u64, 0),
            BitBoard::Medium(b) => (*b as u64, 0),
            BitBoard::Large { data } => {
                if data.is_empty() {
                    (0, 0)
                } else {
                    (data[0], 0)
                }
            }
        };

        Self {
            board,
            current_chunk_idx: start_idx,
            current_chunk: first_chunk,
        }
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_chunk != 0 {
                // Hardware intrinsic: TZCNT via trailing_zeros()
                let trailing = self.current_chunk.trailing_zeros();

                // Hardware intrinsic: BLSR (Reset lowest set bit)
                // self.current_chunk &= self.current_chunk - 1;
                // Note: Rust compiler optimizes the below to BLSR if BMI1 is enabled.
                // We use the safe XOR approach or just clear the bit directly.
                // Resetting via XOR is clean.
                self.current_chunk &= !(1 << trailing);

                // For medium board, we need to handle the upper bits if it was > 64 bits?
                // Wait, Small(u32) is fine (fits in u64). Medium(u128) might be > 64 bits.
                // My logic for Medium below treats it as 2 chunks.

                // Correction for Medium(u128):
                // In `new`, I took `*b as u64`. That only grabs the lower 64 bits.
                // I need to handle the Medium case correctly in `next`.

                let index = if let BitBoard::Medium(_) = self.board {
                    self.current_chunk_idx * 64 + trailing as usize
                } else if let BitBoard::Large { .. } = self.board {
                    self.current_chunk_idx * 64 + trailing as usize
                } else {
                    // Small
                    trailing as usize
                };

                return Some(index);
            }

            // Move to next chunk if needed
            match self.board {
                BitBoard::Small(_) => return None, // Small only has 1 chunk (and we just processed it)
                BitBoard::Medium(b) => {
                    if self.current_chunk_idx == 0 {
                        self.current_chunk_idx = 1;
                        self.current_chunk = (b >> 64) as u64;
                    } else {
                        return None;
                    }
                }
                BitBoard::Large { data } => {
                    self.current_chunk_idx += 1;
                    if self.current_chunk_idx < data.len() {
                        self.current_chunk = data[self.current_chunk_idx];
                    } else {
                        return None;
                    }
                }
            }
        }
    }
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

    pub fn index_to_coords(&self, index: usize) -> SmallVec<[usize; 4]> {
        let mut coords = SmallVec::with_capacity(self.dimension);
        // Fill with zeros first then overwrite? Or verify strict logic.
        // Logic: coords[i] = temp % side...
        // We know dimension.
        coords.resize(self.dimension, 0);
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

    pub fn get_piece_at_index(&self, index: usize) -> Option<Piece> {
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

    pub fn apply_move(&mut self, mv: &Move) -> Result<UnmakeInfo, String> {
        let from_idx = self
            .coords_to_index(&mv.from.values)
            .ok_or("Invalid from")?;
        let to_idx = self.coords_to_index(&mv.to.values).ok_or("Invalid to")?;

        let moving_piece = self
            .get_piece_at_index(from_idx)
            .ok_or("No piece at from")?;

        // 0. Prepare Unmake Info
        let saved_ep = self.en_passant_target;
        let saved_castling = self.castling_rights;
        let mut captured = None;

        // Check if there is a piece at destination (Standard Capture)
        if let Some(target_p) = self.get_piece_at_index(to_idx) {
            captured = Some((to_idx, target_p));
        }

        self.history.push(self.hash);

        // 1. Check if this is an EP capture
        if moving_piece.piece_type == PieceType::Pawn {
            if let Some((target, victim)) = self.en_passant_target {
                if to_idx == target {
                    // EP Capture: Remove the victim pawn
                    // Capture info needs to be updated to the victim
                    if let Some(victim_p) = self.get_piece_at_index(victim) {
                        captured = Some((victim, victim_p));
                    }
                    self.remove_piece_at_index(victim);
                }
            }
        }

        // 2. Clear EP target for the next turn
        self.en_passant_target = None;

        // 3. Set EP target if double push
        if moving_piece.piece_type == PieceType::Pawn {
            let mut diffs = SmallVec::<[usize; 4]>::new();
            for i in 0..self.dimension {
                let d = (mv.from.values[i] as isize - mv.to.values[i] as isize).abs();
                diffs.push(d as usize);
            }

            let double_step_axis = diffs.iter().position(|&d| d == 2);
            let any_other_movement = diffs
                .iter()
                .enumerate()
                .any(|(i, &d)| i != double_step_axis.unwrap_or(999) && d != 0);

            if let Some(axis) = double_step_axis {
                if !any_other_movement {
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
        let mut castling_rook_move: Option<(usize, usize, Piece)> = None;

        // Update Rights if King Moves
        if moving_piece.piece_type == PieceType::King {
            match moving_piece.owner {
                Player::White => self.castling_rights &= !0x3,
                Player::Black => self.castling_rights &= !0xC,
            }
        }

        // Update Rights if Rook Moves/Captured (Side=8 enforced)
        if self.side == 8 {
            let w_rank = 0;
            let b_rank = 7;
            let mut w_qs_c: SmallVec<[usize; 4]> = smallvec![w_rank; self.dimension];
            w_qs_c[1] = 0;
            let mut w_ks_c: SmallVec<[usize; 4]> = smallvec![w_rank; self.dimension];
            w_ks_c[1] = 7;

            let mut b_qs_c: SmallVec<[usize; 4]> = smallvec![b_rank; self.dimension];
            b_qs_c[1] = 0;
            let mut b_ks_c: SmallVec<[usize; 4]> = smallvec![b_rank; self.dimension];
            b_ks_c[1] = 7;

            let w_qs = self.coords_to_index(&w_qs_c);
            let w_ks = self.coords_to_index(&w_ks_c);
            let b_qs = self.coords_to_index(&b_qs_c);
            let b_ks = self.coords_to_index(&b_ks_c);

            // Important: Logic check for rook capture or move.
            // If captured (to_idx), rights lost. If moved (from_idx), rights lost.
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
        // If standard capture, remove piece at to_idx.
        // If EP, we already removed victim.
        // If no capture, to_idx is empty.
        // NOTE: if captured is set, we need to ensure the piece is gone.
        // remove_piece_at_index(to_idx) is safe even if empty?
        // Yes, clears bits.
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

        Ok(UnmakeInfo {
            captured,
            en_passant_target: saved_ep,
            castling_rights: saved_castling,
        })
    }

    pub fn unmake_move(&mut self, mv: &Move, info: UnmakeInfo) {
        // Restore hash
        if let Some(h) = self.history.pop() {
            self.hash = h;
        }

        // Restore State
        self.en_passant_target = info.en_passant_target;
        self.castling_rights = info.castling_rights;

        let from_idx = self.coords_to_index(&mv.from.values).unwrap(); // Should exist
        let to_idx = self.coords_to_index(&mv.to.values).unwrap();

        // 1. In reverse: Handle Castling first? Or just move pieces back.
        // If it was Castling, move Rook back.
        // Same logic to detect castling from move.
        // Moving piece was likely King if castling. Checks...
        // We need 'moving_piece' type. usage of get_piece_at_index(to_idx)
        // after move applied gives us the piece. (Or promoted piece).

        // Actually, we can just check if Dist=2 for King logic.
        // But what if we don't know it was a King?
        // We can inspect the piece at `to_idx` before we move it back.
        let moved_piece = self
            .get_piece_at_index(to_idx)
            .expect("Piece missing in unmake");

        if moved_piece.piece_type == PieceType::King {
            let dist_file = (mv.from.values[1] as isize - mv.to.values[1] as isize).abs();
            let mut other_axes_moved = false;
            for i in 0..self.dimension {
                if i != 1 && mv.from.values[i] != mv.to.values[i] {
                    other_axes_moved = true;
                    break;
                }
            }
            if dist_file == 2 && !other_axes_moved {
                // Castling! Move rook back.
                // Inverse: Rook went from r_from -> r_to. We move r_to -> r_from.
                let is_kingside = mv.to.values[1] > mv.from.values[1];
                let rook_file_from = if is_kingside { 7 } else { 0 };
                let rook_file_to = if is_kingside { 5 } else { 3 };

                let mut rook_from_coords = mv.from.values.clone();
                rook_from_coords[1] = rook_file_from;
                let mut rook_to_coords = mv.from.values.clone();
                rook_to_coords[1] = rook_file_to;

                let r_from_idx = self.coords_to_index(&rook_from_coords).unwrap();
                let r_to_idx = self.coords_to_index(&rook_to_coords).unwrap();

                let rook_piece = self
                    .get_piece_at_index(r_to_idx)
                    .expect("Rook missing unmake");
                self.remove_piece_at_index(r_to_idx);
                self.place_piece_at_index(r_from_idx, rook_piece);
            }
        }

        // 2. Move Piece back (to -> from)
        self.remove_piece_at_index(to_idx);

        let original_piece = if mv.promotion.is_some() {
            Piece {
                piece_type: PieceType::Pawn,
                owner: moved_piece.owner,
            }
        } else {
            moved_piece
        };
        self.place_piece_at_index(from_idx, original_piece);

        // 3. Restore Captured
        if let Some((idx, piece)) = info.captured {
            self.place_piece_at_index(idx, piece);
        }
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
                // Determine the common length for vectorization
                let len = std::cmp::min(data.len(), other_data.len());

                // This slice-based loop allows LLVM to generate AVX2 instructions
                // (vpor ymm0, ymm0, ymm1)
                for (d, o) in data[0..len].iter_mut().zip(&other_data[0..len]) {
                    *d |= *o;
                }

                // Handle size mismatches (rare in static board dimensions)
                if other_data.len() > data.len() {
                    data.extend_from_slice(&other_data[len..]);
                }

                BitBoard::Large { data }
            }
            _ => panic!("Mismatched BitBoard types"),
        }
    }
    pub fn iter_indices(&self) -> BitIterator<'_> {
        BitIterator::new(self)
    }
}
