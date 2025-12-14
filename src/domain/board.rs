use crate::domain::coordinate::Coordinate;
use crate::domain::models::{GameResult, Move, Piece, PieceType, Player};
use crate::domain::zobrist::ZobristKeys;
use smallvec::{SmallVec, smallvec};
use std::collections::HashMap;
use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not, Shl, ShlAssign, Shr, ShrAssign};
use std::sync::Arc;

#[derive(Debug)]
pub struct BoardCache {
    pub index_to_coords: Vec<SmallVec<[usize; 4]>>,
    pub validity_masks: HashMap<(Vec<isize>, usize), BitBoard>,

    pub knight_offsets: Vec<Vec<isize>>,
    pub king_offsets: Vec<Vec<isize>>,
    pub rook_directions: Vec<Vec<isize>>,
    pub bishop_directions: Vec<Vec<isize>>,

    pub white_pawn_capture_offsets: Vec<Vec<isize>>,
    pub black_pawn_capture_offsets: Vec<Vec<isize>>,
}

impl BoardCache {
    pub fn new(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let mut index_to_coords = Vec::with_capacity(total_cells);

        for i in 0..total_cells {
            let mut coords = SmallVec::with_capacity(dimension);
            coords.resize(dimension, 0);
            let mut temp = i;
            for d in 0..dimension {
                coords[d] = temp % side;
                temp /= side;
            }
            index_to_coords.push(coords);
        }

        let mut validity_masks = HashMap::new();

        let rook_directions = crate::domain::rules::Rules::get_rook_directions_calc(dimension);
        let bishop_directions = crate::domain::rules::Rules::get_bishop_directions_calc(dimension);
        let knight_offsets = crate::domain::rules::Rules::get_knight_offsets_calc(dimension);
        let king_offsets = crate::domain::rules::Rules::get_king_offsets_calc(dimension);

        let white_pawn_capture_offsets =
            crate::domain::rules::Rules::get_pawn_capture_offsets_calc(dimension, Player::White);
        let black_pawn_capture_offsets =
            crate::domain::rules::Rules::get_pawn_capture_offsets_calc(dimension, Player::Black);

        let all_dirs = rook_directions.iter().chain(bishop_directions.iter());

        for dir in all_dirs {
            let mut step = 1;
            while step < side {
                let mut mask_bb = BitBoard::new_empty(dimension, side);

                for i in 0..total_cells {
                    let coords = &index_to_coords[i];
                    let mut valid = true;
                    for (c, &d) in coords.iter().zip(dir.iter()) {
                        let res = *c as isize + (d * step as isize);
                        if res < 0 || res >= side as isize {
                            valid = false;
                            break;
                        }
                    }
                    if valid {
                        mask_bb.set_bit(i);
                    }
                }

                validity_masks.insert((dir.clone(), step), mask_bb);
                step *= 2;
            }
        }

        Self {
            index_to_coords,
            validity_masks,
            knight_offsets,
            king_offsets,
            rook_directions,
            bishop_directions,
            white_pawn_capture_offsets,
            black_pawn_capture_offsets,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BitBoard {
    Small(u32),
    Medium(u128),
    Large { data: SmallVec<[u64; 8]> },
}

impl BitBoard {
    pub fn copy_from(&mut self, other: &Self) {
        match (self, other) {
            (BitBoard::Small(a), BitBoard::Small(b)) => *a = *b,
            (BitBoard::Medium(a), BitBoard::Medium(b)) => *a = *b,
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                if a.len() != b.len() {
                    a.resize(b.len(), 0);
                }
                a.copy_from_slice(b);
            }

            (this, that) => *this = that.clone(),
        }
    }
}

impl BitAndAssign<&BitBoard> for BitBoard {
    fn bitand_assign(&mut self, rhs: &BitBoard) {
        match (&mut *self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => {
                *a &= b;
            }
            (BitBoard::Medium(a), BitBoard::Medium(b)) => {
                *a &= b;
            }
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::min(a.len(), b.len());
                for (l, r) in a.iter_mut().zip(b.iter()).take(len) {
                    *l &= *r;
                }

                if a.len() > len {
                    for l in a.iter_mut().skip(len) {
                        *l = 0;
                    }
                }
            }
            _ => {
                *self = &*self & rhs;
            }
        }
    }
}

impl BitOrAssign<&BitBoard> for BitBoard {
    fn bitor_assign(&mut self, rhs: &BitBoard) {
        match (&mut *self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => {
                *a |= b;
            }
            (BitBoard::Medium(a), BitBoard::Medium(b)) => {
                *a |= b;
            }
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::min(a.len(), b.len());
                for (l, r) in a.iter_mut().zip(b.iter()).take(len) {
                    *l |= *r;
                }

                if b.len() > a.len() {
                    a.extend_from_slice(&b[len..]);
                }
            }
            _ => {
                *self = &*self | rhs;
            }
        }
    }
}

impl ShlAssign<usize> for BitBoard {
    fn shl_assign(&mut self, shift: usize) {
        if shift == 0 {
            return;
        }
        match self {
            BitBoard::Small(b) => *b = b.wrapping_shl(shift as u32),
            BitBoard::Medium(b) => *b = b.wrapping_shl(shift as u32),
            BitBoard::Large { data } => {
                let chunks_shift = shift / 64;
                let bits_shift = shift % 64;

                if chunks_shift > 0 {
                    if chunks_shift >= data.len() {
                        for x in data.iter_mut() {
                            *x = 0;
                        }
                    } else {
                        for i in (chunks_shift..data.len()).rev() {
                            data[i] = data[i - chunks_shift];
                        }

                        for i in 0..chunks_shift {
                            data[i] = 0;
                        }
                    }
                }

                if bits_shift > 0 {
                    let inv_shift = 64 - bits_shift;
                    for i in (0..data.len()).rev() {
                        let prev = if i > 0 { data[i - 1] } else { 0 };
                        data[i] = (data[i] << bits_shift) | (prev >> inv_shift);
                    }
                }
            }
        }
    }
}

impl ShrAssign<usize> for BitBoard {
    fn shr_assign(&mut self, shift: usize) {
        if shift == 0 {
            return;
        }
        match self {
            BitBoard::Small(b) => *b = b.wrapping_shr(shift as u32),
            BitBoard::Medium(b) => *b = b.wrapping_shr(shift as u32),
            BitBoard::Large { data } => {
                let chunks_shift = shift / 64;
                let bits_shift = shift % 64;

                if chunks_shift > 0 {
                    if chunks_shift >= data.len() {
                        for x in data.iter_mut() {
                            *x = 0;
                        }
                    } else {
                        for i in 0..(data.len() - chunks_shift) {
                            data[i] = data[i + chunks_shift];
                        }

                        for i in (data.len() - chunks_shift)..data.len() {
                            data[i] = 0;
                        }
                    }
                }

                if bits_shift > 0 {
                    let inv_shift = 64 - bits_shift;
                    for i in 0..data.len() {
                        let next = if i + 1 < data.len() { data[i + 1] } else { 0 };
                        data[i] = (data[i] >> bits_shift) | (next << inv_shift);
                    }
                }
            }
        }
    }
}

impl<'a, 'b> BitAnd<&'b BitBoard> for &'a BitBoard {
    type Output = BitBoard;

    fn bitand(self, rhs: &'b BitBoard) -> BitBoard {
        match (self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => BitBoard::Small(a & b),
            (BitBoard::Medium(a), BitBoard::Medium(b)) => BitBoard::Medium(a & b),
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::max(a.len(), b.len());
                let mut new_data = SmallVec::with_capacity(len);

                for i in 0..len {
                    let val_a = a.get(i).copied().unwrap_or(0);
                    let val_b = b.get(i).copied().unwrap_or(0);
                    new_data.push(val_a & val_b);
                }
                BitBoard::Large { data: new_data }
            }
            _ => panic!("Mismatched BitBoard types in BitAnd"),
        }
    }
}

impl<'a, 'b> BitOr<&'b BitBoard> for &'a BitBoard {
    type Output = BitBoard;

    fn bitor(self, rhs: &'b BitBoard) -> BitBoard {
        match (self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => BitBoard::Small(a | b),
            (BitBoard::Medium(a), BitBoard::Medium(b)) => BitBoard::Medium(a | b),
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::max(a.len(), b.len());

                let mut new_data = SmallVec::with_capacity(len);
                for i in 0..len {
                    let val_a = a.get(i).copied().unwrap_or(0);
                    let val_b = b.get(i).copied().unwrap_or(0);
                    new_data.push(val_a | val_b);
                }
                BitBoard::Large { data: new_data }
            }
            _ => panic!("Mismatched BitBoard types in BitOr"),
        }
    }
}

impl<'a> Not for &'a BitBoard {
    type Output = BitBoard;

    fn not(self) -> BitBoard {
        match self {
            BitBoard::Small(a) => BitBoard::Small(!a),
            BitBoard::Medium(a) => BitBoard::Medium(!a),
            BitBoard::Large { data } => {
                let mut new_data = SmallVec::with_capacity(data.len());
                for x in data {
                    new_data.push(!x);
                }
                BitBoard::Large { data: new_data }
            }
        }
    }
}

impl<'a> Shl<usize> for &'a BitBoard {
    type Output = BitBoard;
    fn shl(self, shift: usize) -> BitBoard {
        let mut res = self.clone();
        res <<= shift;
        res
    }
}

impl<'a> Shr<usize> for &'a BitBoard {
    type Output = BitBoard;
    fn shr(self, shift: usize) -> BitBoard {
        let mut res = self.clone();
        res >>= shift;
        res
    }
}

impl BitAnd for BitBoard {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        &self & &rhs
    }
}
impl BitOr for BitBoard {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        &self | &rhs
    }
}
impl Not for BitBoard {
    type Output = Self;
    fn not(self) -> Self {
        !&self
    }
}
impl Shl<usize> for BitBoard {
    type Output = Self;
    fn shl(self, rhs: usize) -> Self {
        &self << rhs
    }
}
impl Shr<usize> for BitBoard {
    type Output = Self;
    fn shr(self, rhs: usize) -> Self {
        &self >> rhs
    }
}

impl BitBoard {
    pub fn zero_like(&self) -> Self {
        match self {
            BitBoard::Small(_) => BitBoard::Small(0),
            BitBoard::Medium(_) => BitBoard::Medium(0),
            BitBoard::Large { data } => BitBoard::Large {
                data: smallvec![0u64; data.len()],
            },
        }
    }
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
    pub cache: Arc<BoardCache>,
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
        let cache = Arc::new(BoardCache::new(dimension, side));
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
            cache,
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

    fn hash_xor_piece(&mut self, index: usize, piece: Piece) {
        let offset = match (piece.owner, piece.piece_type) {
            (Player::White, PieceType::Pawn) => 0,
            (Player::White, PieceType::Knight) => 1,
            (Player::White, PieceType::Bishop) => 2,
            (Player::White, PieceType::Rook) => 3,
            (Player::White, PieceType::Queen) => 4,
            (Player::White, PieceType::King) => 5,
            (Player::Black, PieceType::Pawn) => 6,
            (Player::Black, PieceType::Knight) => 7,
            (Player::Black, PieceType::Bishop) => 8,
            (Player::Black, PieceType::Rook) => 9,
            (Player::Black, PieceType::Queen) => 10,
            (Player::Black, PieceType::King) => 11,
        };
        self.hash ^= self.zobrist.piece_keys[offset * self.total_cells + index];
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
        self.cache.index_to_coords[index].clone()
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

            for d in 2..self.dimension {
                white_coords[d] = 1;
            }

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

            for d in 2..self.dimension {
                white_coords[d] = 0;
            }
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

                for d in 2..self.dimension {
                    black_coords[d] = self.side - 2;
                }

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

            for d in 2..self.dimension {
                black_coords[d] = self.side - 1;
            }

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
        self.get_piece_at_index(index)
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

        self.history.push(self.hash);

        self.hash ^= self.zobrist.black_to_move;

        if self.castling_rights > 0 {
            self.hash ^= self.zobrist.castling_keys[self.castling_rights as usize];
        }

        if let Some((ep, _)) = self.en_passant_target {
            if ep < self.zobrist.en_passant_keys.len() {
                self.hash ^= self.zobrist.en_passant_keys[ep];
            }
        }

        self.hash_xor_piece(from_idx, moving_piece);

        if let Some(target_p) = self.get_piece_at_index(to_idx) {
            captured = Some((to_idx, target_p));
            self.hash_xor_piece(to_idx, target_p);
        }

        if moving_piece.piece_type == PieceType::Pawn {
            if let Some((target, victim)) = self.en_passant_target {
                if to_idx == target {
                    if let Some(victim_p) = self.get_piece_at_index(victim) {
                        captured = Some((victim, victim_p));
                        self.hash_xor_piece(victim, victim_p);
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
                        if target_idx < self.zobrist.en_passant_keys.len() {
                            self.hash ^= self.zobrist.en_passant_keys[target_idx];
                        }
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

        self.hash_xor_piece(to_idx, piece_to_place);

        if let Some((r_from, r_to, r_piece)) = castling_rook_move {
            self.hash_xor_piece(r_from, r_piece);
            self.remove_piece_at_index(r_from);
            self.hash_xor_piece(r_to, r_piece);
            self.place_piece_at_index(r_to, r_piece);
        }

        if self.castling_rights > 0 {
            self.hash ^= self.zobrist.castling_keys[self.castling_rights as usize];
        }

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

    pub fn make_null_move(&mut self) -> UnmakeInfo {
        let saved_ep = self.en_passant_target;
        let saved_castling = self.castling_rights;

        self.history.push(self.hash);
        self.en_passant_target = None;

        self.hash ^= self.zobrist.black_to_move;

        if let Some((ep, _)) = saved_ep {
            if ep < self.zobrist.en_passant_keys.len() {
                self.hash ^= self.zobrist.en_passant_keys[ep];
            }
        }

        UnmakeInfo {
            captured: None,
            en_passant_target: saved_ep,
            castling_rights: saved_castling,
        }
    }

    pub fn unmake_null_move(&mut self, info: UnmakeInfo) {
        if let Some(h) = self.history.pop() {
            self.hash = h;
        }
        self.en_passant_target = info.en_passant_target;
        self.castling_rights = info.castling_rights;
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
        }
    }

    pub fn get_smallest_attacker(
        &self,
        target_sq: &Coordinate,
        attacker: Player,
    ) -> Option<(i32, usize)> {
        let occupancy = match attacker {
            Player::White => &self.white_occupancy,
            Player::Black => &self.black_occupancy,
        };

        let pawn_attacker_offsets = match attacker.opponent() {
            Player::White => &self.cache.white_pawn_capture_offsets,
            Player::Black => &self.cache.black_pawn_capture_offsets,
        };

        for offset in pawn_attacker_offsets {
            if let Some(src) =
                crate::domain::rules::Rules::apply_offset(&target_sq.values, &offset, self.side)
            {
                if let Some(idx) = self.coords_to_index(&src) {
                    if occupancy.get_bit(idx) && self.pawns.get_bit(idx) {
                        return Some((100, idx));
                    }
                }
            }
        }

        for offset in &self.cache.knight_offsets {
            if let Some(src) =
                crate::domain::rules::Rules::apply_offset(&target_sq.values, &offset, self.side)
            {
                if let Some(idx) = self.coords_to_index(&src) {
                    if occupancy.get_bit(idx) && self.knights.get_bit(idx) {
                        return Some((320, idx));
                    }
                }
            }
        }

        for dir in &self.cache.bishop_directions {
            if crate::domain::rules::Rules::scan_ray_for_threat(
                self,
                &target_sq.values,
                dir,
                attacker,
                &[PieceType::Bishop],
            ) {
                if let Some(idx) =
                    self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Bishop)
                {
                    return Some((330, idx));
                }
            }
        }

        for dir in &self.cache.rook_directions {
            if crate::domain::rules::Rules::scan_ray_for_threat(
                self,
                &target_sq.values,
                dir,
                attacker,
                &[PieceType::Rook],
            ) {
                if let Some(idx) =
                    self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Rook)
                {
                    return Some((500, idx));
                }
            }
        }

        for dir in &self.cache.bishop_directions {
            if let Some(idx) = self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Queen)
            {
                return Some((900, idx));
            }
        }
        for dir in &self.cache.rook_directions {
            if let Some(idx) = self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Queen)
            {
                return Some((900, idx));
            }
        }

        for offset in &self.cache.king_offsets {
            if let Some(src) =
                crate::domain::rules::Rules::apply_offset(&target_sq.values, &offset, self.side)
            {
                if let Some(idx) = self.coords_to_index(&src) {
                    if occupancy.get_bit(idx) && self.kings.get_bit(idx) {
                        return Some((20000, idx));
                    }
                }
            }
        }

        None
    }

    fn trace_ray_for_piece(
        &self,
        origin_coord: &Coordinate,
        dir: &[isize],
        owner: Player,
        pt: PieceType,
    ) -> Option<usize> {
        let mut current = origin_coord.values.clone();
        let occupancy = &self.white_occupancy | &self.black_occupancy;
        let my_occ = match owner {
            Player::White => &self.white_occupancy,
            Player::Black => &self.black_occupancy,
        };

        loop {
            if let Some(next) = crate::domain::rules::Rules::apply_offset(&current, dir, self.side)
            {
                if let Some(idx) = self.coords_to_index(&next) {
                    if occupancy.get_bit(idx) {
                        if my_occ.get_bit(idx) {
                            let is_type = match pt {
                                PieceType::Bishop => self.bishops.get_bit(idx),
                                PieceType::Rook => self.rooks.get_bit(idx),
                                PieceType::Queen => self.queens.get_bit(idx),
                                _ => false,
                            };
                            if is_type {
                                return Some(idx);
                            }
                        }
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
        None
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
            BitBoard::Large {
                data: smallvec![0u64; len],
            }
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

    pub fn or_with(mut self, other: &Self) -> Self {
        self |= other;
        self
    }

    pub fn iter_indices(&self) -> BitIterator<'_> {
        BitIterator::new(self)
    }
}
