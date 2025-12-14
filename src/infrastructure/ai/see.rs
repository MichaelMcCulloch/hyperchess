use crate::domain::board::Board;
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType};

pub struct SEE;

impl SEE {
    pub fn static_exchange_evaluation(board: &Board, mv: &Move) -> i32 {
        let to_idx = match board.coords_to_index(&mv.to.values) {
            Some(i) => i,
            None => return 0,
        };

        let value;

        if let Some(target) = board.get_piece_at_index(to_idx) {
            value = Self::get_val(target.piece_type);
        } else if board
            .en_passant_target
            .map(|(t, _)| t == to_idx)
            .unwrap_or(false)
        {
            value = 100;
        } else {
            return 0;
        }

        let mut board_clone = board.clone();
        let mut gain = Vec::new();
        gain.push(value);

        let attacker_sq = mv.from.clone();
        let attacker_piece = board
            .get_piece_at_index(board.coords_to_index(&mv.from.values).unwrap())
            .unwrap();

        let mut side_to_move = attacker_piece.owner.opponent();
        let target_sq = mv.to.clone();

        let mut attacking_piece_val = Self::get_val(attacker_piece.piece_type);

        board_clone.clear_cell(&attacker_sq);

        loop {
            if let Some((val, from_idx)) =
                board_clone.get_smallest_attacker(&target_sq, side_to_move)
            {
                let captured_val = attacking_piece_val;

                let last_gain = *gain.last().unwrap();
                gain.push(captured_val - last_gain);

                attacking_piece_val = val;
                side_to_move = side_to_move.opponent();

                let coords = board_clone.index_to_coords(from_idx);
                board_clone.clear_cell(&Coordinate::new(coords.to_vec()));

                if val >= 20000 {
                    break;
                }
            } else {
                break;
            }
        }

        while gain.len() > 1 {
            let last = gain.pop().unwrap();
            let prev = gain.last_mut().unwrap();
            if last > -(*prev) {
                *prev = -last;
            }
        }

        gain[0]
    }

    fn get_val(pt: PieceType) -> i32 {
        match pt {
            PieceType::Pawn => 100,
            PieceType::Knight => 320,
            PieceType::Bishop => 330,
            PieceType::Rook => 500,
            PieceType::Queen => 900,
            PieceType::King => 20000,
        }
    }
}
