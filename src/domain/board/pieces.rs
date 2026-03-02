use crate::domain::board::BitBoardLarge;
use crate::domain::board::board_representation::BoardRepresentation;
use crate::domain::models::{Piece, PieceType, Player};

/// Hot-path mutable piece placement data. Cloned per-thread during parallel search
/// and mutated millions of times. Kept minimal for cache locality.
#[derive(Clone, Debug)]
pub struct PieceMap<R: BoardRepresentation> {
    pub white_occupancy: R,
    pub black_occupancy: R,
    /// Combined `white_occupancy | black_occupancy`, maintained incrementally.
    pub all_occupancy: R,
    pub pawns: R,
    pub rooks: R,
    pub knights: R,
    pub bishops: R,
    pub queens: R,
    pub kings: R,
    /// Cached king cell indices (avoids bitboard scan). u16::MAX = absent.
    pub white_king_idx: u16,
    pub black_king_idx: u16,
}

pub type Pieces = PieceMap<BitBoardLarge>;

impl<R: BoardRepresentation> PieceMap<R> {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let empty = R::new_empty(dimension, side);
        Self {
            white_occupancy: empty.clone(),
            black_occupancy: empty.clone(),
            all_occupancy: empty.clone(),
            pawns: empty.clone(),
            rooks: empty.clone(),
            knights: empty.clone(),
            bishops: empty.clone(),
            queens: empty.clone(),
            kings: empty,
            white_king_idx: u16::MAX,
            black_king_idx: u16::MAX,
        }
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

    pub fn place_piece_at_index(&mut self, index: usize, piece: Piece) {
        match piece.owner {
            Player::White => self.white_occupancy.set_bit(index),
            Player::Black => self.black_occupancy.set_bit(index),
        }
        self.all_occupancy.set_bit(index);

        match piece.piece_type {
            PieceType::Pawn => self.pawns.set_bit(index),
            PieceType::Rook => self.rooks.set_bit(index),
            PieceType::Knight => self.knights.set_bit(index),
            PieceType::Bishop => self.bishops.set_bit(index),
            PieceType::Queen => self.queens.set_bit(index),
            PieceType::King => {
                self.kings.set_bit(index);
                match piece.owner {
                    Player::White => self.white_king_idx = index as u16,
                    Player::Black => self.black_king_idx = index as u16,
                }
            }
        }
    }

    pub fn remove_piece_at_index(&mut self, index: usize) {
        if self.white_king_idx == index as u16 {
            self.white_king_idx = u16::MAX;
        } else if self.black_king_idx == index as u16 {
            self.black_king_idx = u16::MAX;
        }
        self.white_occupancy.clear_bit(index);
        self.black_occupancy.clear_bit(index);
        self.all_occupancy.clear_bit(index);
        self.pawns.clear_bit(index);
        self.rooks.clear_bit(index);
        self.knights.clear_bit(index);
        self.bishops.clear_bit(index);
        self.queens.clear_bit(index);
        self.kings.clear_bit(index);
    }

    /// O(1) king index lookup. Returns None if no king present.
    #[inline]
    pub fn king_index(&self, player: Player) -> Option<usize> {
        let idx = match player {
            Player::White => self.white_king_idx,
            Player::Black => self.black_king_idx,
        };
        if idx == u16::MAX {
            None
        } else {
            Some(idx as usize)
        }
    }
}
