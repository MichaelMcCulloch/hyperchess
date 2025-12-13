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
use smallvec::{smallvec, SmallVec};
use std::fmt;
use std::sync::Arc;

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
                let trailing = self.current_chunk.trailing_zeros();

                self.current_chunk &= !(1 << trailing);

                let index = if let BitBoard::Medium(_) = self.board {
                    self.current_chunk_idx * 64 + trailing as usize
                } else if let BitBoard::Large { .. } = self.board {
                    self.current_chunk_idx * 64 + trailing as usize
                } else {
                    trailing as usize
                };

                return Some(index);
            }

            match self.board {
                BitBoard::Small(_) => return None,
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

    pub white_occupancy: BitBoard,
    pub black_occupancy: BitBoard,

    pub pawns: BitBoard,
    pub rooks: BitBoard,
    pub knights: BitBoard,
    pub bishops: BitBoard,
    pub queens: BitBoard,
    pub kings: BitBoard,

    pub zobrist: Arc<ZobristKeys>,
    pub hash: u64,
    pub history: Vec<u64>,
    pub en_passant_target: Option<(usize, usize)>,
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
        board.castling_rights = 0xF;
        board.setup_standard_chess();
        board
    }

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
            let mut white_coords = vec![0; self.dimension];
            white_coords[1] = file_y;

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

            let mut black_coords = vec![self.side - 1; self.dimension];

            black_coords[1] = file_y;

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

        let saved_ep = self.en_passant_target;
        let saved_castling = self.castling_rights;
        let mut captured = None;

        if let Some(target_p) = self.get_piece_at_index(to_idx) {
            captured = Some((to_idx, target_p));
        }

        self.history.push(self.hash);

        if moving_piece.piece_type == PieceType::Pawn {
            if let Some((target, victim)) = self.en_passant_target {
                if to_idx == target {
                    if let Some(victim_p) = self.get_piece_at_index(victim) {
                        captured = Some((victim, victim_p));
                    }
                    self.remove_piece_at_index(victim);
                }
            }
        }

        self.en_passant_target = None;

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

        let mut castling_rook_move: Option<(usize, usize, Piece)> = None;

        if moving_piece.piece_type == PieceType::King {
            match moving_piece.owner {
                Player::White => self.castling_rights &= !0x3,
                Player::Black => self.castling_rights &= !0xC,
            }
        }

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
        if let Some(h) = self.history.pop() {
            self.hash = h;
        }

        self.en_passant_target = info.en_passant_target;
        self.castling_rights = info.castling_rights;

        let from_idx = self.coords_to_index(&mv.from.values).unwrap();
        let to_idx = self.coords_to_index(&mv.to.values).unwrap();

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

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
                let len = std::cmp::min(data.len(), other_data.len());

                for (d, o) in data[0..len].iter_mut().zip(&other_data[0..len]) {
                    *d |= *o;
                }

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
```
```./src/domain/coordinate.rs
use smallvec::SmallVec;
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Coordinate {
    pub values: SmallVec<[usize; 4]>,
}

impl Coordinate {
    pub fn new<I: Into<SmallVec<[usize; 4]>>>(values: I) -> Self {
        Self {
            values: values.into(),
        }
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
pub mod game;
pub mod models;
pub mod rules;
pub mod services;
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
use smallvec::SmallVec;

pub type MoveList = SmallVec<[Move; 64]>;

pub struct Rules;

impl Rules {
    pub fn generate_legal_moves(board: &mut Board, player: Player) -> MoveList {
        let mut moves = MoveList::new();
        let pseudo_legal = Self::generate_pseudo_legal_moves(board, player);

        for mv in pseudo_legal {
            if !Self::leaves_king_in_check(board, player, &mv) {
                moves.push(mv);
            }
        }

        Self::generate_castling_moves(board, player, &mut moves);
        moves
    }

    pub fn is_square_attacked(board: &Board, square: &Coordinate, by_player: Player) -> bool {
        let _index = if let Some(idx) = board.coords_to_index(&square.values) {
            idx
        } else {
            return false;
        };

        let enemy_occupancy = match by_player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        let dimension = board.dimension;
        let side = board.side;

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
        let mut current: SmallVec<[usize; 4]> = SmallVec::from_slice(origin_vals);
        let enemy_occupancy = match attacker {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        loop {
            if let Some(next) = Self::apply_offset(&current, direction, board.side) {
                if let Some(idx) = board.coords_to_index(&next) {
                    if all_occupancy.get_bit(idx) {
                        if enemy_occupancy.get_bit(idx) {
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
                            return false;
                        } else {
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

    fn generate_pseudo_legal_moves(board: &Board, player: Player) -> MoveList {
        let mut moves = MoveList::new();
        let occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for i in occupancy.iter_indices() {
            let coord_vals = board.index_to_coords(i);
            let coord = Coordinate::new(coord_vals.clone());

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
                continue;
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
        moves
    }

    fn leaves_king_in_check(board: &mut Board, player: Player, mv: &Move) -> bool {
        let info = match board.apply_move(mv) {
            Ok(i) => i,
            Err(_) => return true,
        };

        let in_check = if let Some(king_pos) = board.get_king_coordinate(player) {
            Self::is_square_attacked(board, &king_pos, player.opponent())
        } else {
            false
        };

        board.unmake_move(mv, info);
        in_check
    }

    fn generate_castling_moves(board: &Board, player: Player, moves: &mut MoveList) {
        if board.side != 8 {
            return;
        }

        let (rights_mask, rank) = match player {
            Player::White => (0x3, 0),
            Player::Black => (0xC, board.side - 1),
        };

        let my_rights = board.castling_rights & rights_mask;
        if my_rights == 0 {
            return;
        }

        let king_file = 4;
        let mut king_coords = vec![rank; board.dimension];
        king_coords[1] = king_file;
        let king_coord = Coordinate::new(king_coords.clone());

        if Self::is_square_attacked(board, &king_coord, player.opponent()) {
            return;
        }

        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        let ks_mask = match player {
            Player::White => 0x1,
            Player::Black => 0x4,
        };
        if (my_rights & ks_mask) != 0 {
            let f_file = 5;
            let g_file = 6;

            let mut f_coords = king_coords.clone();
            f_coords[1] = f_file;
            let mut g_coords = king_coords.clone();
            g_coords[1] = g_file;

            let f_idx = board.coords_to_index(&f_coords);
            let g_idx = board.coords_to_index(&g_coords);

            let mut blocked = true;
            if let (Some(fi), Some(gi)) = (f_idx, g_idx) {
                if !all_occupancy.get_bit(fi) && !all_occupancy.get_bit(gi) {
                    blocked = false;
                }
            }

            if !blocked {
                if !Self::is_square_attacked(board, &Coordinate::new(f_coords), player.opponent())
                    && !Self::is_square_attacked(
                        board,
                        &Coordinate::new(g_coords.clone()),
                        player.opponent(),
                    )
                {
                    moves.push(Move {
                        from: king_coord.clone(),
                        to: Coordinate::new(g_coords),
                        promotion: None,
                    });
                }
            }
        }

        let qs_mask = match player {
            Player::White => 0x2,
            Player::Black => 0x8,
        };
        if (my_rights & qs_mask) != 0 {
            let b_file = 1;
            let c_file = 2;
            let d_file = 3;

            let mut b_coords = king_coords.clone();
            b_coords[1] = b_file;
            let mut c_coords = king_coords.clone();
            c_coords[1] = c_file;
            let mut d_coords = king_coords.clone();
            d_coords[1] = d_file;

            let b_idx = board.coords_to_index(&b_coords);
            let c_idx = board.coords_to_index(&c_coords);
            let d_idx = board.coords_to_index(&d_coords);

            let mut blocked = true;
            if let (Some(bi), Some(ci), Some(di)) = (b_idx, c_idx, d_idx) {
                if !all_occupancy.get_bit(bi)
                    && !all_occupancy.get_bit(ci)
                    && !all_occupancy.get_bit(di)
                {
                    blocked = false;
                }
            }

            if !blocked {
                if !Self::is_square_attacked(board, &Coordinate::new(d_coords), player.opponent())
                    && !Self::is_square_attacked(
                        board,
                        &Coordinate::new(c_coords.clone()),
                        player.opponent(),
                    )
                {
                    moves.push(Move {
                        from: king_coord.clone(),
                        to: Coordinate::new(c_coords),
                        promotion: None,
                    });
                }
            }
        }
    }

    fn get_rook_directions(dimension: usize) -> Vec<Vec<isize>> {
        let mut dirs = Vec::new();

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
        let mut offsets = Vec::new();

        for i in 0..dimension {
            for j in 0..dimension {
                if i == j {
                    continue;
                }

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
        let direction = match attacker {
            Player::White => -1,
            Player::Black => 1,
        };

        let mut offsets = Vec::new();

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

    fn apply_offset(
        coords: &[usize],
        offset: &[isize],
        side: usize,
    ) -> Option<SmallVec<[usize; 4]>> {
        let mut new_coords = SmallVec::with_capacity(coords.len());
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
        moves: &mut MoveList,
    ) {
        let same_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for offset in offsets {
            if let Some(target_coords) = Self::apply_offset(&origin.values, offset, board.side) {
                if let Some(target_idx) = board.coords_to_index(&target_coords) {
                    if !same_occupancy.get_bit(target_idx) {
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
        moves: &mut MoveList,
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
                            break;
                        }

                        moves.push(Move {
                            from: origin.clone(),
                            to: Coordinate::new(next.clone()),
                            promotion: None,
                        });

                        if enemy_occupancy.get_bit(idx) {
                            break;
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
        moves: &mut MoveList,
    ) {
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        let enemy_occupancy = match player.opponent() {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for movement_axis in 0..board.dimension {
            if movement_axis == 1 {
                continue;
            }

            let forward_dir = match player {
                Player::White => 1,
                Player::Black => -1,
            };

            let mut forward_step = vec![0; board.dimension];
            forward_step[movement_axis] = forward_dir;

            if let Some(target) = Self::apply_offset(&origin.values, &forward_step, board.side) {
                if let Some(idx) = board.coords_to_index(&target) {
                    if !all_occupancy.get_bit(idx) {
                        Self::add_pawn_move(origin, &target, board.side, player, moves);

                        let is_start_rank = match player {
                            Player::White => origin.values[movement_axis] == 1,
                            Player::Black => origin.values[movement_axis] == board.side - 2,
                        };

                        if is_start_rank {
                            if let Some(target2) =
                                Self::apply_offset(&target, &forward_step, board.side)
                            {
                                if let Some(idx2) = board.coords_to_index(&target2) {
                                    if !all_occupancy.get_bit(idx2) {
                                        Self::add_pawn_move(
                                            origin, &target2, board.side, player, moves,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            for capture_axis in 0..board.dimension {
                if capture_axis == movement_axis {
                    continue;
                }
                for s in [-1, 1] {
                    let mut cap_step = forward_step.clone();
                    cap_step[capture_axis] = s;

                    if let Some(target) = Self::apply_offset(&origin.values, &cap_step, board.side)
                    {
                        if let Some(idx) = board.coords_to_index(&target) {
                            if enemy_occupancy.get_bit(idx) {
                                Self::add_pawn_move(origin, &target, board.side, player, moves);
                            } else if let Some((ep_target, _)) = board.en_passant_target {
                                if idx == ep_target {
                                    moves.push(Move {
                                        from: origin.clone(),
                                        to: Coordinate::new(target),
                                        promotion: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn add_pawn_move(
        from: &Coordinate,
        to_vals: &[usize],
        side: usize,
        player: Player,
        moves: &mut MoveList,
    ) {
        let is_promotion = (0..to_vals.len()).all(|i| {
            if i == 1 {
                true
            } else {
                match player {
                    Player::White => to_vals[i] == side - 1,
                    Player::Black => to_vals[i] == 0,
                }
            }
        });

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
use crate::domain::board::Board;
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

        let mut en_passant_keys = Vec::with_capacity(total_cells);
        for _ in 0..total_cells {
            en_passant_keys.push(rng.r#gen());
        }

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

        if let Some((ep_target, _)) = board.en_passant_target {
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
use crate::domain::board::{Board, UnmakeInfo};
use crate::domain::models::{Move, Player};
use crate::domain::rules::{MoveList, Rules};
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use rand::seq::SliceRandom;
use std::sync::Arc;

use std::f64;

const UCT_C: f64 = 1.4142;
const CHECKMATE_SCORE: i32 = 30000;

struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    visits: u32,
    score: f64,
    unexpanded_moves: MoveList,
    is_terminal: bool,
    move_to_node: Option<Move>,
    player_to_move: Player,
}

pub struct MCTS {
    nodes: Vec<Node>,
    root_player: Player,
    tt: Option<Arc<LockFreeTT>>,
    serial: bool,
}

use rayon::prelude::*;

impl MCTS {
    pub fn new(root_state: &Board, root_player: Player, tt: Option<Arc<LockFreeTT>>) -> Self {
        let mut root_clone = root_state.clone();
        let mut moves = Rules::generate_legal_moves(&mut root_clone, root_player);
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
            serial: false,
        }
    }

    pub fn with_serial(mut self) -> Self {
        self.serial = true;
        self
    }

    pub fn run(&mut self, root_state: &Board, iterations: usize) -> f64 {
        if iterations == 0 {
            return 0.5;
        }

        let num_threads = rayon::current_num_threads();
        // User requested strategy: "chunk work, maybe 5 iterations a thread".
        // We ensure at least 5 iterations per task to amortize the setup cost (Board clone).
        const MIN_ITERATIONS_PER_TASK: usize = 5;

        let num_tasks = if self.serial {
            1
        } else {
            (iterations / MIN_ITERATIONS_PER_TASK).clamp(1, num_threads)
        };

        if num_tasks <= 1 {
            self.execute_iterations(root_state, iterations);
            let root = &self.nodes[0];
            return if root.visits == 0 {
                0.5
            } else {
                root.score / root.visits as f64
            };
        }

        let chunk_size = iterations / num_tasks;
        let remainder = iterations % num_tasks;

        let results: Vec<(u32, f64)> = (0..num_tasks)
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

                let mut local_mcts = MCTS::new(root_state, self.root_player, self.tt.clone());
                local_mcts.execute_iterations(root_state, count);

                let root = &local_mcts.nodes[0];
                (root.visits, root.score)
            })
            .collect();

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

        let mut current_state = root_state.clone();

        for _ in 0..iterations {
            let mut node_idx = 0;
            let mut current_player = self.root_player;

            let mut path_stack: Vec<(Move, UnmakeInfo)> = Vec::with_capacity(64);

            while self.nodes[node_idx].unexpanded_moves.is_empty()
                && !self.nodes[node_idx].children.is_empty()
            {
                let best_child = self.select_child(node_idx);
                node_idx = best_child;

                let mv = self.nodes[node_idx].move_to_node.as_ref().unwrap();

                let info = current_state.apply_move(mv).unwrap();
                path_stack.push((mv.clone(), info));

                current_player = current_player.opponent();
            }

            if !self.nodes[node_idx].unexpanded_moves.is_empty() {
                let mv = self.nodes[node_idx].unexpanded_moves.pop().unwrap();

                let info = current_state.apply_move(&mv).unwrap();
                path_stack.push((mv.clone(), info));

                let next_player = current_player.opponent();

                let legal_moves = Rules::generate_legal_moves(&mut current_state, next_player);
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
                current_player = next_player;
            }

            let result_score = if self.nodes[node_idx].is_terminal {
                self.evaluate_terminal(&current_state, current_player)
            } else {
                self.rollout_inplace(
                    &mut current_state,
                    current_player,
                    &mut rng,
                    &mut path_stack,
                )
            };

            self.backpropagate(node_idx, result_score);

            while let Some((mv, info)) = path_stack.pop() {
                current_state.unmake_move(&mv, info);
            }
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

    fn rollout_inplace(
        &self,
        state: &mut Board,
        mut player: Player,
        rng: &mut rand::rngs::ThreadRng,
        stack: &mut Vec<(Move, UnmakeInfo)>,
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
            let info = state.apply_move(mv).unwrap();
            stack.push((mv.clone(), info));

            player = player.opponent();
            depth += 1;
        }

        0.5
    }

    fn evaluate_terminal(&self, state: &Board, player_at_leaf: Player) -> f64 {
        if let Some(king_pos) = state.get_king_coordinate(player_at_leaf) {
            if Rules::is_square_attacked(state, &king_pos, player_at_leaf.opponent()) {
                if let Some(tt) = &self.tt {
                    tt.store(state.hash, -CHECKMATE_SCORE, 255, Flag::Exact, None);
                }

                if player_at_leaf == self.root_player {
                    return 0.0;
                } else {
                    return 1.0;
                }
            }
        }

        if let Some(tt) = &self.tt {
            tt.store(state.hash, 0, 255, Flag::Exact, None);
        }

        0.5
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
        let board = Board::new(2, 8);
        let mut mcts = MCTS::new(&board, Player::White, None);
        let score = mcts.run(&board, 10);
        assert!(score >= 0.0 && score <= 1.0);
    }

    #[test]
    fn test_mcts_parallel_execution() {
        let board = Board::new(2, 8);
        let mut mcts = MCTS::new(&board, Player::White, None);

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
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CHECKMATE_SCORE: i32 = 30000;
const TIMEOUT_CHECK_INTERVAL: usize = 2048;

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
    use_mcts: bool,
    mcts_iterations: usize,
    num_threads: usize,
}

impl MinimaxBot {
    pub fn new(depth: usize, time_limit_ms: u64, _dimension: usize, _side: usize) -> Self {
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).max(1))
            .unwrap_or(1);

        Self {
            depth,
            time_limit: Duration::from_millis(time_limit_ms),
            tt: Arc::new(LockFreeTT::new(256)), // Increased TT size for parallel access
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: std::sync::atomic::AtomicUsize::new(0),
            use_mcts: false,
            mcts_iterations: 100,
            num_threads,
        }
    }

    pub fn with_mcts(mut self, iterations: usize) -> Self {
        self.use_mcts = true;
        self.mcts_iterations = iterations;
        // Adjust depth for hybrid approach
        self.depth = if self.use_mcts { 3 } else { self.depth };
        self
    }

    fn evaluate(&self, board: &Board, player_at_leaf: Option<Player>) -> i32 {
        if self.use_mcts {
            if let Some(player) = player_at_leaf {
                // Critical: Run MCTS serially here!
                // We are already inside a parallel Minimax thread.
                let mut mcts = MCTS::new(board, player, None).with_serial();
                let win_rate = mcts.run(board, self.mcts_iterations);

                let val_f = (win_rate - 0.5) * 2.0 * (VAL_KING as f64);
                let val = val_f as i32;

                return if player == Player::Black { -val } else { val };
            }
        }

        let mut score = 0;
        for i in board.white_occupancy.iter_indices() {
            score += self.get_piece_value(board, i);
        }
        for i in board.black_occupancy.iter_indices() {
            score -= self.get_piece_value(board, i);
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
                return 0;
            }
        }
        if self.stop_flag.load(Ordering::Relaxed) {
            return 0;
        }

        let hash = board.hash;

        // LAZY SMP: Check TT for cutoffs from OTHER threads
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
            return 0;
        }

        // MOVE ORDERING (Basic for now)
        // Ideally we would prioritize captures, etc.
        // For now, relies on TT updates from other threads to narrow the window.

        // Local variable for Best Score
        let mut best_score = -i32::MAX;
        let original_alpha = alpha;

        for mv in moves {
            let info = match board.apply_move(&mv) {
                Ok(i) => i,
                Err(_) => continue,
            };

            let score = -self.minimax(
                board,
                depth - 1,
                -beta,
                -alpha,
                player.opponent(),
                start_time,
            );

            board.unmake_move(&mv, info);

            if self.stop_flag.load(Ordering::Relaxed) {
                return 0;
            }

            if score > best_score {
                best_score = score;
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                break; // Beta Cutoff
            }
        }

        // Store result in shared TT
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

        // Generate Root Moves
        let root_moves = Rules::generate_legal_moves(&mut board.clone(), player);
        if root_moves.is_empty() {
            return None;
        }

        // LAZY SMP ENTRY POINT
        // We launch N threads. They all run the search (Iterative Deepening).
        // To ensure they don't do identical work, we shuffle root moves differently for each thread.
        let results: Vec<(Move, i32)> = (0..self.num_threads)
            .into_par_iter()
            .map(|thread_idx| {
                let mut local_board = board.clone();
                let mut local_best_move = None;
                let mut local_best_score = -i32::MAX;

                // Optional: Shuffle root moves differently per thread to encourage
                // different traversal orders (Lazy SMP diversity)
                let mut my_moves = root_moves.clone();
                if thread_idx > 0 {
                    use rand::seq::SliceRandom;
                    let mut rng = rand::thread_rng();
                    my_moves.shuffle(&mut rng);
                }

                // Iterative Deepening
                for d in 1..=self.depth {
                    let mut alpha = -i32::MAX;
                    let beta = i32::MAX;
                    let mut best_score_this_depth = -i32::MAX;
                    let mut best_move_this_depth = None;

                    for mv in &my_moves {
                        let info = local_board.apply_move(mv).unwrap();

                        let score = -self.minimax(
                            &mut local_board,
                            d - 1,
                            -beta,
                            -alpha,
                            player.opponent(),
                            start_time,
                        );

                        local_board.unmake_move(mv, info);

                        if self.stop_flag.load(Ordering::Relaxed) {
                            break;
                        }

                        if score > best_score_this_depth {
                            best_score_this_depth = score;
                            best_move_this_depth = Some(mv.clone());
                        }
                        alpha = alpha.max(score);
                    }

                    if !self.stop_flag.load(Ordering::Relaxed) {
                        local_best_score = best_score_this_depth;
                        local_best_move = best_move_this_depth;
                    } else {
                        break;
                    }
                }

                (
                    local_best_move.unwrap_or(my_moves[0].clone()),
                    local_best_score,
                )
            })
            .collect();

        // Aggregate results: Pick the move with the highest score found by ANY thread.
        // Lazy SMP works because threads share the TT. If one finds a good move, others see it.
        // We take the max score from all threads.
        let best = results.into_iter().max_by_key(|r| r.1);

        best.map(|(m, _)| m)
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
    pub promotion: u8,
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

        let entry_hash = (entry >> 32) as u32;
        if entry_hash != (hash >> 32) as u32 {
            return None;
        }

        let data = entry as u32;

        let score = (data & 0xFFFF) as i16 as i32;
        let depth = ((data >> 16) & 0xFF) as u8;
        let flag_u8 = ((data >> 24) & 0x3) as u8;

        let flag = match flag_u8 {
            0 => Flag::Exact,
            1 => Flag::LowerBound,
            2 => Flag::UpperBound,
            _ => Flag::Exact,
        };

        Some((score, depth, flag, None))
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

                    let promotion = if parts.len() > 2 {
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
        if s.contains('\x1b') {
            // ANSI strings are treated as atomic (length 1 visual)
            if x < self.width && y < self.height {
                self.buffer[y * self.width + x] = s.to_string();
            }
        } else {
            // Non-ANSI strings are split into chars
            for (i, c) in s.chars().enumerate() {
                let curr_x = x + i;
                if curr_x < self.width && y < self.height {
                    self.buffer[y * self.width + curr_x] = c.to_string();
                }
            }
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
    // Pre-calculate size to allocate canvas
    let (w, h, _) = calculate_metrics(dim, side, true, true);
    let mut canvas = Canvas::new(w, h);

    draw_recursive(board, dim, &mut canvas, 0, 0, 0, true, true);

    canvas.put(0, 0, "HELLO");

    eprintln!("DUMP W={} H={}", w, h);
    for y in 0..h {
        let mut row = String::new();
        for x in 0..w {
            let val = &canvas.buffer[y * w + x];
            row.push('|');
            row.push_str(if val == " " { "_" } else { val });
        }
        row.push('|');
        eprintln!("R{}: {}", y, row);
    }

    canvas.to_string()
}

// Returns (width, height, content_offset_y)
fn calculate_metrics(
    dim: usize,
    side: usize,
    is_top: bool,
    is_left: bool,
) -> (usize, usize, usize) {
    let res = if dim == 0 {
        (1, 1, 0)
    } else if dim == 1 {
        (side, 1, 0)
    } else if dim == 2 {
        let has_col_labels = is_top;
        let has_row_labels = is_left;

        let body_w = side * 2 - 1;
        let body_h = side;

        let label_w = if has_row_labels { 2 } else { 0 };
        let label_h = if has_col_labels { 1 } else { 0 };

        (body_w + label_w, body_h + label_h, label_h)
    } else if dim % 2 != 0 {
        // Odd dimension (Horizontal stack)
        // Labels (Top): "11", "12"...
        let has_labels = is_top;
        let label_h = if has_labels { 1 } else { 0 };
        let gap = 2;

        // Children:
        // Child 0 inherits is_left.
        // Others have is_left = false.
        // All inherit is_top.

        let (c0_w, c0_h, c0_off_y) = calculate_metrics(dim - 1, side, is_top, is_left);
        // We assume all children have same height/offset because is_top is shared
        // But width might differ due to is_left
        let (other_w, _, _) = calculate_metrics(dim - 1, side, is_top, false);

        let total_w = c0_w + (side - 1) * (other_w + gap);
        // Note: is_top is shared, so all children include their headers in their height.
        // But we ALSO add OUR header if is_top.
        let total_h = c0_h + label_h;
        let content_off_y = label_h + c0_off_y;

        (total_w, total_h, content_off_y)
    } else {
        // Even dimension (Vertical stack)
        // Labels (Left): "AA", "AB"...
        let has_labels = is_left;
        let label_w = if has_labels { 5 } else { 0 };
        // Gap is 0 for tight packing

        let (c0_w, c0_h, c0_off_y) = calculate_metrics(dim - 1, side, is_top, is_left);
        let (other_w, other_h, _) = calculate_metrics(dim - 1, side, false, is_left);

        // Max width
        let max_child_w = std::cmp::max(c0_w, other_w);
        let total_w = max_child_w + label_w;

        // Height sum
        // Let's enforce gap=1.
        let actual_gap = 1;
        let total_h = c0_h + (side - 1) * (other_h + actual_gap);

        (total_w, total_h, c0_off_y)
    };
    res
}

fn draw_recursive(
    board: &Board,
    current_dim: usize,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
    is_top: bool,
    is_left: bool,
) {
    let side = board.side();

    if current_dim == 2 {
        let has_col_labels = is_top;
        let has_row_labels = is_left;

        let col_label_h = if has_col_labels { 1 } else { 0 };
        let row_label_w = if has_row_labels { 2 } else { 0 };

        if has_col_labels {
            for dx in 0..side {
                let label = format!("{}", dx + 1);
                let label_x = x + row_label_w + dx * 2;
                canvas.put(label_x, y, &label);
            }
        }

        for dy in 0..side {
            if has_row_labels {
                let row_char = (b'A' + dy as u8) as char;
                let label_str = format!("{}", row_char);
                // "A "
                canvas.put(x, y + col_label_h + dy, &label_str);
            }

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
                canvas.put(x + row_label_w + dx * 2, y + col_label_h + dy, &s);
            }
        }
        return;
    }

    let stride = side.pow((current_dim - 1) as u32);

    if current_dim % 2 != 0 {
        // Odd (Horizontal)
        let has_labels = is_top;
        let label_h = if has_labels { 1 } else { 0 };
        let gap = 2;
        let prefix_digit = (current_dim - 1) / 2;

        let mut current_x = x;
        // All children share is_top.
        // First child is_left, others false.

        for i in 0..side {
            let child_is_top = is_top;
            let child_is_left = is_left && (i == 0);
            let next_base = base_index + i * stride;

            let (child_w, child_h, _) =
                calculate_metrics(current_dim - 1, side, child_is_top, child_is_left);

            // Draw Header if top
            if has_labels {
                let label_val = i + 1;
                let label = format!("{}{}", prefix_digit, label_val);
                // Center label over child
                let label_len = label.len();
                let center_offset = if child_w > label_len {
                    (child_w - label_len) / 2
                } else {
                    0
                };
                eprintln!(
                    "Dim 3: x={} child_w={} label_len={} -> offset={} pos={}",
                    current_x,
                    child_w,
                    label_len,
                    center_offset,
                    current_x + center_offset
                );
                canvas.put(current_x + center_offset, y, &label);
            }

            draw_recursive(
                board,
                current_dim - 1,
                canvas,
                current_x,
                y + label_h,
                next_base,
                child_is_top,
                child_is_left,
            );

            if i < side - 1 {
                // Separator
                let sep_x = current_x + child_w + gap / 2 - 1;
                for k in 0..child_h {
                    canvas.put(
                        sep_x,
                        y + label_h + k,
                        &format!("{}|{}", COLOR_DIM, COLOR_RESET),
                    );
                }
                current_x += child_w + gap;
            }
        }
    } else {
        // Even (Vertical)
        let has_labels = is_left;
        let label_w = if has_labels { 5 } else { 0 };
        let gap = 1;
        let prefix_idx = (current_dim - 2) / 2 - 1;
        let prefix_char = (b'A' + prefix_idx as u8) as char;

        let mut current_y = y;

        for i in 0..side {
            let child_is_top = is_top && (i == 0);
            let child_is_left = is_left;
            let next_base = base_index + i * stride;

            eprintln!(
                "Dim 4 loop i={}: x={} label_w={} -> child_x={}",
                i,
                x,
                label_w,
                x + label_w
            );

            let (child_w, child_h, child_content_off) =
                calculate_metrics(current_dim - 1, side, child_is_top, child_is_left);

            if has_labels {
                // Draw Label "AA"
                let suffix_char = (b'A' + i as u8) as char;
                let label = format!("{}{}", prefix_char, suffix_char);
                // "AA"
                // Align with content offset
                canvas.put(x, current_y + child_content_off, &label);
            }

            draw_recursive(
                board,
                current_dim - 1,
                canvas,
                x + label_w,
                current_y,
                next_base,
                child_is_top,
                child_is_left,
            );

            if i < side - 1 {
                let sep_y = current_y + child_h;
                // Draw separator
                // Width = child_w? We should match child width.
                // Or max width?
                // Visual indicates separator is typically same width as board row.
                for k in 0..child_w {
                    canvas.put(
                        x + label_w + k,
                        sep_y,
                        &format!("{}-{}", COLOR_DIM, COLOR_RESET),
                    );
                }
                current_y += child_h + gap;
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

    let mut dimension = 2;
    let side = 8;
    let mut player_white_type = "h";
    let mut player_black_type = "c";
    let mut depth = 4;
    let time_limit = 1000;

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
    let dim = 2;
    let side = 8;
    let mut board = Board::new_empty(dim, side);

    use hyperchess::domain::coordinate::Coordinate;
    use hyperchess::domain::models::{Piece, PieceType};

    let pawn = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    let start_idx = 8;
    let start_coord = Coordinate::new(board.index_to_coords(start_idx));

    board.set_piece(&start_coord, pawn).unwrap();

    let w_king = Piece {
        piece_type: PieceType::King,
        owner: Player::White,
    };
    let b_king = Piece {
        piece_type: PieceType::King,
        owner: Player::Black,
    };

    let w_king_coord = Coordinate::new(vec![0, 0]);
    let b_king_coord = Coordinate::new(vec![7, 7]);

    board.set_piece(&w_king_coord, w_king).unwrap();
    board.set_piece(&b_king_coord, b_king).unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    assert!(!moves.is_empty(), "Should generate moves");

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
```./tests/castling_general.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord_2d(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

fn coord_3d(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_castling_standard_8x8() {
    let side = 8;
    let dim = 2;
    let mut board = Board::new_empty(dim, side);
    board.castling_rights = 0xF;

    let king_pos = coord_2d(0, 4);
    let rook_pos = coord_2d(0, 7);

    board
        .set_piece(
            &king_pos,
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &rook_pos,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let castling_target = coord_2d(0, 6);
    let castle_move = moves
        .iter()
        .find(|m| m.to == castling_target && m.from == king_pos);

    assert!(castle_move.is_some(), "Should allow castling on 8x8 board");

    board.apply_move(castle_move.unwrap()).unwrap();

    assert!(board.get_piece(&castling_target).is_some());

    let rook_coord = coord_2d(0, 5);
    let rook_piece = board.get_piece(&rook_coord);
    assert!(rook_piece.is_some(), "Rook should be at F1 (0,5)");
    assert_eq!(rook_piece.unwrap().piece_type, PieceType::Rook);
}

#[test]
fn test_castling_3d_blocked() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);
    board.castling_rights = 0xF;

    let king_pos = coord_3d(0, 4, 0);

    board
        .set_piece(
            &king_pos,
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord_3d(0, 7, 0),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord_3d(0, 5, 0),
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let castling_target = coord_3d(0, 6, 0);
    let castle_move = moves
        .iter()
        .find(|m| m.to == castling_target && m.from == king_pos);

    if castle_move.is_some() {
        eprintln!("Castle move found: {:?}", castle_move.unwrap());
        eprintln!("All moves: {:?}", moves);
    }

    assert!(
        castle_move.is_none(),
        "Castling should be blocked on 3D board path"
    );
}
```
```./tests/display_test.rs
use hyperchess::domain::board::Board;
use hyperchess::infrastructure::display::render_board;

#[test]
fn test_display_labels_2d() {
    let board = Board::new(2, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check Column Labels
    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));

    // Check Row Labels
    assert!(output.contains("A"));
    assert!(output.contains("B"));
    assert!(output.contains("C"));
}

#[test]
fn test_display_labels_3d() {
    let board = Board::new(3, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check Dimension Labels (Horizontal: 11, 12, 13)
    // "1" prefix + index 1..3
    assert!(output.contains("11"));
    assert!(output.contains("12"));
    assert!(output.contains("13"));

    // Check internal 2D labels
    assert!(output.contains("A"));
    assert!(output.contains("1"));
}

#[test]
fn test_display_labels_4d() {
    let board = Board::new(4, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check Dimension Labels (Vertical: AA, AB, AC)
    // "A" prefix + char A..C
    assert!(output.contains("AA"));
    assert!(output.contains("AB"));
    assert!(output.contains("AC"));
}
```
```./tests/display_verification.rs
#[cfg(test)]
mod tests {
    use hyperchess::domain::board::Board;
    use hyperchess::infrastructure::display::render_board;

    #[test]
    fn test_label_rendering_4d() {
        // Create a 4D board with small side length to keep output manageable
        // Dim 4, Side 2
        // Structure:
        // Vertical (Dim 4): AA, AB
        //   Horizontal (Dim 3): 11, 12
        //     Board (Dim 2)

        // We expect:
        // AA (Top):
        //   11 (Left): Should have Top Labels (1,2) and Left Labels (A,B)
        //   12 (Right): Should have Top Labels (1,2) but NO Left Labels
        // AB (Bottom):
        //   11 (Left): Should have NO Top Labels but HAVE Left Labels (A,B)
        //   12 (Right): Should have NO Top Labels and NO Left Labels

        let board = Board::new(4, 2);
        let output = render_board(&board);
        let expected = r###"      11   12  
      1 2    
AA  A ♕ ♙| . .
    B ♔ ♙| . .
      --------
AB    . .| . ♛
      . .| . ♚"###;

        let strip_ansi = |s: &str| -> String {
            let mut result = String::new();
            let mut in_escape = false;
            for c in s.chars() {
                if c == '\x1b' {
                    in_escape = true;
                }
                if !in_escape {
                    result.push(c);
                }
                if in_escape && c == 'm' {
                    in_escape = false;
                }
            }
            result
        };

        let output_clean = strip_ansi(&output);
        println!("{}", output_clean);
        println!("{}", expected);

        assert_eq!(
            expected, output_clean,
            "File labels '1 2' should appear exactly twice (top row only)"
        );
    }
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

    assert!(
        is_piece_at(&board, &coord(0, 4), PieceType::King, Player::White),
        "White King at (0,4)"
    );

    assert!(
        is_piece_at(&board, &coord(0, 3), PieceType::Queen, Player::White),
        "White Queen at (0,3)"
    );

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

    assert!(board.get_piece(&coord(3, 3)).is_none());
    assert!(board.get_piece(&coord(4, 4)).is_none());
}

#[test]
fn test_3d_setup() {
    let board = Board::new(3, 4);

    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![0, 2, 0]),
            PieceType::King,
            Player::White
        ),
        "White King at (0, 2, 0)"
    );

    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![3, 2, 3]),
            PieceType::King,
            Player::Black
        ),
        "Black King at (3, 2, 3)"
    );

    assert!(
        board.get_piece(&Coordinate::new(vec![0, 2, 1])).is_none(),
        "Should be empty at z=1"
    );
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
    let board = Board::new(3, 4);
    let mut mcts = MCTS::new(&board, Player::White, None);
    let win_rate = mcts.run(&board, 50);

    assert!(win_rate >= 0.0);
    assert!(win_rate <= 1.0);
    println!("MCTS Win Rate: {}", win_rate);
}

#[test]
fn test_mcts_checkmate_detection() {
    let board = Board::new(2, 8);

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
    let mut board = Board::new_empty(2, 4);

    board
        .set_piece(
            &coord(3, 3),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();

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

    board
        .set_piece(
            &coord(2, 2),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

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

    board
        .set_piece(
            &coord(0, 3),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::White,
            },
        )
        .unwrap();

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

    let mut bot = MinimaxBot::new(2, 1000, 2, 4);
    let mv = bot
        .get_move(&board, Player::White)
        .expect("Should return a move");

    assert!(
        mv.to == coord(0, 1) || mv.to == coord(2, 1),
        "Should find checkmate move (Queen to (0,1) or King to (2,1)), found {:?}",
        mv.to
    );
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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let mate_move = moves.iter().find(|m| m.to == coord(0, 1));
    assert!(mate_move.is_some(), "Move to (0,1) should be legal");

    board.apply_move(mate_move.unwrap()).unwrap();

    let black_moves = Rules::generate_legal_moves(&mut board, Player::Black);
    assert!(
        black_moves.is_empty(),
        "Black should have no moves after Checkmate"
    );

    let black_king = board.get_king_coordinate(Player::Black).unwrap();
    assert!(
        Rules::is_square_attacked(&board, &black_king, Player::White),
        "Black King should be in check"
    );
}

#[test]
fn test_avoid_immediate_mate() {}
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

    let pawn_pos = coord(1, 3);
    let p = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    board.set_piece(&pawn_pos, p).unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(
        dests.contains(&coord(2, 3)),
        "Should have single push on rank"
    );
    assert!(
        dests.contains(&coord(3, 3)),
        "Should have double push on rank"
    );
    assert!(
        !dests.contains(&coord(1, 4)),
        "Should NOT have single push on file (Lateral forbidden)"
    );
    assert_eq!(dests.len(), 2, "Should have 2 moves (2 Rank pushes)");
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
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    assert_eq!(
        moves.len(),
        0,
        "Pawn blocked on rank and forbidden on file should have no moves"
    );
}

#[test]
fn test_pawn_capture() {
    let mut board = Board::new_empty(2, 8);
    let pawn_pos = coord(3, 3);
    let enemy_pos = coord(4, 4);

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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord(4, 3)), "Single push rank");
    assert!(!dests.contains(&coord(3, 4)), "Single push file forbidden");
    assert!(dests.contains(&coord(4, 4)), "Capture intersection");

    assert_eq!(dests.len(), 2, "Should have 2 moves (1 push + 1 capture)");
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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    assert_eq!(moves.len(), 8);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

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

    board
        .set_piece(
            &coord(4, 6),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let rook_moves: Vec<_> = moves.into_iter().filter(|m| m.from == pos).collect();
    let dests: HashSet<Coordinate> = rook_moves.iter().map(|m| m.to.clone()).collect();

    assert_eq!(rook_moves.len(), 12);
    assert!(!dests.contains(&coord(4, 6)));
    assert!(!dests.contains(&coord(4, 7)));
}

#[test]
fn test_bishop_moves() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(0, 0);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 2, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(0, 0, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(2, 1, 2)), "2D diagonal xz");
    assert!(dests.contains(&coord3(1, 2, 2)), "2D diagonal yz");

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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 1, 1)));
    assert!(dests.contains(&coord3(1, 2, 1)));
    assert!(dests.contains(&coord3(1, 1, 2)));

    assert!(!dests.contains(&coord3(2, 2, 1)));
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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

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
    let dimension = 5;
    let side = 3;
    let mut board = Board::new_empty(dimension, side);

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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

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
    let side = 5;
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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

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
```./tests/pawn_promotion_rule.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord_3d(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_promotion_conditions_3d_white() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    let start_pos = coord_3d(6, 0, 7);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(7, 0, 7) && m.promotion == Some(PieceType::Queen));

    assert!(promo_move.is_some(), "Should promote at (7, 0, 7)");
}

#[test]
fn test_no_promotion_partial_far_side_white() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    let start_pos = coord_3d(6, 0, 0);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let plain_move = moves
        .iter()
        .find(|m| m.to == coord_3d(7, 0, 0) && m.promotion.is_none());
    assert!(plain_move.is_some(), "Should be a normal move");

    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(7, 0, 0) && m.promotion == Some(PieceType::Queen));
    assert!(
        promo_move.is_none(),
        "Should NOT promote at (7, 0, 0) if Z is not max"
    );
}

#[test]
fn test_promotion_conditions_3d_black() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    let start_pos = coord_3d(1, 0, 0);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::Black);
    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(0, 0, 0) && m.promotion == Some(PieceType::Queen));

    assert!(promo_move.is_some(), "Black should promote at (0, 0, 0)");
}

#[test]
fn test_no_promotion_partial_black() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    let start_pos = coord_3d(1, 0, 7);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::Black);
    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(0, 0, 7) && m.promotion.is_some());

    assert!(
        promo_move.is_none(),
        "Black should NOT promote at (0, 0, 7)"
    );
}
```
```./tests/special_moves.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_en_passant() {
    let mut board = Board::new_empty(2, 8);

    board
        .set_piece(
            &coord(4, 4),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(6, 5),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let move_black = hyperchess::domain::models::Move {
        from: coord(6, 5),
        to: coord(4, 5),
        promotion: None,
    };
    board.apply_move(&move_black).unwrap();

    let ep_target_idx = board.coords_to_index(&[5, 5]).unwrap();
    let ep_victim_idx = board.coords_to_index(&[4, 5]).unwrap();
    assert_eq!(
        board.en_passant_target,
        Some((ep_target_idx, ep_victim_idx)),
        "EP Target/Victim tuple should be set"
    );

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let ep_move = moves.iter().find(|m| m.to == coord(5, 5));

    assert!(
        ep_move.is_some(),
        "En Passant capture move should be generated"
    );

    board.apply_move(ep_move.unwrap()).unwrap();

    let p = board.get_piece(&coord(5, 5));
    assert!(p.is_some());
    assert_eq!(p.unwrap().owner, Player::White);

    let captured = board.get_piece(&coord(4, 5));
    assert!(captured.is_none(), "Captured pawn should be removed");

    assert_eq!(board.en_passant_target, None);
}

#[test]
fn test_castling_kingside_white() {
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

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_some(),
        "White Kingside Castling should be available"
    );

    board.apply_move(castle_move.unwrap()).unwrap();

    let k = board.get_piece(&coord(0, 6));
    assert!(k.is_some());
    assert_eq!(k.unwrap().piece_type, PieceType::King);

    let r = board.get_piece(&coord(0, 5));
    assert!(r.is_some());
    assert_eq!(r.unwrap().piece_type, PieceType::Rook);

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

    board
        .set_piece(
            &coord(0, 5),
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
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

    board
        .set_piece(
            &coord(7, 5),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_none(),
        "Castling through check should be illegal"
    );
}
```
```./tests/super_pawn.rs
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord_3d(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_super_pawn_z_axis_movement() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    let start_pos = coord_3d(0, 0, 1);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let move_z = moves.iter().find(|m| m.to == coord_3d(0, 0, 2));
    let move_x = moves.iter().find(|m| m.to == coord_3d(1, 0, 1));
    let move_y = moves.iter().find(|m| m.to == coord_3d(0, 1, 1));

    assert!(move_z.is_some(), "Should allow Z-axis push");
    assert!(move_x.is_some(), "Should allow X-axis push");

    assert!(
        move_y.is_none(),
        "Should NOT allow Y-axis push (Lateral Forbidden)"
    );
}

#[test]
fn test_super_pawn_capture_multidimensional() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    let p1 = coord_3d(1, 1, 1);
    board
        .set_piece(
            &p1,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let target = coord_3d(2, 2, 1);
    board
        .set_piece(
            &target,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let capture = moves.iter().find(|m| m.to == target);

    assert!(
        capture.is_some(),
        "Should capture diagonally across dimensions"
    );
}
```
