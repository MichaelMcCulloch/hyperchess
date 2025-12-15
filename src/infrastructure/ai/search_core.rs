use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::domain::rules::Rules;
use crate::infrastructure::ai::eval::Evaluator;
use crate::infrastructure::ai::see::SEE;

pub const VAL_PAWN: i32 = 100;
pub const VAL_KNIGHT: i32 = 320;
pub const VAL_BISHOP: i32 = 330;
pub const VAL_ROOK: i32 = 500;
pub const VAL_QUEEN: i32 = 900;
pub const VAL_KING: i32 = 20000;

pub fn get_piece_value(board: &Board, idx: usize) -> i32 {
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

pub fn q_search(
    board: &mut Board,
    mut alpha: i32,
    beta: i32,
    player: Player,
    nodes_searched: &Arc<AtomicUsize>,
    stop_flag: &Arc<AtomicBool>,
) -> i32 {
    if nodes_searched.fetch_add(1, Ordering::Relaxed) % 2048 == 0 {
        if stop_flag.load(Ordering::Relaxed) {
            return 0;
        }
    }

    let score_val = Evaluator::evaluate(board);
    let stand_pat = if player == Player::Black {
        -score_val
    } else {
        score_val
    };

    if stand_pat >= beta {
        return beta;
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let moves = Rules::generate_loud_moves(board, player);

    let mut sorted_moves: Vec<(Move, i32)> = moves
        .into_iter()
        .map(|m| {
            let to_idx = board.coords_to_index(&m.to.values).unwrap_or(0);
            let victim = get_piece_value(board, to_idx);
            (m, victim)
        })
        .collect();
    sorted_moves.sort_by(|a, b| b.1.cmp(&a.1));

    for (mv, _) in sorted_moves {
        let see_val = SEE::static_exchange_evaluation(board, &mv);
        if see_val < 0 {
            continue;
        }

        let info = match board.apply_move(&mv) {
            Ok(i) => i,
            Err(_) => continue,
        };

        let score = -q_search(
            board,
            -beta,
            -alpha,
            player.opponent(),
            nodes_searched,
            stop_flag,
        );

        board.unmake_move(&mv, info);

        if score >= beta {
            return beta;
        }
        if score > alpha {
            alpha = score;
        }
    }
    alpha
}
