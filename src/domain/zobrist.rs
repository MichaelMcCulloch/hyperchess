use crate::domain::board::{BoardRepresentation, GenericBoard};
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

    pub fn get_hash<R: BoardRepresentation>(
        &self,
        board: &GenericBoard<R>,
        current_player: Player,
    ) -> u64 {
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
