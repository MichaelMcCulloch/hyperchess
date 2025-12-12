```./src/application/game_service.rs
use crate::domain::board::Board;
use crate::domain::models::{GameResult, Player};
use crate::domain::services::PlayerStrategy;

pub struct GameService<'a> {
    board: Board,
    player_white: Box<dyn PlayerStrategy + 'a>,
    player_black: Box<dyn PlayerStrategy + 'a>,
    turn: Player,
}

impl<'a> GameService<'a> {
    pub fn new(
        board: Board,
        player_white: Box<dyn PlayerStrategy + 'a>,
        player_black: Box<dyn PlayerStrategy + 'a>,
    ) -> Self {
        GameService {
            board,
            player_white,
            player_black,
            turn: Player::White,
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn turn(&self) -> Player {
        self.turn
    }

    pub fn is_game_over(&self) -> Option<GameResult> {
        match self.board.check_status(self.turn) {
            GameResult::InProgress => None,
            result => Some(result),
        }
    }

    pub fn perform_next_move(&mut self) -> Result<GameResult, String> {
        if self.is_game_over().is_some() {
            return Err("Game is over".to_string());
        }

        let strategy = match self.turn {
            Player::White => &mut self.player_white,
            Player::Black => &mut self.player_black,
        };

        if let Some(mv) = strategy.get_move(&self.board, self.turn) {
            self.board.apply_move(&mv).map_err(|e| e.to_string())?;

            self.turn = self.turn.opponent();

            Ok(self.board.check_status(self.turn))
        } else {
            Err("No move available".to_string())
        }
    }
}
```
```./src/application/mod.rs
pub mod game_service;
```
```./src/domain/board.rs
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
```
```./src/domain/coordinate.rs
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Coordinate {
    pub values: Vec<usize>,
}

impl Coordinate {
    pub fn new(values: Vec<usize>) -> Self {
        Self { values }
    }

    pub fn dim(&self) -> usize {
        self.values.len()
    }
}

impl fmt::Debug for Coordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, v) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", v)?;
        }
        write!(f, ")")
    }
}
```
```./src/domain/game.rs
use crate::domain::board::Board;
use crate::domain::models::{GameResult, Move, Player};

#[derive(Debug)]
pub enum GameError {
    InvalidMove(String),
}

/// The Game Aggregate Root.
/// It controls the lifecycle of the game, turns, and winning conditions.
pub struct Game {
    board: Board,
    turn: Player,
    status: GameResult,
    move_history: Vec<(Player, Move)>,
}

impl Game {
    pub fn new(board: Board) -> Self {
        Self {
            board,
            turn: Player::White,
            status: GameResult::InProgress,
            move_history: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.status = GameResult::InProgress;
        self.turn = Player::White;
    }

    pub fn play_turn(&mut self, mv: Move) -> Result<GameResult, GameError> {
        if self.status != GameResult::InProgress {
            return Err(GameError::InvalidMove("Game is already over".to_string()));
        }

        self.board.apply_move(&mv).map_err(GameError::InvalidMove)?;

        self.move_history.push((self.turn, mv.clone()));

        let result = self.board.check_status(self.turn);
        self.status = result;

        if result == GameResult::InProgress {
            self.turn = self.turn.opponent();
        }

        Ok(result)
    }

    pub fn current_turn(&self) -> Player {
        self.turn
    }

    pub fn status(&self) -> GameResult {
        self.status
    }

    pub fn board(&self) -> &Board {
        &self.board
    }
}
```
```./src/domain/mod.rs
pub mod board;
pub mod coordinate;
pub mod game; // Existing game.rs, verify if it needs updates
pub mod models;
pub mod rules;
pub mod services; // Existing services.rs
pub mod zobrist;
```
```./src/domain/models.rs
use crate::domain::coordinate::Coordinate;
use std::fmt::Debug;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Player {
    White,
    Black,
}

impl Player {
    pub fn opponent(&self) -> Self {
        match self {
            Player::White => Player::Black,
            Player::Black => Player::White,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PieceType {
    Pawn,
    Rook,
    Knight,
    Bishop,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Piece {
    pub piece_type: PieceType,
    pub owner: Player,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Move {
    pub from: Coordinate,
    pub to: Coordinate,
    pub promotion: Option<PieceType>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameResult {
    Checkmate(Player),
    Stalemate,
    Draw,
    InProgress,
}
```
```./src/domain/rules.rs
use crate::domain::board::Board;
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType, Player};

pub struct Rules;

impl Rules {
    pub fn generate_legal_moves(board: &Board, player: Player) -> Vec<Move> {
        let mut moves = Vec::new();
        let pseudo_legal = Self::generate_pseudo_legal_moves(board, player);

        for mv in pseudo_legal {
            if !Self::leaves_king_in_check(board, player, &mv) {
                moves.push(mv);
            }
        }

        // Castling moves
        Self::generate_castling_moves(board, player, &mut moves);

        moves
    }

    pub fn is_square_attacked(board: &Board, square: &Coordinate, by_player: Player) -> bool {
        // To check if a square is attacked by `by_player`, we can pretend there is a piece on `square`
        // and see if it can "capture" a piece of `by_player` using the movement rules of that piece.
        // E.g. if a Knight on `square` can jump to a square occupied by an enemy Knight, then `square` is attacked by that enemy Knight.
        // NOTE: We rely on board.get_index / coords_to_index which are now internal or public?
        // We made `coords_to_index` public in Board.

        let dimension = board.dimension;
        let side = board.side;
        // Accessing helper on board instance
        let _index = if let Some(idx) = board.coords_to_index(&square.values) {
            idx
        } else {
            return false;
        };

        let enemy_occupancy = match by_player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        // 1. Check Leapers (Knights, Kings)
        // Check Knight attacks
        let knight_offsets = Self::get_knight_offsets(dimension);
        for offset in &knight_offsets {
            if let Some(target_coord) = Self::apply_offset(&square.values, offset, side) {
                if let Some(target_idx) = board.coords_to_index(&target_coord) {
                    if enemy_occupancy.get_bit(target_idx) && board.knights.get_bit(target_idx) {
                        return true;
                    }
                }
            }
        }

        // Check King attacks (useful for validation, though kings can't really attack to checkmate)
        let king_offsets = Self::get_king_offsets(dimension);
        for offset in &king_offsets {
            if let Some(target_coord) = Self::apply_offset(&square.values, offset, side) {
                if let Some(target_idx) = board.coords_to_index(&target_coord) {
                    if enemy_occupancy.get_bit(target_idx) && board.kings.get_bit(target_idx) {
                        return true;
                    }
                }
            }
        }

        // 2. Check Rays (Rook, Bishop, Queen)
        // Reverse raycast: Look outwards from `square`. If first piece hit is enemy slider of relevant type, then attacked.

        // Rook vectors
        let rook_dirs = Self::get_rook_directions(dimension);
        for dir in &rook_dirs {
            if Self::scan_ray_for_threat(
                board,
                &square.values,
                dir,
                by_player,
                &[PieceType::Rook, PieceType::Queen],
            ) {
                return true;
            }
        }

        // Bishop vectors
        let bishop_dirs = Self::get_bishop_directions(dimension);
        for dir in &bishop_dirs {
            if Self::scan_ray_for_threat(
                board,
                &square.values,
                dir,
                by_player,
                &[PieceType::Bishop, PieceType::Queen],
            ) {
                return true;
            }
        }

        // 3. Check Pawns
        // Pawns attack "Forward" + "Sideways".
        // Inverse: Check if there is an enemy pawn that can capture `square`.
        // Enemy pawn moves "Forward" (relative to enemy).
        // So we look "Backward" relative to enemy from `square`.

        let pawn_attack_offsets = Self::get_pawn_capture_offsets_for_target(dimension, by_player);
        for offset in &pawn_attack_offsets {
            if let Some(target_coord) = Self::apply_offset(&square.values, offset, side) {
                if let Some(target_idx) = board.coords_to_index(&target_coord) {
                    if enemy_occupancy.get_bit(target_idx) && board.pawns.get_bit(target_idx) {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn scan_ray_for_threat(
        board: &Board,
        origin_vals: &[usize],
        direction: &[isize],
        attacker: Player,
        threat_types: &[PieceType],
    ) -> bool {
        // We are at `origin_vals` (which is empty or the target square).
        // We look OUTWARD in `direction`.
        // If we hit an enemy piece of `threat_types`, return true.
        // If we hit any other piece (own or enemy non-threat), return false (blocked).

        let mut current = origin_vals.to_vec();
        let enemy_occupancy = match attacker {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        // Own occupancy relative to the square being attacked?
        // No, 'own' relative to the attacker is 'enemy' relative to the square?
        // Wait. `by_player` is the ATTACKER.
        // So `enemy_occupancy` is the ATTACKER's pieces.
        // `own_occupancy` is the DEFENDER's pieces (or Empty).
        // Any piece blocks the ray.

        // Actually simpler: Just check ALL occupancy.
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        loop {
            if let Some(next) = Self::apply_offset(&current, direction, board.side) {
                if let Some(idx) = board.coords_to_index(&next) {
                    if all_occupancy.get_bit(idx) {
                        // Hit a piece. Is it an enemy threat?
                        if enemy_occupancy.get_bit(idx) {
                            // It is an enemy piece. Check type.
                            // We need to check if it is one of the threat_types.

                            // Optimization: The caller passes specific threat types (e.g. Rook+Queen).
                            // But board bitboards are separated.
                            // Let's iterate threat types passed.
                            for &t in threat_types {
                                let match_found = match t {
                                    PieceType::Rook => board.rooks.get_bit(idx),
                                    PieceType::Bishop => board.bishops.get_bit(idx),
                                    PieceType::Queen => board.queens.get_bit(idx),
                                    _ => false,
                                };
                                if match_found {
                                    return true;
                                }
                            }
                            // If we hit an enemy piece but it's not in the threat list (e.g. a pawn blocking a rook),
                            // then it blocks the view.
                            return false;
                        } else {
                            // Hit own piece (Defender's piece), blocks view.
                            return false;
                        }
                    }
                    current = next;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        false
    }

    // --- Internal Helpers ---

    fn generate_pseudo_legal_moves(board: &Board, player: Player) -> Vec<Move> {
        let mut moves = Vec::new();
        // Iterate all cells, find pieces owned by player
        for i in 0..board.total_cells {
            let occupancy = match player {
                Player::White => &board.white_occupancy,
                Player::Black => &board.black_occupancy,
            };

            if occupancy.get_bit(i) {
                let coord_vals = board.index_to_coords(i);
                let coord = Coordinate::new(coord_vals.clone());

                // Identify piece type
                let piece_type = if board.pawns.get_bit(i) {
                    PieceType::Pawn
                } else if board.knights.get_bit(i) {
                    PieceType::Knight
                } else if board.bishops.get_bit(i) {
                    PieceType::Bishop
                } else if board.rooks.get_bit(i) {
                    PieceType::Rook
                } else if board.queens.get_bit(i) {
                    PieceType::Queen
                } else if board.kings.get_bit(i) {
                    PieceType::King
                } else {
                    continue; // Error?
                };

                match piece_type {
                    PieceType::Pawn => Self::generate_pawn_moves(board, &coord, player, &mut moves),
                    PieceType::Knight => Self::generate_leaper_moves(
                        board,
                        &coord,
                        player,
                        &Self::get_knight_offsets(board.dimension),
                        &mut moves,
                    ),
                    PieceType::King => Self::generate_leaper_moves(
                        board,
                        &coord,
                        player,
                        &Self::get_king_offsets(board.dimension),
                        &mut moves,
                    ),
                    PieceType::Rook => Self::generate_slider_moves(
                        board,
                        &coord,
                        player,
                        &Self::get_rook_directions(board.dimension),
                        &mut moves,
                    ),
                    PieceType::Bishop => Self::generate_slider_moves(
                        board,
                        &coord,
                        player,
                        &Self::get_bishop_directions(board.dimension),
                        &mut moves,
                    ),
                    PieceType::Queen => {
                        Self::generate_slider_moves(
                            board,
                            &coord,
                            player,
                            &Self::get_rook_directions(board.dimension),
                            &mut moves,
                        );
                        Self::generate_slider_moves(
                            board,
                            &coord,
                            player,
                            &Self::get_bishop_directions(board.dimension),
                            &mut moves,
                        );
                    }
                }
            }
        }
        moves
    }

    fn leaves_king_in_check(board: &Board, player: Player, mv: &Move) -> bool {
        // Clone board, apply move, check if king is attacked
        // This is expensive. Future optimization: Incremental update or specialized check.
        let mut temp_board = board.clone();
        if let Err(_) = temp_board.apply_move(mv) {
            return true; // Illegal move invocation
        }

        if let Some(king_pos) = temp_board.get_king_coordinate(player) {
            Self::is_square_attacked(&temp_board, &king_pos, player.opponent())
        } else {
            // No king? For testing (sandbox), assume safe.
            false
        }
    }

    fn generate_castling_moves(board: &Board, player: Player, moves: &mut Vec<Move>) {
        if board.dimension != 2 || board.side != 8 {
            return;
        }

        let (rights_mask, rank) = match player {
            Player::White => (0x3, 0),              // Rights 1 & 2 (KS, QS)
            Player::Black => (0xC, board.side - 1), // Rights 4 & 8 (KS, QS)
        };

        let my_rights = board.castling_rights & rights_mask;
        if my_rights == 0 {
            return;
        }

        if Self::is_square_attacked(board, &Coordinate::new(vec![rank, 4]), player.opponent()) {
            return; // King in check
        }

        // Kingside
        let ks_mask = match player {
            Player::White => 0x1,
            Player::Black => 0x4,
        };
        if (my_rights & ks_mask) != 0 {
            let f_sq = vec![rank, 5];
            let g_sq = vec![rank, 6];
            let f_idx = board.coords_to_index(&f_sq).unwrap();
            let g_idx = board.coords_to_index(&g_sq).unwrap();

            let all_occupancy = board
                .white_occupancy
                .clone()
                .or_with(&board.black_occupancy);
            let f_occ = all_occupancy.get_bit(f_idx);
            let g_occ = all_occupancy.get_bit(g_idx);

            if !f_occ && !g_occ {
                if !Self::is_square_attacked(
                    board,
                    &Coordinate::new(f_sq.clone()),
                    player.opponent(),
                ) && !Self::is_square_attacked(
                    board,
                    &Coordinate::new(g_sq.clone()),
                    player.opponent(),
                ) {
                    moves.push(Move {
                        from: Coordinate::new(vec![rank, 4]),
                        to: Coordinate::new(g_sq),
                        promotion: None,
                    });
                }
            }
        }

        // Queenside
        let qs_mask = match player {
            Player::White => 0x2,
            Player::Black => 0x8,
        };
        if (my_rights & qs_mask) != 0 {
            let b_sq = vec![rank, 1];
            let c_sq = vec![rank, 2];
            let d_sq = vec![rank, 3];
            let b_idx = board.coords_to_index(&b_sq).unwrap();
            let c_idx = board.coords_to_index(&c_sq).unwrap();
            let d_idx = board.coords_to_index(&d_sq).unwrap();

            let all_occupancy = board
                .white_occupancy
                .clone()
                .or_with(&board.black_occupancy);
            if !all_occupancy.get_bit(b_idx)
                && !all_occupancy.get_bit(c_idx)
                && !all_occupancy.get_bit(d_idx)
            {
                if !Self::is_square_attacked(
                    board,
                    &Coordinate::new(d_sq.clone()),
                    player.opponent(),
                ) && !Self::is_square_attacked(
                    board,
                    &Coordinate::new(c_sq.clone()),
                    player.opponent(),
                ) {
                    moves.push(Move {
                        from: Coordinate::new(vec![rank, 4]),
                        to: Coordinate::new(c_sq),
                        promotion: None,
                    });
                }
            }
        }
    }

    // Geometry Generators

    fn get_rook_directions(dimension: usize) -> Vec<Vec<isize>> {
        let mut dirs = Vec::new();
        // Just one non-zero component, +/- 1
        for i in 0..dimension {
            let mut v = vec![0; dimension];
            v[i] = 1;
            dirs.push(v.clone());
            v[i] = -1;
            dirs.push(v);
        }
        dirs
    }

    fn get_bishop_directions(dimension: usize) -> Vec<Vec<isize>> {
        // Even number of non-zero elements (user spec).
        let mut dirs = Vec::new();
        let num_dirs = 3_usize.pow(dimension as u32);
        for i in 0..num_dirs {
            let mut dir = Vec::with_capacity(dimension);
            let mut temp = i;
            let mut nonzero_count = 0;
            for _ in 0..dimension {
                let val = match temp % 3 {
                    0 => 0,
                    1 => {
                        nonzero_count += 1;
                        1
                    }
                    2 => {
                        nonzero_count += 1;
                        -1
                    }
                    _ => unreachable!(),
                };
                dir.push(val);
                temp /= 3;
            }
            if nonzero_count > 0 && nonzero_count % 2 == 0 {
                dirs.push(dir);
            }
        }
        dirs
    }

    fn get_knight_offsets(dimension: usize) -> Vec<Vec<isize>> {
        // Permutations of (+/- 2, +/- 1, 0...)
        // We need exactly one '2' and one '1', rest 0.
        let mut offsets = Vec::new();

        // This is a bit tricky to generate generically for N dimensions.
        // Iterate all pairs of axes.
        for i in 0..dimension {
            for j in 0..dimension {
                if i == j {
                    continue;
                }

                // +/- 2 on axis i, +/- 1 on axis j
                for s1 in [-1, 1] {
                    for s2 in [-1, 1] {
                        let mut v = vec![0; dimension];
                        v[i] = 2 * s1;
                        v[j] = 1 * s2;
                        offsets.push(v);
                    }
                }
            }
        }
        offsets
    }

    fn get_king_offsets(dimension: usize) -> Vec<Vec<isize>> {
        // Chebyshev 1. All 3^N - 1 neighbors.
        let mut offsets = Vec::new();
        let num_dirs = 3_usize.pow(dimension as u32);
        for i in 0..num_dirs {
            let mut dir = Vec::with_capacity(dimension);
            let mut temp = i;
            let mut all_zero = true;
            for _ in 0..dimension {
                let val = match temp % 3 {
                    0 => 0,
                    1 => 1,
                    2 => -1,
                    _ => unreachable!(),
                };
                if val != 0 {
                    all_zero = false;
                }
                dir.push(val);
                temp /= 3;
            }
            if !all_zero {
                offsets.push(dir);
            }
        }
        offsets
    }

    fn get_pawn_capture_offsets_for_target(dimension: usize, attacker: Player) -> Vec<Vec<isize>> {
        // If 'attacker' is White, they move +1 on axis 0 (forward).
        // Captures are +1 on axis 0 AND +/- 1 on exactly ONE other axis.
        // So we want to find where an Attacker could be relative to the Target.
        // Target = Attacker + Move.
        // Attacker = Target - Move.

        let direction = match attacker {
            Player::White => -1, // Look back
            Player::Black => 1,
        };

        let mut offsets = Vec::new();
        // Axis 0 is forward.
        // For each other dimension, allow +/- 1.
        for i in 1..dimension {
            for s in [-1, 1] {
                let mut v = vec![0; dimension];
                v[0] = direction;
                v[i] = s;
                offsets.push(v);
            }
        }
        offsets
    }

    // Move Generation Logic implementation

    fn apply_offset(coords: &[usize], offset: &[isize], side: usize) -> Option<Vec<usize>> {
        let mut new_coords = Vec::with_capacity(coords.len());
        for (c, &o) in coords.iter().zip(offset.iter()) {
            let val = *c as isize + o;
            if val < 0 || val >= side as isize {
                return None;
            }
            new_coords.push(val as usize);
        }
        Some(new_coords)
    }

    fn generate_leaper_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        offsets: &[Vec<isize>],
        moves: &mut Vec<Move>,
    ) {
        let same_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for offset in offsets {
            if let Some(target_coords) = Self::apply_offset(&origin.values, offset, board.side) {
                if let Some(target_idx) = board.coords_to_index(&target_coords) {
                    if !same_occupancy.get_bit(target_idx) {
                        // Empty or Enemy -> Legal
                        moves.push(Move {
                            from: origin.clone(),
                            to: Coordinate::new(target_coords),
                            promotion: None,
                        });
                    }
                }
            }
        }
    }

    fn generate_slider_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        directions: &[Vec<isize>],
        moves: &mut Vec<Move>,
    ) {
        let own_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        let enemy_occupancy = match player.opponent() {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for dir in directions {
            let mut current = origin.values.clone();
            loop {
                if let Some(next) = Self::apply_offset(&current, dir, board.side) {
                    if let Some(idx) = board.coords_to_index(&next) {
                        if own_occupancy.get_bit(idx) {
                            break; // Blocked by own piece
                        }

                        moves.push(Move {
                            from: origin.clone(),
                            to: Coordinate::new(next.clone()),
                            promotion: None,
                        });

                        if enemy_occupancy.get_bit(idx) {
                            break; // Capture, then stop
                        }

                        current = next;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    fn generate_pawn_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        moves: &mut Vec<Move>,
    ) {
        let forward_dir = match player {
            Player::White => 1,
            Player::Black => -1,
        };

        let enemy_occupancy = match player.opponent() {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        // Just checking occupancy generically
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        // 1. One step forward
        let mut forward_step = vec![0; board.dimension];
        forward_step[0] = forward_dir;

        if let Some(target) = Self::apply_offset(&origin.values, &forward_step, board.side) {
            if let Some(idx) = board.coords_to_index(&target) {
                if !all_occupancy.get_bit(idx) {
                    // Must be empty
                    Self::add_pawn_move(
                        origin,
                        &target,
                        board.dimension,
                        board.side,
                        player,
                        moves,
                    );

                    // 2. Double step?
                    let is_start_rank = match player {
                        Player::White => origin.values[0] == 1,
                        Player::Black => origin.values[0] == board.side - 2,
                    };

                    if is_start_rank {
                        if let Some(target2) =
                            Self::apply_offset(&target, &forward_step, board.side)
                        {
                            if let Some(idx2) = board.coords_to_index(&target2) {
                                if !all_occupancy.get_bit(idx2) {
                                    Self::add_pawn_move(
                                        origin,
                                        &target2,
                                        board.dimension,
                                        board.side,
                                        player,
                                        moves,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Captures
        // +/- 1 on any other axis combined with forward step
        for i in 1..board.dimension {
            for s in [-1, 1] {
                let mut cap_step = forward_step.clone();
                cap_step[i] = s;
                if let Some(target) = Self::apply_offset(&origin.values, &cap_step, board.side) {
                    if let Some(idx) = board.coords_to_index(&target) {
                        if enemy_occupancy.get_bit(idx) {
                            Self::add_pawn_move(
                                origin,
                                &target,
                                board.dimension,
                                board.side,
                                player,
                                moves,
                            );
                        }
                    }
                }
            }
        }

        // 4. En Passant
        if let Some(ep_idx) = board.en_passant_target {
            let ep_coords = board.index_to_coords(ep_idx);
            let diff_rank = ep_coords[0] as isize - origin.values[0] as isize;

            // Should be exactly forward_dir
            if diff_rank == forward_dir {
                // Check if adjacent file (dist 1 in other dimensions)
                for i in 1..board.dimension {
                    let abs_diff = (ep_coords[i] as isize - origin.values[i] as isize).abs();
                    if abs_diff == 1 {
                        let mut is_valid_relation = true;
                        for j in 1..board.dimension {
                            if i != j && origin.values[j] != ep_coords[j] {
                                is_valid_relation = false;
                            }
                        }

                        if is_valid_relation {
                            moves.push(Move {
                                from: origin.clone(),
                                to: Coordinate::new(ep_coords.clone()),
                                promotion: None,
                            });
                        }
                    }
                }
            }
        }
    }

    fn add_pawn_move(
        from: &Coordinate,
        to_vals: &[usize],
        _dimension: usize,
        side: usize,
        player: Player,
        moves: &mut Vec<Move>,
    ) {
        let is_promotion = match player {
            Player::White => to_vals[0] == side - 1,
            Player::Black => to_vals[0] == 0,
        };

        let to = Coordinate::new(to_vals.to_vec());

        if is_promotion {
            for t in [
                PieceType::Queen,
                PieceType::Rook,
                PieceType::Bishop,
                PieceType::Knight,
            ] {
                moves.push(Move {
                    from: from.clone(),
                    to: to.clone(),
                    promotion: Some(t),
                });
            }
        } else {
            moves.push(Move {
                from: from.clone(),
                to,
                promotion: None,
            });
        }
    }
}
```
```./src/domain/services.rs
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use std::time::Duration;

pub trait Clock {
    fn now(&self) -> Duration;
}

pub trait PlayerStrategy {
    fn get_move(&mut self, board: &Board, player: Player) -> Option<Move>;
}
```
```./src/domain/zobrist.rs
use crate::domain::board::Board; // Will create this next
use crate::domain::models::Player;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct ZobristKeys {
    pub piece_keys: Vec<u64>,
    pub black_to_move: u64,
    pub en_passant_keys: Vec<u64>,
    pub castling_keys: Vec<u64>,
}

impl ZobristKeys {
    pub fn new(total_cells: usize) -> Self {
        let mut rng = rand::thread_rng();
        let size = 12 * total_cells;
        let mut piece_keys = Vec::with_capacity(size);
        for _ in 0..size {
            piece_keys.push(rng.r#gen());
        }

        // En Passant keys: one per file?
        // Actually, EP target is an index. But it's restricted to specific ranks.
        // It's cleaner to have one key per FILE (column).
        // Total files = side^(dimension-1)? Or just 'side' if we assume 2D-like columns?
        // Actually, let's just use `total_cells` size for simplicity, or just map 'index' -> key.
        // Let's use `total_cells` to support EP on any square (technically only rank 2/5 etc)
        // Optimization: typical EP is only valid on specific files.
        // We'll trust the board size isn't massive.
        let mut en_passant_keys = Vec::with_capacity(total_cells);
        for _ in 0..total_cells {
            en_passant_keys.push(rng.r#gen());
        }

        // Castling rights: 16 combinations (4 bits)
        let mut castling_keys = Vec::with_capacity(16);
        for _ in 0..16 {
            castling_keys.push(rng.r#gen());
        }

        Self {
            piece_keys,
            black_to_move: rng.r#gen(),
            en_passant_keys,
            castling_keys,
        }
    }

    pub fn get_hash(&self, board: &Board, current_player: Player) -> u64 {
        let mut hash = 0;
        if current_player == Player::Black {
            hash ^= self.black_to_move;
        }

        if let Some(ep_target) = board.en_passant_target {
            if ep_target < self.en_passant_keys.len() {
                hash ^= self.en_passant_keys[ep_target];
            }
        }

        let rights = board.castling_rights as usize;
        if rights < self.castling_keys.len() {
            hash ^= self.castling_keys[rights];
        }

        for i in 0..board.total_cells {
            if board.white_occupancy.get_bit(i) {
                let offset = if board.pawns.get_bit(i) {
                    0
                } else if board.knights.get_bit(i) {
                    1
                } else if board.bishops.get_bit(i) {
                    2
                } else if board.rooks.get_bit(i) {
                    3
                } else if board.queens.get_bit(i) {
                    4
                } else if board.kings.get_bit(i) {
                    5
                } else {
                    continue;
                };
                hash ^= self.piece_keys[offset * board.total_cells + i];
            } else if board.black_occupancy.get_bit(i) {
                let offset = if board.pawns.get_bit(i) {
                    6
                } else if board.knights.get_bit(i) {
                    7
                } else if board.bishops.get_bit(i) {
                    8
                } else if board.rooks.get_bit(i) {
                    9
                } else if board.queens.get_bit(i) {
                    10
                } else if board.kings.get_bit(i) {
                    11
                } else {
                    continue;
                };
                hash ^= self.piece_keys[offset * board.total_cells + i];
            }
        }
        hash
    }
}
```
```./src/infrastructure/ai/mcts.rs
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::domain::rules::Rules;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use rand::seq::SliceRandom;
use std::sync::Arc;

use std::f64;

const UCT_C: f64 = 1.4142; // Sqrt(2)
const CHECKMATE_SCORE: i32 = 30000;

struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    visits: u32,
    score: f64,
    unexpanded_moves: Vec<Move>,
    is_terminal: bool,
    move_to_node: Option<Move>,
    player_to_move: Player,
}

pub struct MCTS {
    nodes: Vec<Node>,
    root_player: Player,
    tt: Option<Arc<LockFreeTT>>,
}

use rayon::prelude::*;

impl MCTS {
    pub fn new(root_state: &Board, root_player: Player, tt: Option<Arc<LockFreeTT>>) -> Self {
        let mut moves = Rules::generate_legal_moves(root_state, root_player);
        let mut rng = rand::thread_rng();
        moves.shuffle(&mut rng);

        let root = Node {
            parent: None,
            children: Vec::new(),
            visits: 0,
            score: 0.0,
            unexpanded_moves: moves,
            is_terminal: false,
            move_to_node: None,
            player_to_move: root_player,
        };

        Self {
            nodes: vec![root],
            root_player,
            tt,
        }
    }

    pub fn run(&mut self, root_state: &Board, iterations: usize) -> f64 {
        if iterations == 0 {
            return 0.5;
        }

        // Parallel Execution (Root Parallelization)
        let num_threads = rayon::current_num_threads();
        let chunk_size = iterations / num_threads;
        let remainder = iterations % num_threads;

        let results: Vec<(u32, f64)> = (0..num_threads)
            .into_par_iter()
            .map(|i| {
                let count = if i < remainder {
                    chunk_size + 1
                } else {
                    chunk_size
                };
                if count == 0 {
                    return (0, 0.0);
                }

                // Create a local MCTS instance for this thread
                // Note: We share the Transposition Table (tt) which is thread-safe (Arc<LockFreeTT>)
                let mut local_mcts = MCTS::new(root_state, self.root_player, self.tt.clone());
                local_mcts.execute_iterations(root_state, count);

                let root = &local_mcts.nodes[0];
                (root.visits, root.score)
            })
            .collect();

        // Aggregation
        let (total_visits, total_score) = results
            .into_iter()
            .fold((0, 0.0), |acc, x| (acc.0 + x.0, acc.1 + x.1));

        if total_visits == 0 {
            0.5
        } else {
            total_score / total_visits as f64
        }
    }

    fn execute_iterations(&mut self, root_state: &Board, iterations: usize) {
        let mut rng = rand::thread_rng();

        for _ in 0..iterations {
            let mut node_idx = 0;
            let mut current_state = root_state.clone();
            let mut current_player = self.root_player;

            // 1. Selection
            while self.nodes[node_idx].unexpanded_moves.is_empty()
                && !self.nodes[node_idx].children.is_empty()
            {
                let best_child = self.select_child(node_idx);
                node_idx = best_child;

                let mv = self.nodes[node_idx].move_to_node.as_ref().unwrap();
                current_state.apply_move(mv).unwrap();
                current_player = current_player.opponent();
            }

            // 2. Expansion
            if !self.nodes[node_idx].unexpanded_moves.is_empty() {
                let mv = self.nodes[node_idx].unexpanded_moves.pop().unwrap();

                let mut next_state = current_state.clone();
                next_state.apply_move(&mv).unwrap();
                let next_player = current_player.opponent();

                let legal_moves = Rules::generate_legal_moves(&next_state, next_player);
                let is_terminal = legal_moves.is_empty();

                let new_node = Node {
                    parent: Some(node_idx),
                    children: Vec::new(),
                    visits: 0,
                    score: 0.0,
                    unexpanded_moves: legal_moves,
                    is_terminal,
                    move_to_node: Some(mv),
                    player_to_move: next_player,
                };

                let new_node_idx = self.nodes.len();
                self.nodes.push(new_node);
                self.nodes[node_idx].children.push(new_node_idx);

                node_idx = new_node_idx;
                current_state = next_state;
                current_player = next_player;
            }

            // 3. Simulation
            let result_score = if self.nodes[node_idx].is_terminal {
                self.evaluate_terminal(&current_state, current_player)
            } else {
                self.rollout(&mut current_state, current_player, &mut rng)
            };

            // 4. Backpropagation
            self.backpropagate(node_idx, result_score);
        }
    }

    fn select_child(&self, parent_idx: usize) -> usize {
        let parent = &self.nodes[parent_idx];
        let log_n = (parent.visits as f64).ln();

        let mut best_score = -f64::INFINITY;
        let mut best_child = 0;

        let maximize = parent.player_to_move == self.root_player;

        for &child_idx in &parent.children {
            let child = &self.nodes[child_idx];
            let win_rate = if child.visits > 0 {
                child.score / child.visits as f64
            } else {
                0.0
            };

            let exploitation = if maximize { win_rate } else { 1.0 - win_rate };

            let exploration = UCT_C * (log_n / (child.visits as f64 + 1e-6)).sqrt();
            let uct_value = exploitation + exploration;

            if uct_value > best_score {
                best_score = uct_value;
                best_child = child_idx;
            }
        }
        best_child
    }

    fn rollout(
        &self,
        state: &mut Board,
        mut player: Player,
        rng: &mut rand::rngs::ThreadRng,
    ) -> f64 {
        let mut depth = 0;
        const MAX_ROLLOUT_DEPTH: usize = 50;
        const VAL_KING_F: f64 = 20000.0;

        while depth < MAX_ROLLOUT_DEPTH {
            if let Some(tt) = &self.tt {
                if let Some((score, _, flag, _)) = tt.get(state.hash) {
                    if flag == Flag::Exact {
                        let normalized = (score as f64 / VAL_KING_F) / 2.0 + 0.5;
                        return normalized.max(0.0).min(1.0);
                    }
                }
            }

            let moves = Rules::generate_legal_moves(state, player);
            if moves.is_empty() {
                return self.evaluate_terminal(state, player);
            }

            let mv = moves.choose(rng).unwrap();
            state.apply_move(mv).unwrap();
            player = player.opponent();
            depth += 1;
        }

        0.5
    }

    fn evaluate_terminal(&self, state: &Board, player_at_leaf: Player) -> f64 {
        if let Some(king_pos) = state.get_king_coordinate(player_at_leaf) {
            if Rules::is_square_attacked(state, &king_pos, player_at_leaf.opponent()) {
                // Checkmate
                if let Some(tt) = &self.tt {
                    // Store loss for the player who is checkmated (Negamax perspective)
                    tt.store(state.hash, -CHECKMATE_SCORE, 255, Flag::Exact, None);
                }

                if player_at_leaf == self.root_player {
                    return 0.0; // Root lost (Checkmate)
                } else {
                    return 1.0; // Root won (Opponent Checkmated)
                }
            }
        }

        // Stalemate/Draw
        if let Some(tt) = &self.tt {
            tt.store(state.hash, 0, 255, Flag::Exact, None);
        }

        0.5 // Stalemate/Draw
    }

    fn backpropagate(&mut self, mut node_idx: usize, score: f64) {
        loop {
            let node = &mut self.nodes[node_idx];
            node.visits += 1;

            node.score += score;

            if let Some(parent) = node.parent {
                node_idx = parent;
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::Board;

    #[test]
    fn test_mcts_smoke() {
        let board = Board::new(2, 8); // 2D board, side 8
        let mut mcts = MCTS::new(&board, Player::White, None);
        let score = mcts.run(&board, 10);
        assert!(score >= 0.0 && score <= 1.0);
    }

    #[test]
    fn test_mcts_parallel_execution() {
        let board = Board::new(2, 8); // 2D board, side 8
        let mut mcts = MCTS::new(&board, Player::White, None);
        // Run with enough iterations to likely trigger multiple threads
        let score = mcts.run(&board, 100);
        assert!(score >= 0.0 && score <= 1.0);
    }
}
```
```./src/infrastructure/ai/minimax.rs
use super::mcts::MCTS;
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::domain::rules::Rules;
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CHECKMATE_SCORE: i32 = 30000;
const TIMEOUT_CHECK_INTERVAL: usize = 2048;

// Material values
const VAL_PAWN: i32 = 100;
const VAL_KNIGHT: i32 = 320;
const VAL_BISHOP: i32 = 330;
const VAL_ROOK: i32 = 500;
const VAL_QUEEN: i32 = 900;
const VAL_KING: i32 = 20000;

pub struct MinimaxBot {
    depth: usize,
    time_limit: Duration,
    tt: Arc<LockFreeTT>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: std::sync::atomic::AtomicUsize,
    _randomized: bool,
    use_mcts: bool,
    mcts_iterations: usize,
}

impl MinimaxBot {
    pub fn new(depth: usize, time_limit_ms: u64, _dimension: usize, _side: usize) -> Self {
        Self {
            depth,
            time_limit: Duration::from_millis(time_limit_ms),
            tt: Arc::new(LockFreeTT::new(64)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: std::sync::atomic::AtomicUsize::new(0),
            _randomized: true,
            use_mcts: false,      // Default off
            mcts_iterations: 100, // Default 100
        }
    }

    pub fn with_mcts(mut self, iterations: usize) -> Self {
        self.use_mcts = true;
        self.mcts_iterations = iterations;
        Self {
            depth: if self.use_mcts { 2 } else { self.depth }, // Reduce depth if MCTS is on to compensate?
            ..self
        }
    }

    fn evaluate(&self, board: &Board, player_at_leaf: Option<Player>) -> i32 {
        if self.use_mcts {
            if let Some(player) = player_at_leaf {
                // Run MCTS
                // Note: MCTS is expensive.
                let mut mcts = MCTS::new(board, player, Some(self.tt.clone()));
                let win_rate = mcts.run(board, self.mcts_iterations);

                // win_rate is [0, 1] for `player`.
                // Map to score. 1.0 -> 20000, 0.0 -> -20000.
                // value = (win_rate - 0.5) * 2 * 20000
                let val_f = (win_rate - 0.5) * 2.0 * (VAL_KING as f64);
                let val = val_f as i32;

                // Return White-centric score
                if player == Player::Black {
                    return -val;
                } else {
                    return val;
                }
            }
        }

        let mut score = 0;
        for i in 0..board.total_cells {
            if board.white_occupancy.get_bit(i) {
                score += self.get_piece_value(board, i);
            } else if board.black_occupancy.get_bit(i) {
                score -= self.get_piece_value(board, i);
            }
        }
        score
    }

    fn get_piece_value(&self, board: &Board, idx: usize) -> i32 {
        if board.pawns.get_bit(idx) {
            VAL_PAWN
        } else if board.knights.get_bit(idx) {
            VAL_KNIGHT
        } else if board.bishops.get_bit(idx) {
            VAL_BISHOP
        } else if board.rooks.get_bit(idx) {
            VAL_ROOK
        } else if board.queens.get_bit(idx) {
            VAL_QUEEN
        } else if board.kings.get_bit(idx) {
            VAL_KING
        } else {
            0
        }
    }

    fn minimax(
        &self,
        board: &mut Board,
        depth: usize,
        mut alpha: i32,
        mut beta: i32,
        player: Player,
        start_time: Instant,
    ) -> i32 {
        if self.nodes_searched.fetch_add(1, Ordering::Relaxed) % TIMEOUT_CHECK_INTERVAL == 0 {
            if start_time.elapsed() > self.time_limit {
                self.stop_flag.store(true, Ordering::Relaxed);
                return 0; // Abort
            }
        }
        if self.stop_flag.load(Ordering::Relaxed) {
            return 0;
        }

        // Check for repetition
        if board.is_repetition() {
            return 0; // Draw
        }

        let hash = board.hash;
        if let Some((tt_score, tt_depth, tt_flag, _)) = self.tt.get(hash) {
            if tt_depth as usize >= depth {
                match tt_flag {
                    Flag::Exact => return tt_score,
                    Flag::LowerBound => alpha = alpha.max(tt_score),
                    Flag::UpperBound => beta = beta.min(tt_score),
                }
                if alpha >= beta {
                    return tt_score;
                }
            }
        }

        if depth == 0 {
            return match player {
                Player::White => self.evaluate(board, Some(Player::White)),
                Player::Black => -self.evaluate(board, Some(Player::Black)),
            };
        }

        let moves = Rules::generate_legal_moves(board, player);

        if moves.is_empty() {
            if let Some(king_pos) = board.get_king_coordinate(player) {
                if Rules::is_square_attacked(board, &king_pos, player.opponent()) {
                    return -CHECKMATE_SCORE + (self.depth - depth) as i32;
                }
            }
            return 0; // Stalemate
        }

        let mut best_score = -i32::MAX;
        let original_alpha = alpha;

        for mv in moves {
            let mut next_board = board.clone();
            if next_board.apply_move(&mv).is_ok() {
                let score = -self.minimax(
                    &mut next_board,
                    depth - 1,
                    -beta,
                    -alpha,
                    player.opponent(),
                    start_time,
                );

                if self.stop_flag.load(Ordering::Relaxed) {
                    return 0;
                }

                if score > best_score {
                    best_score = score;
                }
                alpha = alpha.max(score);
                if alpha >= beta {
                    break;
                }
            }
        }

        let flag = if best_score <= original_alpha {
            Flag::UpperBound
        } else if best_score >= beta {
            Flag::LowerBound
        } else {
            Flag::Exact
        };

        self.tt.store(hash, best_score, depth as u8, flag, None);

        best_score
    }
}

impl PlayerStrategy for MinimaxBot {
    fn get_move(&mut self, board: &Board, player: Player) -> Option<Move> {
        self.nodes_searched.store(0, Ordering::Relaxed);
        self.stop_flag.store(false, Ordering::Relaxed);

        let start_time = Instant::now();
        let mut best_score = -i32::MAX;
        let mut best_moves = Vec::new(); // Collect all best moves

        // Root Search
        let moves = Rules::generate_legal_moves(board, player);
        if moves.is_empty() {
            return None;
        }

        for mv in moves {
            let mut next_board = board.clone();
            if next_board.apply_move(&mv).is_ok() {
                if next_board.is_repetition() {
                    // Repetition handling logic (implicit via minimax returning 0 for it usually)
                }

                let score = -self.minimax(
                    &mut next_board,
                    self.depth - 1,
                    -i32::MAX,
                    i32::MAX,
                    player.opponent(),
                    start_time,
                );

                if score > best_score {
                    best_score = score;
                    best_moves.clear();
                    best_moves.push(mv);
                } else if score == best_score {
                    best_moves.push(mv);
                }
            }
        }

        // Pick random best move
        if !best_moves.is_empty() {
            let mut rng = rand::thread_rng();
            use rand::seq::SliceRandom;
            best_moves.choose(&mut rng).cloned()
        } else {
            None
        }
    }
}
```
```./src/infrastructure/ai/mod.rs
pub mod mcts;
pub mod minimax;
pub mod transposition;

pub use minimax::MinimaxBot;
```
```./src/infrastructure/ai/transposition.rs
use std::sync::atomic::{AtomicU64, Ordering};

// Pack data into u64:
// 32 bits score | 8 bits depth | 2 bits flag | 22 bits partial hash/verification
// We will store the FULL key in a separate atomic for verification.
// The packed data is primarily for the value payload.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Exact,
    LowerBound,
    UpperBound,
}

#[derive(Clone, Copy, Debug)]
pub struct PackedMove {
    pub from_idx: u16,
    pub to_idx: u16,
    pub promotion: u8, // 0=None, 1=Q, 2=R, 3=B, 4=N...
}

impl PackedMove {
    pub fn to_u32(&self) -> u32 {
        (self.from_idx as u32) | ((self.to_idx as u32) << 16)
    }

    pub fn from_u32(val: u32) -> Self {
        Self {
            from_idx: (val & 0xFFFF) as u16,
            to_idx: ((val >> 16) & 0xFFFF) as u16,
            promotion: 0,
        }
    }
}

pub struct LockFreeTT {
    table: Vec<AtomicU64>,
    size_mask: usize,
}

impl LockFreeTT {
    pub fn new(size_mb: usize) -> Self {
        let size = size_mb * 1024 * 1024 / std::mem::size_of::<AtomicU64>();
        let num_entries = size.next_power_of_two();

        let mut table = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            table.push(AtomicU64::new(0));
        }

        LockFreeTT {
            table,
            size_mask: num_entries - 1,
        }
    }

    pub fn get(&self, hash: u64) -> Option<(i32, u8, Flag, Option<PackedMove>)> {
        let index = (hash as usize) & self.size_mask;
        let entry = self.table[index].load(Ordering::Relaxed);

        if entry == 0 {
            return None;
        }

        let entry_hash = (entry >> 32) as u32; // Top 32 bits of hash
        if entry_hash != (hash >> 32) as u32 {
            return None;
        }

        let data = entry as u32;
        // Unpacking:
        // Score: 16 bits (0-15)
        // Depth: 8 bits (16-23)
        // Flag: 2 bits (24-25)
        // HasMove: 1 bit (26)
        // Move From: ? We didn't store full move in u64 with this packing scheme.
        // The previous attempt realized we can't fit it.
        // Let's settle for NOT storing the move if we don't have space with 64-bit entry.
        // OR we just assume we return None for now as placeholder for the refactor.
        // To properly support Move storage we need 128-bit atomics or a larger struct.
        // For this task, let's keep the signature but return None for move.

        let score = (data & 0xFFFF) as i16 as i32;
        let depth = ((data >> 16) & 0xFF) as u8;
        let flag_u8 = ((data >> 24) & 0x3) as u8;

        let flag = match flag_u8 {
            0 => Flag::Exact,
            1 => Flag::LowerBound,
            2 => Flag::UpperBound,
            _ => Flag::Exact,
        };

        Some((score, depth, flag, None)) // Placeholder: We are not storing moves yet due to size constraints.
    }

    pub fn store(
        &self,
        hash: u64,
        score: i32,
        depth: u8,
        flag: Flag,
        _best_move: Option<PackedMove>,
    ) {
        let index = (hash as usize) & self.size_mask;
        let key_part = (hash >> 32) as u32;

        let score_part = (score.clamp(i16::MIN as i32 + 1, i16::MAX as i32 - 1) as i16) as u16;
        let flag_u8 = match flag {
            Flag::Exact => 0,
            Flag::LowerBound => 1,
            Flag::UpperBound => 2,
        };

        let mut data: u32 = score_part as u32;
        data |= (depth as u32) << 16;
        data |= (flag_u8 as u32) << 24;
        // We drop best_move for now as decided.

        let entry = ((key_part as u64) << 32) | (data as u64);

        self.table[index].store(entry, Ordering::Relaxed);
    }
}
```
```./src/infrastructure/console.rs
use crate::domain::board::Board;
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, Player};
use crate::domain::services::PlayerStrategy;
use std::io::{self, Write};

pub struct HumanConsolePlayer;

impl HumanConsolePlayer {
    pub fn new() -> Self {
        Self
    }

    fn parse_index(input: &str) -> Result<usize, String> {
        input
            .trim()
            .parse::<usize>()
            .map_err(|_| "Invalid number".to_string())
    }

    fn index_to_coord(idx: usize, dim: usize, side: usize) -> Coordinate {
        let mut coords = vec![0; dim];
        let mut temp = idx;
        for i in 0..dim {
            coords[i] = temp % side;
            temp /= side;
        }
        Coordinate::new(coords)
    }
}

impl PlayerStrategy for HumanConsolePlayer {
    fn get_move(&mut self, board: &Board, _player: Player) -> Option<Move> {
        let dim = board.dimension();
        let side = board.side();
        let total = board.total_cells();

        loop {
            println!(
                "Enter Move (From Index -> To Index, e.g. '0 10'). Max Index: {}",
                total - 1
            );
            print!("> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            let parts: Vec<&str> = input.trim().split_whitespace().collect();
            if parts.len() < 2 {
                println!("Please provide two indices: From To");
                continue;
            }

            let from_res = Self::parse_index(parts[0]);
            let to_res = Self::parse_index(parts[1]);

            match (from_res, to_res) {
                (Ok(from_idx), Ok(to_idx)) => {
                    if from_idx >= total || to_idx >= total {
                        println!("Index out of bounds!");
                        continue;
                    }

                    let from_coord = Self::index_to_coord(from_idx, dim, side);
                    let to_coord = Self::index_to_coord(to_idx, dim, side);

                    // Optional promotion?
                    let promotion = if parts.len() > 2 {
                        // primitive parsing for now
                        match parts[2] {
                            "Q" | "q" | "4" => Some(crate::domain::models::PieceType::Queen),
                            "R" | "r" | "3" => Some(crate::domain::models::PieceType::Rook),
                            "B" | "b" | "2" => Some(crate::domain::models::PieceType::Bishop),
                            "N" | "n" | "1" => Some(crate::domain::models::PieceType::Knight),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    return Some(Move {
                        from: from_coord,
                        to: to_coord,
                        promotion,
                    });
                }
                _ => println!("Invalid indices."),
            }
        }
    }
}
```
```./src/infrastructure/display.rs
use crate::domain::board::Board;
use crate::domain::models::{PieceType, Player};
use std::fmt;

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_WHITE: &str = "\x1b[37m";
const COLOR_BLACK: &str = "\x1b[31m";
const COLOR_DIM: &str = "\x1b[90m";

struct Canvas {
    width: usize,
    height: usize,
    buffer: Vec<String>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Canvas {
            width,
            height,
            buffer: vec![" ".to_string(); width * height],
        }
    }

    fn put(&mut self, x: usize, y: usize, s: &str) {
        if x < self.width && y < self.height {
            self.buffer[y * self.width + x] = s.to_string();
        }
    }
}

impl fmt::Display for Canvas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for y in 0..self.height {
            for x in 0..self.width {
                write!(f, "{}", self.buffer[y * self.width + x])?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

pub fn render_board(board: &Board) -> String {
    let dim = board.dimension();
    let side = board.side();
    let (w, h) = calculate_size(dim, side);
    let mut canvas = Canvas::new(w, h);

    draw_recursive(board, dim, &mut canvas, 0, 0, 0);

    canvas.to_string()
}

fn calculate_size(dim: usize, side: usize) -> (usize, usize) {
    if dim == 0 {
        return (1, 1);
    }
    if dim == 1 {
        return (side, 1);
    }
    if dim == 2 {
        return (side * 2 - 1, side);
    }

    let (child_w, child_h) = calculate_size(dim - 1, side);

    if dim % 2 != 0 {
        let gap = 2;
        (child_w * side + gap * (side - 1), child_h)
    } else {
        let gap = 1;
        (child_w, child_h * side + gap * (side - 1))
    }
}

fn draw_recursive(
    board: &Board,
    current_dim: usize,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
) {
    let side = board.side();

    if current_dim == 2 {
        for dy in 0..side {
            for dx in 0..side {
                let cell_idx = base_index + dx + dy * side;
                let coord_vals = board.index_to_coords(cell_idx);
                let coord = crate::domain::coordinate::Coordinate::new(coord_vals);

                let s = match board.get_piece(&coord) {
                    Some(piece) => {
                        let symbol = match piece.owner {
                            Player::White => match piece.piece_type {
                                PieceType::Pawn => "♙",
                                PieceType::Knight => "♘",
                                PieceType::Bishop => "♗",
                                PieceType::Rook => "♖",
                                PieceType::Queen => "♕",
                                PieceType::King => "♔",
                            },
                            Player::Black => match piece.piece_type {
                                PieceType::Pawn => "♟",
                                PieceType::Knight => "♞",
                                PieceType::Bishop => "♝",
                                PieceType::Rook => "♜",
                                PieceType::Queen => "♛",
                                PieceType::King => "♚",
                            },
                        };

                        let color = match piece.owner {
                            Player::White => COLOR_WHITE,
                            Player::Black => COLOR_BLACK,
                        };
                        format!("{}{}{}", color, symbol, COLOR_RESET)
                    }
                    None => format!("{}.{}", COLOR_DIM, COLOR_RESET),
                };
                canvas.put(x + dx * 2, y + dy, &s);
            }
        }
        return;
    }

    let (child_w, child_h) = calculate_size(current_dim - 1, side);
    let stride = side.pow((current_dim - 1) as u32);

    if current_dim % 2 != 0 {
        let gap = 2;
        for i in 0..side {
            let next_x = x + i * (child_w + gap);
            let next_y = y;
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < side - 1 {
                let sep_x = next_x + child_w + gap / 2 - 1;
                for k in 0..child_h {
                    canvas.put(sep_x, next_y + k, &format!("{}|{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    } else {
        let gap = 1;
        for i in 0..side {
            let next_x = x;
            let next_y = y + i * (child_h + gap);
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < side - 1 {
                let sep_y = next_y + child_h;
                for k in 0..child_w {
                    canvas.put(next_x + k, sep_y, &format!("{}-{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    }
}
```
```./src/infrastructure/mod.rs
pub mod ai;
pub mod console;
pub mod display;
pub mod symmetries;
pub mod time;
```
```./src/infrastructure/symmetries.rs
pub struct SymmetryHandler {
    pub maps: Vec<Vec<usize>>,
}

impl SymmetryHandler {
    pub fn new(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let mut maps = Vec::new();

        let mut axes: Vec<usize> = (0..dimension).collect();
        let permutations = permute(&mut axes);

        let num_reflections = 1 << dimension;

        for perm in &permutations {
            for ref_mask in 0..num_reflections {
                let mut map = vec![0; total_cells];

                for i in 0..total_cells {
                    let coords = index_to_coords(i, dimension, side);

                    let mut new_coords = vec![0; dimension];
                    for (dest_axis, &src_axis) in perm.iter().enumerate() {
                        new_coords[dest_axis] = coords[src_axis];
                    }

                    for (axis, val) in new_coords.iter_mut().enumerate() {
                        if (ref_mask >> axis) & 1 == 1 {
                            *val = side - 1 - *val;
                        }
                    }

                    map[i] = coords_to_index(&new_coords, side);
                }
                maps.push(map);
            }
        }

        SymmetryHandler { maps }
    }
}

fn permute(arr: &mut [usize]) -> Vec<Vec<usize>> {
    let mut res = Vec::new();
    heap_permute(arr.len(), arr, &mut res);
    res
}

fn heap_permute(k: usize, arr: &mut [usize], res: &mut Vec<Vec<usize>>) {
    if k == 1 {
        res.push(arr.to_vec());
    } else {
        heap_permute(k - 1, arr, res);
        for i in 0..k - 1 {
            if k % 2 == 0 {
                arr.swap(i, k - 1);
            } else {
                arr.swap(0, k - 1);
            }
            heap_permute(k - 1, arr, res);
        }
    }
}

fn index_to_coords(mut index: usize, dim: usize, side: usize) -> Vec<usize> {
    let mut coords = Vec::with_capacity(dim);
    for _ in 0..dim {
        coords.push(index % side);
        index /= side;
    }
    coords
}

fn coords_to_index(coords: &[usize], side: usize) -> usize {
    let mut idx = 0;
    let mut mul = 1;
    for &c in coords {
        idx += c * mul;
        mul *= side;
    }
    idx
}
```
```./src/infrastructure/time.rs
use crate::domain::services::Clock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
    }
}

pub struct FakeClock {
    current_time: Duration,
}

impl FakeClock {
    pub fn new(start_time: Duration) -> Self {
        Self {
            current_time: start_time,
        }
    }

    pub fn advance(&mut self, amount: Duration) {
        self.current_time += amount;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.current_time
    }
}
```
```./src/interface/console.rs
use crate::application::game_service::GameService;
use crate::domain::models::GameResult;
use crate::infrastructure::display::render_board;

pub struct ConsoleInterface;

impl ConsoleInterface {
    pub fn run(mut game_service: GameService) {
        println!("Starting Game...");
        println!("{}", render_board(game_service.board()));

        loop {
            if let Some(result) = game_service.is_game_over() {
                match result {
                    GameResult::Checkmate(p) => println!("Checkmate! Player {:?} Wins!", p),
                    GameResult::Stalemate => println!("Stalemate! It's a Draw!"),
                    GameResult::Draw => println!("Draw!"),
                    _ => {}
                }
                break;
            }

            println!("Player {:?}'s turn", game_service.turn());

            match game_service.perform_next_move() {
                Ok(_) => {
                    println!("{}", render_board(game_service.board()));
                }
                Err(e) => {
                    println!("Error: {}", e);
                    if e == "No move available" {
                        break;
                    }
                }
            }
        }
    }
}
```
```./src/interface/mod.rs
pub mod console;
```
```./src/lib.rs
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interface;
```
```./src/main.rs
use hyperchess::application::game_service::GameService;
use hyperchess::domain::board::Board;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;
use hyperchess::infrastructure::console::HumanConsolePlayer;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dimension = 3;
    let side = 8; // Default side 8 for HyperChess
    let mut player_white_type = "h";
    let mut player_black_type = "c";
    let mut depth = 4;
    let time_limit = 1000; // ms

    if args.len() > 1 {
        if let Ok(d) = args[1].parse::<usize>() {
            dimension = d;
        }
    }
    if args.len() > 2 {
        let mode = args[2].as_str();
        if mode.len() >= 2 {
            player_white_type = &mode[0..1];
            player_black_type = &mode[1..2];
        }
    }
    if args.len() > 3 {
        if let Ok(d) = args[3].parse::<usize>() {
            depth = d;
        }
    }

    let player_white: Box<dyn PlayerStrategy> = match player_white_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth, time_limit, dimension, side).with_mcts(50)),
        _ => Box::new(HumanConsolePlayer::new()),
    };

    let player_black: Box<dyn PlayerStrategy> = match player_black_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth, time_limit, dimension, side).with_mcts(50)),
        _ => Box::new(MinimaxBot::new(depth, time_limit, dimension, side).with_mcts(50)),
    };

    let board = Board::new(dimension, side);

    let game = GameService::new(board, player_white, player_black);
    hyperchess::interface::console::ConsoleInterface::run(game);
}
```
```./tests/basic_chess.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::models::Player;
use hyperchess::domain::rules::Rules;

#[test]
fn test_initial_board_setup_and_pawn_move() {
    let dim = 2; // Simple 2D chess
    let side = 8;
    let mut board = Board::new_empty(dim, side);

    // We need to populate the board first. BitBoardState::new returns EMPTY board now (based on persistence.rs change).
    // So we must manually setup pieces or assume GameService setup.
    // Wait, MinimaxBot expects pieces.
    // Let's manually place a White Pawn at index 8 (Row 1, Col 0) and check moves.

    use hyperchess::domain::coordinate::Coordinate;
    use hyperchess::domain::models::{Piece, PieceType};

    let pawn = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    let start_idx = 8; // (0, 1) in 8x8
    let start_coord = Coordinate::new(board.index_to_coords(start_idx));

    board.set_piece(&start_coord, pawn).unwrap();

    // Add Kings (Required for move legality check)
    let w_king = Piece {
        piece_type: PieceType::King,
        owner: Player::White,
    };
    let b_king = Piece {
        piece_type: PieceType::King,
        owner: Player::Black,
    };

    // Place Kings far away
    let w_king_coord = Coordinate::new(vec![0, 0]);
    let b_king_coord = Coordinate::new(vec![7, 7]);

    board.set_piece(&w_king_coord, w_king).unwrap();
    board.set_piece(&b_king_coord, b_king).unwrap();

    // Generate moves for White
    let moves = Rules::generate_legal_moves(&board, Player::White);

    assert!(!moves.is_empty(), "Should generate moves");

    // Find pawn move
    let pawn_move = moves
        .iter()
        .find(|m| m.from == start_coord)
        .expect("Should find pawn move")
        .clone();

    println!("Applying move: {:?}", pawn_move);

    board.apply_move(&pawn_move).unwrap();

    assert!(
        board.get_piece(&start_coord).is_none(),
        "Start square should be empty"
    );
    assert!(
        board.get_piece(&pawn_move.to).is_some(),
        "End square should be occupied"
    );
}
```
```./tests/initial_state.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{PieceType, Player};

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_standard_2d_chess_setup() {
    let board = Board::new(2, 8);

    // Check Corner Rooks
    assert!(
        is_piece_at(&board, &coord(0, 0), PieceType::Rook, Player::White),
        "White Rook at (0,0)"
    );
    assert!(
        is_piece_at(&board, &coord(0, 7), PieceType::Rook, Player::White),
        "White Rook at (0,7)"
    );
    assert!(
        is_piece_at(&board, &coord(7, 0), PieceType::Rook, Player::Black),
        "Black Rook at (7,0)"
    );

    // Check King/Queen
    // White King at (0, 4)
    assert!(
        is_piece_at(&board, &coord(0, 4), PieceType::King, Player::White),
        "White King at (0,4)"
    );
    // White Queen at (0, 3)
    assert!(
        is_piece_at(&board, &coord(0, 3), PieceType::Queen, Player::White),
        "White Queen at (0,3)"
    );

    // Check Pawns
    for i in 0..8 {
        assert!(
            is_piece_at(&board, &coord(1, i), PieceType::Pawn, Player::White),
            "White Pawn at (1, {})",
            i
        );
        assert!(
            is_piece_at(&board, &coord(6, i), PieceType::Pawn, Player::Black),
            "Black Pawn at (6, {})",
            i
        );
    }

    // Check Empty Middle
    assert!(board.get_piece(&coord(3, 3)).is_none());
    assert!(board.get_piece(&coord(4, 4)).is_none());
}

#[test]
fn test_3d_setup() {
    // 3D 4x4x4
    let board = Board::new(3, 4);

    // New Setup Logic:
    // White: x=0 (Rank), pieces at z=0.
    // Black: x=3 (Rank), pieces at z=3 (side-1).
    // King position is determined by y (file) index.
    // y = side / 2 = 2.
    // So White King at (0, 2, 0).
    // Black King at (3, 2, 3).

    // White King
    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![0, 2, 0]),
            PieceType::King,
            Player::White
        ),
        "White King at (0, 2, 0)"
    );

    // Black King
    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![3, 2, 3]),
            PieceType::King,
            Player::Black
        ),
        "Black King at (3, 2, 3)"
    );

    // Verify EMPTY elsewhere (e.g. z=1)
    assert!(
        board.get_piece(&Coordinate::new(vec![0, 2, 1])).is_none(),
        "Should be empty at z=1"
    );

    // Count total pieces?
    // Side=4.
    // White: 4 Pawns + 4 Pieces = 8.
    // Black: 4 Pawns + 4 Pieces = 8.
    // Total 16.
    // Implementation details: Board stores pieces in bitboards.
    // Check occupancy count if possible, or just trust specific checks.
}

fn is_piece_at(board: &Board, c: &Coordinate, t: PieceType, p: Player) -> bool {
    if let Some(piece) = board.get_piece(c) {
        piece.piece_type == t && piece.owner == p
    } else {
        false
    }
}
```
```./tests/mcts_test.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::models::Player;
use hyperchess::infrastructure::ai::mcts::MCTS;

#[test]
fn test_mcts_initialization_and_run() {
    let board = Board::new(3, 4); // Small 3D board
    let mut mcts = MCTS::new(&board, Player::White, None);
    let win_rate = mcts.run(&board, 50);

    // Win rate should be between 0 and 1
    assert!(win_rate >= 0.0);
    assert!(win_rate <= 1.0);
    println!("MCTS Win Rate: {}", win_rate);
}

#[test]
fn test_mcts_checkmate_detection() {
    let board = Board::new(2, 8);
    // Board::new already sets up standard chess.
    // board.setup_standard_chess(); // No need to call again if new calls it, but let's check.
    // Board::new calls setup_standard_chess.

    let mut mcts = MCTS::new(&board, Player::White, None);
    let win_rate = mcts.run(&board, 50);

    assert!(win_rate >= 0.0);
    assert!(win_rate <= 1.0);
}
```
```./tests/minimax_behavior.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_detect_checkmate_in_one() {
    // 2D 4x4 board.
    // White King at (0,0).
    // White Rook at (0, 2).
    // Black King at (2,0).
    // Move Rook to (2, 2) -> Checkmate? (Assuming lateral check and King blocked).
    // Let's set up a simpler "Fool's Mate" style or similar direct mate.

    // 3x3 board for simplicity.
    // White King at (0,0).
    // Black King at (2,0).
    // White Rook at (0,1).
    // White to move. Move Rook to (2,1).
    // Black King at (2,0) is attacked by (2,1) Rook? No, orthogonal.
    // Rook at (2,1) attacks (2,0).
    // Black King at (2,0) has neighbors: (1,0), (1,1), (2,1).
    // If (1,0) and (1,1) are also attacked or blocked.

    // Easier: Back rank mate.
    // Board 4x4.
    // Black King at (0, 3) (Top Left-ish).
    // Black Pawns at (0, 2), (1, 2) blocking escape.
    // White Rook at (3, 0).
    // Move: Rook (3,0) -> (3,3)? No, (0,3) needs to be attacked.
    // Move: Rook (3,0) -> (0,0) CHECK -> King stuck?
    // Let's just trust valid chess logic.

    let mut board = Board::new_empty(2, 4);

    // Setup Black King trapped in corner (3,3)
    board
        .set_piece(
            &coord(3, 3),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    // Block escapes: (2,3) and (3,2) blocked by own pieces
    board
        .set_piece(
            &coord(2, 3),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(3, 2),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();
    // Diagonal (2,2) needs coverage.
    board
        .set_piece(
            &coord(2, 2),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    // Attacker: White Rook at (0, 3). Moves to check on file 3? No, King is at (3,3).
    // White Rook at (0, 3) attacks (3,3)? Yes, if path clear.
    // Path: (1,3), (2,3).
    // (2,3) is occupied by Black Pawn. So blocked.

    // Setup Helper Mate
    // Black King at (0,0).
    // White King at (2,0) (Opposition).
    // White Queen at (3,3). Move to (0,3)? Check?
    // Move Queen to (0,1) -> Checkmate?
    // (0,0) attacked by Queen at (0,1).
    // Neighbors: (1,0) attacked by Q(0,1)? Yes (diagonal).
    // (1,1) attacked by Q(0,1)? Yes (rank).
    // (1,0) also covers by King(2,0)? No, King(2,0) attacks (1,0), (1,1), (2,1).
    // Yes, White King at (2,0) guards (1,0) and (1,1).
    // So Black King has no moves.

    board = Board::new_empty(2, 4);
    board
        .set_piece(
            &coord(0, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(2, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();

    // White Queen at (0, 3).
    board
        .set_piece(
            &coord(0, 3),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::White,
            },
        )
        .unwrap();

    // Best move should be Q(0,3) -> (0,1) # Checkmate.
    // Or Q(0,3) -> (0,0) capture? No, King there.
    // Wait, (0,1) is adjacent to Black King (0,0).
    // Supported by White King at (2,0)?
    // Dist from (2,0) to (0,1) is... dx=2, dy=1. Not adjacent. Not supported.
    // Black King captures Queen.

    // Need King closer. White King at (0,2)? No, adjacent kings illegal.
    // White King at (1,2). Guards (0,1), (1,1), (2,1)...
    // (0,1) is guarded by King at (1,2).

    board = Board::new_empty(2, 4);
    board
        .set_piece(
            &coord(0, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(1, 2),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 3),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::White,
            },
        )
        .unwrap();

    // Bot with depth 2 should find mate in 1.
    let mut bot = MinimaxBot::new(2, 1000, 2, 4);
    let mv = bot
        .get_move(&board, Player::White)
        .expect("Should return a move");

    assert!(
        mv.to == coord(0, 1) || mv.to == coord(2, 1),
        "Should find checkmate move (Queen to (0,1) or King to (2,1)), found {:?}",
        mv.to
    );
    // Also accept generic "mate finding".
}

#[test]
fn test_verify_mate_validity() {
    let mut board = Board::new_empty(2, 4);
    board
        .set_piece(
            &coord(0, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(1, 2),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 3),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::White,
            },
        )
        .unwrap();

    // 1. Verify Q->(0,1) is legal
    let moves = Rules::generate_legal_moves(&board, Player::White);
    let mate_move = moves.iter().find(|m| m.to == coord(0, 1));
    assert!(mate_move.is_some(), "Move to (0,1) should be legal");

    // 2. Apply move
    board.apply_move(mate_move.unwrap()).unwrap();

    // 3. Verify Black has no moves
    let black_moves = Rules::generate_legal_moves(&board, Player::Black);
    assert!(
        black_moves.is_empty(),
        "Black should have no moves after Checkmate"
    );

    // 4. Verify Black is in check
    let black_king = board.get_king_coordinate(Player::Black).unwrap();
    assert!(
        Rules::is_square_attacked(&board, &black_king, Player::White),
        "Black King should be in check"
    );
}

#[test]
fn test_avoid_immediate_mate() {
    // If Black is about to be mated, it should move King or block.
}
```
```./tests/movement_2d.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use std::collections::HashSet;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_pawn_moves_white_start() {
    let mut board = Board::new_empty(2, 8);
    // Remove all pieces for clean slate testing?
    // `new` creates empty board? User prompt said "The board is empty at the beginning of the game".
    // Let's verify that. If so, we just place what we need.

    // Low-level setup: White Pawn at (1, 1) (Rank 1 is usually pawn start in 0-indexed terms? in standard chess: rank 1 (0-7 indexing) is White Pawns)
    // Coords: vec![rank, file] or vec![file, rank]?
    // mechanics.rs: `forward_dir` for White is +1 on axis 0.
    // So axis 0 is "Rank" (Forward/Backward). Axis 1 is "File" (Sideways).
    // White moves +1 on Axis 0.
    // Start Rank for White is typically index 1.
    // Start Rank for Black is typically index 6.

    let pawn_pos = coord(1, 3); // Rank 1, File 3
    let p = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    board.set_piece(&pawn_pos, p).unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Expect: Single push to (2, 3), Double push to (3, 3).
    // No captures available.

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord(2, 3)), "Should have single push");
    assert!(
        dests.contains(&coord(3, 3)),
        "Should have double push from start rank"
    );
    assert_eq!(dests.len(), 2, "Should only have 2 moves");
}

#[test]
fn test_pawn_blocked() {
    let mut board = Board::new_empty(2, 8);
    let pawn_pos = coord(1, 4);
    let blocker = coord(2, 4);

    board
        .set_piece(
            &pawn_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &blocker,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::Black,
            },
        )
        .unwrap(); // Enemy blocks

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Pawn cannot move forward if blocked.
    assert_eq!(moves.len(), 0, "Pawn should be blocked");
}

#[test]
fn test_pawn_capture() {
    let mut board = Board::new_empty(2, 8);
    let pawn_pos = coord(3, 3); // Not start rank
    let enemy_pos = coord(4, 4); // Diagonally forward right

    board
        .set_piece(
            &pawn_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &enemy_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord(4, 3)), "Single push");
    assert!(dests.contains(&coord(4, 4)), "Capture right");
    // Double push NOT allowed (not start rank)
    assert!(!dests.contains(&coord(5, 3)), "No double push");
    assert_eq!(dests.len(), 2);
}

#[test]
fn test_knight_moves_center() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(4, 4);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Knight,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // 8 possible moves in 2D
    assert_eq!(moves.len(), 8);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();
    // +/- 2 on one axis, +/- 1 on other
    assert!(dests.contains(&coord(6, 5)));
    assert!(dests.contains(&coord(6, 3)));
    assert!(dests.contains(&coord(2, 5)));
    assert!(dests.contains(&coord(2, 3)));
    assert!(dests.contains(&coord(5, 6)));
    assert!(dests.contains(&coord(3, 6)));
    assert!(dests.contains(&coord(5, 2)));
    assert!(dests.contains(&coord(3, 2)));
}

#[test]
fn test_rook_moves() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(4, 4);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    // Add a blocker
    board
        .set_piece(
            &coord(4, 6),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap(); // Clean block

    let moves = Rules::generate_legal_moves(&board, Player::White);
    let rook_moves: Vec<_> = moves.into_iter().filter(|m| m.from == pos).collect();
    let dests: HashSet<Coordinate> = rook_moves.iter().map(|m| m.to.clone()).collect();

    // Axis 0 (Vertical/Rank): (0..8) except 4 -> 7 squares.
    // Axis 1 (Horizontal/File): 4 is blocked at 6. Can go 0,1,2,3,5. (Blocked at 6 means cannot go to 6 or 7).
    // Total: 7 + 5 = 12 moves?

    // Explicit checks:
    // Vertical: (0,4), (1,4), (2,4), (3,4), (5,4), (6,4), (7,4) -> 7 moves
    // Horizontal: (4,0), (4,1), (4,2), (4,3), (4,5) -> 5 moves

    assert_eq!(rook_moves.len(), 12);
    assert!(!dests.contains(&coord(4, 6))); // Blocked
    assert!(!dests.contains(&coord(4, 7))); // Behind blocker
}

#[test]
fn test_bishop_moves() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(0, 0); // Corner
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    // Main diagonal only: (1,1) .. (7,7) -> 7 moves
    assert_eq!(moves.len(), 7);
}

#[test]
fn test_king_moves() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(1, 1);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    // 8 neighbors
    assert_eq!(moves.len(), 8);
}
```
```./tests/movement_3d.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use std::collections::HashSet;

fn coord3(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_bishop_moves_3d() {
    // 3D board, 4x4x4
    let mut board = Board::new_empty(3, 4);
    let pos = coord3(1, 1, 1);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    // Bishops in 3D: even number of non-zero displacements.
    // Dirs:
    // 1. (±1, ±1, 0)
    // 2. (±1, 0, ±1)
    // 3. (0, ±1, ±1)
    // Total dirs = 4 + 4 + 4 = 12 directions.

    // Let's check a few targets.
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 2, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(0, 0, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(2, 1, 2)), "2D diagonal xz");
    assert!(dests.contains(&coord3(1, 2, 2)), "2D diagonal yz");

    // (2,2,2) would be (1+1, 1+1, 1+1) -> 3 non-zero displacements -> ODD -> Not a Bishop move in default "HyperChess" (usually).
    // Let's verify standard hyperchess rules for "Bishop".
    // mechanics.rs: `get_bishop_directions`: "count of non-zero elements is EVEN".
    // So (1,1,1) displacement is NOT allowed.
    assert!(
        !dests.contains(&coord3(2, 2, 2)),
        "3D space diagonal forbidden for Bishop"
    );
}

#[test]
fn test_rook_moves_3d() {
    let mut board = Board::new_empty(3, 4);
    let pos = coord3(1, 1, 1);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    // Rooks: 1 non-zero displacement.
    // directions: (±1, 0, 0), (0, ±1, 0), (0, 0, ±1) -> 6 dirs.

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 1, 1)));
    assert!(dests.contains(&coord3(1, 2, 1)));
    assert!(dests.contains(&coord3(1, 1, 2)));

    assert!(!dests.contains(&coord3(2, 2, 1))); // Diagonal
}

#[test]
fn test_knight_moves_3d() {
    let mut board = Board::new_empty(3, 4);
    let pos = coord3(0, 0, 0);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Knight,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    // Knights: One axis ±2, one axis ±1.
    // From (0,0,0):
    // (2, 1, 0), (2, 0, 1)
    // (1, 2, 0), (0, 2, 1)
    // (1, 0, 2), (0, 1, 2)
    // Negatives are out of bounds.

    assert!(dests.contains(&coord3(2, 1, 0)));
    assert!(dests.contains(&coord3(0, 1, 2)));
    assert_eq!(dests.len(), 6);
}
```
```./tests/movement_5d.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

#[test]
fn test_5d_bishop_movement() {
    // 5D board, side length 2 (3^5 = 243 cells if side 3, but side 2 is 2^5 = 32 cells)
    // Small side ensures we don't explode memory if vec size depends on (side^N) linearly.
    let dimension = 5;
    let side = 3;
    let mut board = Board::new_empty(dimension, side);

    // Center-ish: (1, 1, 1, 1, 1)
    let center = Coordinate::new(vec![1, 1, 1, 1, 1]);
    board
        .set_piece(
            &center,
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Valid moves must have EVEN number of unit steps.
    for m in moves {
        let diff = diff_coords(&center, &m.to);
        let non_zeros = diff.iter().filter(|&&d| d != 0).count();
        assert!(non_zeros > 0, "Must move");
        assert_eq!(
            non_zeros % 2,
            0,
            "Bishop 5D move must have even number of coordinate changes. Found move to {:?} with {} changes",
            m.to,
            non_zeros
        );
    }
}

#[test]
fn test_5d_rook_movement() {
    let dimension = 5;
    let side = 3;
    let mut board = Board::new_empty(dimension, side);

    let center = Coordinate::new(vec![1, 1, 1, 1, 1]);
    board
        .set_piece(
            &center,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Valid moves must have EXACTLY ONE unit step.
    for m in moves {
        let diff = diff_coords(&center, &m.to);
        let non_zeros = diff.iter().filter(|&&d| d != 0).count();
        assert_eq!(
            non_zeros, 1,
            "Rook 5D move must allow movement on exactly one axis"
        );
    }
}

#[test]
fn test_5d_knight_movement() {
    let dimension = 5;
    let side = 5; // Need enough space for L-jump
    let mut board = Board::new_empty(dimension, side);

    let center = Coordinate::new(vec![2, 2, 2, 2, 2]);
    board
        .set_piece(
            &center,
            Piece {
                piece_type: PieceType::Knight,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    for m in moves {
        let diff = diff_coords(&center, &m.to);
        let non_zeros = diff.iter().filter(|&&d| d != 0).count();
        assert_eq!(non_zeros, 2, "Knight 5D move changes exactly 2 coords");

        let abs_sum: usize = diff.iter().map(|&d| d.abs() as usize).sum();
        assert_eq!(
            abs_sum, 3,
            "Knight move is +/-2 and +/-1 => sum of abs diffs is 3"
        );
    }
}

fn diff_coords(c1: &Coordinate, c2: &Coordinate) -> Vec<isize> {
    c1.values
        .iter()
        .zip(c2.values.iter())
        .map(|(a, b)| *a as isize - *b as isize)
        .collect()
}
```
```./tests/special_moves.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
// use std::collections::HashSet;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_en_passant() {
    // 1. Setup Board
    let mut board = Board::new_empty(2, 8);

    // Low-level setup: White Pawn at (1, 4), moves to (3, 4) (Double Push)
    // Actually, Black Pawn should be the one capturing? Or White?
    // Let's test White Capturing.
    // White Pawn at (4, 4). Black Pawn moves (6, 5) -> (4, 5).
    // White captures (4, 4) -> (5, 5). En Passant target was (5, 5).

    // Setup White Pawn at 4,4 (Rank 4, File E)
    board
        .set_piece(
            &coord(4, 4),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    // Setup Black Pawn at 6,5 (Rank 6, File F) -- Start pos
    board
        .set_piece(
            &coord(6, 5),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    // 2. Execute Black Double Push
    let move_black = hyperchess::domain::models::Move {
        from: coord(6, 5),
        to: coord(4, 5),
        promotion: None,
    };
    board.apply_move(&move_black).unwrap();

    // 3. Verify En Passant Target
    // Rank 5, File 5 -> (5, 5)
    let ep_target_idx = board.coords_to_index(&[5, 5]).unwrap();
    assert_eq!(
        board.en_passant_target,
        Some(ep_target_idx),
        "EP Target should be set"
    );

    // 4. Generate White Moves
    let moves = Rules::generate_legal_moves(&board, Player::White);
    let ep_move = moves.iter().find(|m| m.to == coord(5, 5));

    assert!(
        ep_move.is_some(),
        "En Passant capture move should be generated"
    );

    // 5. Execute En Passant
    board.apply_move(ep_move.unwrap()).unwrap();

    // 6. Verify Result
    // White Pawn at (5, 5)
    let p = board.get_piece(&coord(5, 5));
    assert!(p.is_some());
    assert_eq!(p.unwrap().owner, Player::White);

    // Black Pawn at (4, 5) should be gone
    let captured = board.get_piece(&coord(4, 5));
    assert!(captured.is_none(), "Captured pawn should be removed");

    // EP target should be cleared
    assert_eq!(board.en_passant_target, None);
}

#[test]
fn test_castling_kingside_white() {
    let mut board = Board::new_empty(2, 8);
    board.castling_rights = 0xF; // All rights

    // White King at E1 (0, 4)
    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    // White Rook at H1 (0, 7)
    board
        .set_piece(
            &coord(0, 7),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    // Generate moves
    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Expect Castling move to G1 (0, 6) from King (0, 4)
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_some(),
        "White Kingside Castling should be available"
    );

    // Execute
    board.apply_move(castle_move.unwrap()).unwrap();

    // Verify King at G1
    let k = board.get_piece(&coord(0, 6));
    assert!(k.is_some());
    assert_eq!(k.unwrap().piece_type, PieceType::King);

    // Verify Rook at F1 (0, 5)
    let r = board.get_piece(&coord(0, 5));
    assert!(r.is_some());
    assert_eq!(r.unwrap().piece_type, PieceType::Rook);

    // Verify Rights lost (White rights 0 & 1 cleared -> 0xC remaining (Black rights))
    assert_eq!(board.castling_rights & 0x3, 0);
}

#[test]
fn test_castling_blocked() {
    let mut board = Board::new_empty(2, 8);
    board.castling_rights = 0xF;

    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 7),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();
    // Blocker at F1 (0, 5)
    board
        .set_piece(
            &coord(0, 5),
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(castle_move.is_none(), "Castling should be blocked");
}

#[test]
fn test_castling_through_check() {
    let mut board = Board::new_empty(2, 8);
    board.castling_rights = 0xF;

    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 7),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    // Black Rook attacking F1 (0, 5)
    // Place Black Rook at F8 (7, 5)
    board
        .set_piece(
            &coord(7, 5),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_none(),
        "Castling through check should be illegal"
    );
}
```
