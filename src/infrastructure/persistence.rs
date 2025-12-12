use crate::domain::coordinate::Coordinate;
use crate::domain::models::{BoardState, Move, Piece, PieceType, Player};
use crate::infrastructure::zobrist::ZobristKeys;
use std::fmt;
use std::sync::Arc;

// Keep BitBoard implementation as is for storage
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BitBoard {
    Small(u32),
    Medium(u128),
    Large { data: Vec<u64> },
}

#[derive(Clone, Debug)]
pub struct BitBoardState {
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
}

impl BitBoardState {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let empty = BitBoard::new_empty(dimension, side);

        // Intentionally create a new ZobristKeys for each "Game" (or ideally shared, but this is safe)
        // For distinct games to have distinct hashes, we might want a shared seed?
        // But for per-game consistency, a new random set is fine as long as it persists for the game.
        // Actually, to make tests deterministic or to share across clones, this is fine.
        let zobrist = Arc::new(ZobristKeys::new(total_cells));
        let hash = 0; // Will be calculated after setup

        BitBoardState {
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
        }
    }

    // Helper to get raw index
    fn get_index(&self, coord: &Coordinate) -> Option<usize> {
        crate::infrastructure::persistence::coords_to_index(&coord.values, self.side)
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
        // Only populate one "slice" of pieces per side in N-dims.
        // Iterate only over the "file" dimension (dimension 1, typically 'y').
        // All other dimensions > 1 are fixed to 0 for White and side-1 for Black.

        for file_y in 0..self.side {
            // --- White Pieces (z=0, w=0, ...) ---
            let mut white_coords = vec![0; self.dimension];
            white_coords[1] = file_y;

            // Place Pawn at Rank 1
            white_coords[0] = 1;
            if let Some(idx) = self.get_index(&Coordinate::new(white_coords.clone())) {
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
            if let Some(idx) = self.get_index(&Coordinate::new(white_coords.clone())) {
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
                if let Some(idx) = self.get_index(&Coordinate::new(black_coords.clone())) {
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
            if let Some(idx) = self.get_index(&Coordinate::new(black_coords.clone())) {
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

        // Initial hash calculation
        self.hash = self.zobrist.get_hash(self, Player::White); // Assume White starts? BoardState doesn't track current player yet...
                                                                // Wait, BoardState is just state. It doesn't know whose turn it is implicitly unless we store it.
                                                                // Zobrist hash usually includes "side to move".
                                                                // For Minimax, we pass `player` in.
                                                                // But `apply_move` doesn't take `player` (it's implicit in the move or the flow).
                                                                // Let's assume standard chess setup implies White to move, but correct hashing depends on `player`.
                                                                // I will calculate a "Board Config" hash here, avoiding the "side to move" part if I don't know it,
                                                                // OR I will assume White to move for the initial setup.
                                                                // However, `ZobristKey::get_hash` takes `current_player`.
                                                                // Let's assume White for setup.
                                                                // When `apply_move` happens, we need to know who moved to flip the hash.
                                                                // `Move` struct doesn't have `player`? No, it's just from/to.
                                                                // But `BitBoardState::get_piece` gives owner.
                                                                // I can deduce the player from the moving piece!
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

        // Generic N-D heuristic
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
        // 3-fold repetition: simple count check
        // We only check for 2 previous occurrences + current = 3
        let count = self.history.iter().filter(|&&h| h == self.hash).count();
        // If it's in history 2 times already, current state makes it 3.
        // Wait, history stores *previous* states.
        // So checking if `self.hash` exists 2 times in history is 3-fold.
        // Even 1 recurrence is enough to trigger a draw claim in some rules?
        // Standard chess is 3-fold.
        // The user complained about "looping". Even 2-fold (revisiting once) is the start of a loop.
        // If I return true on ANY repetition, it forces strict progress.
        // Let's stick to strict repetition avoidance for now (1 previous occurrence) to kill the loop aggressively?
        // No, standard is 3-fold. But Minimax looks ahead.
        // If Minimax sees "I go A -> B -> A", it sees A in history ONCE.
        // If that is penalized, it won't do it.
        // So `count >= 1` is enough to penalize immediate "undo" moves or simple loops if we want to avoid them.
        // Let's try `count >= 1` (2-fold) first. It prevents "dancing".
        count >= 1
    }

    /// Recalculates hash fully. used for testing or re-sync.
    pub fn update_hash(&mut self, player_to_move: Player) {
        self.hash = self.zobrist.get_hash(self, player_to_move);
    }
}

impl BoardState for BitBoardState {
    fn new(dimension: usize, side: usize) -> Self {
        let mut board = Self::new_empty(dimension, side);
        board.setup_standard_chess(); // Call the helper method
        board
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn side(&self) -> usize {
        self.side
    }

    fn total_cells(&self) -> usize {
        self.total_cells
    }

    fn get_piece(&self, coord: &Coordinate) -> Option<Piece> {
        let index = self.get_index(coord)?;

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

    fn apply_move(&mut self, mv: &Move) -> Result<(), String> {
        let from_idx = self.get_index(&mv.from).ok_or("Invalid From coord")?;
        let to_idx = self.get_index(&mv.to).ok_or("Invalid To coord")?;

        let moving_piece = self.get_piece(&mv.from).ok_or("No piece at origin")?;

        // Push current hash to history BEFORE modifying state
        self.history.push(self.hash);

        // 1. Remove piece from 'from'
        self.remove_piece_at_index(from_idx);

        // 2. Remove any piece at 'to' (Capture)
        self.remove_piece_at_index(to_idx);

        // 3. Place piece at 'to'
        let piece_to_place = if let Some(promo_type) = mv.promotion {
            Piece {
                piece_type: promo_type,
                owner: moving_piece.owner,
            }
        } else {
            moving_piece
        };

        self.place_piece_at_index(to_idx, piece_to_place);

        // Update Hash
        // We know the player who moved is `moving_piece.owner`.
        // The NEXT player is `moving_piece.owner.opponent()`.
        // Zobrist hash depends on "side to move".
        // Use the opponent as the side to move for the NEW state.
        self.hash = self.zobrist.get_hash(self, moving_piece.owner.opponent());

        Ok(())
    }

    fn get_king_coordinate(&self, player: Player) -> Option<Coordinate> {
        let occupancy = match player {
            Player::White => &self.white_occupancy,
            Player::Black => &self.black_occupancy,
        };

        for i in 0..self.total_cells {
            if occupancy.get_bit(i) && self.kings.get_bit(i) {
                return Some(Coordinate::new(index_to_coords(
                    i,
                    self.dimension,
                    self.side,
                )));
            }
        }
        None
    }

    fn set_piece(&mut self, coord: &Coordinate, piece: Piece) -> Result<(), String> {
        let index = self.get_index(coord).ok_or("Invalid coord")?;
        self.remove_piece_at_index(index);
        self.place_piece_at_index(index, piece);
        // Note: set_piece is usually for setup. We should probably invalidate history or just update hash.
        // For debugging/setup, let's just update hash assuming White to move default?
        // Or leave it stale. safer to update if possible.
        // Let's assume White to move for arbitrary set_piece calls unless specified.
        self.hash = self.zobrist.get_hash(self, Player::White);
        Ok(())
    }

    fn clear_cell(&mut self, coord: &Coordinate) {
        if let Some(index) = self.get_index(coord) {
            self.remove_piece_at_index(index);
            self.hash = self.zobrist.get_hash(self, Player::White);
        }
    }

    fn check_status(&self, _player_to_move: Player) -> crate::domain::models::GameResult {
        crate::domain::models::GameResult::InProgress
    }
}

impl fmt::Display for BitBoardState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", crate::infrastructure::display::render_board(self))
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
            BitBoard::Medium(b) => *b |= 1 << index,
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

pub fn index_to_coords(index: usize, dimension: usize, side: usize) -> Vec<usize> {
    let mut coords = vec![0; dimension];
    let mut temp = index;
    for i in 0..dimension {
        coords[i] = temp % side;
        temp /= side;
    }
    coords
}

pub fn coords_to_index(coords: &[usize], side: usize) -> Option<usize> {
    let mut index = 0;
    let mut multiplier = 1;
    for &c in coords {
        if c >= side {
            return None;
        }
        index += c * multiplier;
        multiplier *= side;
    }
    Some(index)
}
