use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::domain::board::{Board, UnmakeInfo};
use crate::domain::models::{Move, Player};
use crate::domain::rules::Rules;
use crate::infrastructure::ai::eval::Evaluator;
use crate::infrastructure::ai::see::SEE;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT, PackedMove};

pub const VAL_PAWN: i32 = 100;
pub const VAL_KNIGHT: i32 = 320;
pub const VAL_BISHOP: i32 = 330;
pub const VAL_ROOK: i32 = 500;
pub const VAL_QUEEN: i32 = 900;
pub const VAL_KING: i32 = 20000;

pub fn get_piece_value(board: &Board, idx: usize) -> i32 {
    if board.pieces.pawns.get_bit(idx) {
        VAL_PAWN
    } else if board.pieces.knights.get_bit(idx) {
        VAL_KNIGHT
    } else if board.pieces.bishops.get_bit(idx) {
        VAL_BISHOP
    } else if board.pieces.rooks.get_bit(idx) {
        VAL_ROOK
    } else if board.pieces.queens.get_bit(idx) {
        VAL_QUEEN
    } else if board.pieces.kings.get_bit(idx) {
        VAL_KING
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Iterative q_search
// ---------------------------------------------------------------------------
//
// Uses an explicit stack of apply/unmake pairs instead of call-stack recursion.
// The move list is stored inline in each frame to avoid separate heap allocs.

struct QFrame {
    alpha: i32,
    beta: i32,
    player: Player,
    hash: u64,
    moves: smallvec::SmallVec<[(Move, i32); 8]>,
    move_idx: usize,
    pending_unmake: Option<(Move, UnmakeInfo)>,
}

use smallvec::SmallVec;

pub fn q_search(
    board: &mut Board,
    alpha: i32,
    beta: i32,
    player: Player,
    nodes_searched: &Arc<AtomicUsize>,
    stop_flag: &Arc<AtomicBool>,
    tt: Option<&Arc<LockFreeTT>>,
) -> i32 {
    let mut stack: SmallVec<[QFrame; 16]> = SmallVec::new();
    let mut return_value: i32 = 0;

    // Push initial frame — initialization happens inline below
    stack.push(QFrame {
        alpha,
        beta,
        player,
        hash: 0,
        moves: SmallVec::new(),
        move_idx: usize::MAX, // sentinel: means "not yet initialized"
        pending_unmake: None,
    });

    'outer: loop {
        let depth = stack.len() - 1;

        // --- Handle child return ---
        if stack[depth].pending_unmake.is_some() {
            let (mv, info) = stack[depth].pending_unmake.take().unwrap();
            let score = -return_value;
            board.unmake_move(&mv, info);

            if score >= stack[depth].beta {
                if let Some(t) = tt {
                    t.store(stack[depth].hash, score, 0, Flag::LowerBound, None);
                }
                return_value = stack[depth].beta;
                stack.pop();
                if stack.is_empty() {
                    return return_value;
                }
                continue;
            }
            if score > stack[depth].alpha {
                stack[depth].alpha = score;
            }
            // Fall through to process next move
        }

        // --- Initialization (first entry) ---
        if stack[depth].move_idx == usize::MAX {
            if nodes_searched.fetch_add(1, Ordering::Relaxed) % 4096 == 0 {
                if stop_flag.load(Ordering::Relaxed) {
                    return_value = 0;
                    stack.pop();
                    if stack.is_empty() {
                        return return_value;
                    }
                    continue;
                }
            }

            let hash = board.state.hash;
            stack[depth].hash = hash;

            if let Some(t) = tt {
                if let Some((tt_score, _, tt_flag, _)) = t.get(hash) {
                    match tt_flag {
                        Flag::Exact => {
                            return_value = tt_score;
                            stack.pop();
                            if stack.is_empty() {
                                return return_value;
                            }
                            continue;
                        }
                        Flag::LowerBound => {
                            stack[depth].alpha = stack[depth].alpha.max(tt_score);
                        }
                        Flag::UpperBound => {}
                    }
                    if stack[depth].alpha >= stack[depth].beta {
                        return_value = tt_score;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }
                }
            }

            let score_val = Evaluator::evaluate(board);
            let stand_pat = if stack[depth].player == Player::Black {
                -score_val
            } else {
                score_val
            };

            if stand_pat >= stack[depth].beta {
                return_value = stack[depth].beta;
                stack.pop();
                if stack.is_empty() {
                    return return_value;
                }
                continue;
            }

            if stand_pat > stack[depth].alpha {
                stack[depth].alpha = stand_pat;
            }

            let loud = Rules::generate_loud_moves(board, stack[depth].player);
            let mut sorted: SmallVec<[(Move, i32); 8]> = loud
                .into_iter()
                .map(|m| {
                    let to_idx = board.coords_to_index(&m.to.values).unwrap_or(0);
                    let victim = get_piece_value(board, to_idx);
                    (m, victim)
                })
                .collect();
            sorted.sort_unstable_by(|a, b| b.1.cmp(&a.1));

            stack[depth].moves = sorted;
            stack[depth].move_idx = 0;
            // Fall through to process moves
        }

        // --- Process moves ---
        while stack[depth].move_idx < stack[depth].moves.len() {
            let idx = stack[depth].move_idx;
            let (ref mv, _) = stack[depth].moves[idx];
            let mv = mv.clone();
            stack[depth].move_idx += 1;

            let see_val = SEE::static_exchange_evaluation(board, &mv);
            if see_val < 0 {
                continue;
            }

            let info = match board.apply_move(&mv) {
                Ok(i) => i,
                Err(_) => continue,
            };

            let child_alpha = -stack[depth].beta;
            let child_beta = -stack[depth].alpha;
            let child_player = stack[depth].player.opponent();

            stack[depth].pending_unmake = Some((mv, info));

            // Push child
            stack.push(QFrame {
                alpha: child_alpha,
                beta: child_beta,
                player: child_player,
                hash: 0,
                moves: SmallVec::new(),
                move_idx: usize::MAX,
                pending_unmake: None,
            });
            continue 'outer;
        }

        // --- All moves done ---
        return_value = stack[depth].alpha;
        stack.pop();
        if stack.is_empty() {
            return return_value;
        }
    }
}

// ---------------------------------------------------------------------------
// Iterative minimax_shallow
// ---------------------------------------------------------------------------

struct ShallowFrame {
    depth: usize,
    alpha: i32,
    beta: i32,
    original_alpha: i32,
    player: Player,
    hash: u64,
    tt_move_coords: Option<(u16, u16)>,
    moves: Vec<(Move, i32)>,
    move_idx: usize,
    best_score: i32,
    best_move_obj: Option<Move>,
    pending_unmake: Option<(Move, UnmakeInfo)>,
}

pub fn minimax_shallow(
    board: &mut Board,
    depth: usize,
    alpha: i32,
    beta: i32,
    player: Player,
    nodes_searched: &Arc<AtomicUsize>,
    stop_flag: &Arc<AtomicBool>,
    tt: Option<&Arc<LockFreeTT>>,
) -> i32 {
    let mut stack: SmallVec<[ShallowFrame; 8]> = SmallVec::new();
    let mut return_value: i32 = 0;

    stack.push(ShallowFrame {
        depth,
        alpha,
        beta,
        original_alpha: alpha,
        player,
        hash: 0,
        tt_move_coords: None,
        moves: Vec::new(),
        move_idx: usize::MAX, // sentinel: not yet initialized
        best_score: -i32::MAX,
        best_move_obj: None,
        pending_unmake: None,
    });

    'outer: loop {
        let d = stack.len() - 1;

        // --- Handle child return ---
        if stack[d].pending_unmake.is_some() {
            let (mv, info) = stack[d].pending_unmake.take().unwrap();
            let score = -return_value;
            board.unmake_move(&mv, info);

            if score > stack[d].best_score {
                stack[d].best_score = score;
                stack[d].best_move_obj = Some(mv);
            }
            if score > stack[d].alpha {
                stack[d].alpha = score;
            }
            if stack[d].alpha >= stack[d].beta {
                store_shallow_tt(board, &stack[d], tt);
                return_value = stack[d].best_score;
                stack.pop();
                if stack.is_empty() {
                    return return_value;
                }
                continue;
            }
            // Fall through to process next move
        }

        // --- Initialization ---
        if stack[d].move_idx == usize::MAX {
            if stop_flag.load(Ordering::Relaxed) {
                return_value = 0;
                stack.pop();
                if stack.is_empty() {
                    return return_value;
                }
                continue;
            }

            let hash = board.state.hash;
            stack[d].hash = hash;

            if let Some(t) = tt {
                if let Some((tt_score, tt_depth, tt_flag, best_m)) = t.get(hash) {
                    if let Some(pm) = best_m {
                        stack[d].tt_move_coords = Some((pm.from_idx, pm.to_idx));
                    }
                    if tt_depth as usize >= stack[d].depth {
                        match tt_flag {
                            Flag::Exact => {
                                return_value = tt_score;
                                stack.pop();
                                if stack.is_empty() {
                                    return return_value;
                                }
                                continue;
                            }
                            Flag::LowerBound => stack[d].alpha = stack[d].alpha.max(tt_score),
                            Flag::UpperBound => stack[d].beta = stack[d].beta.min(tt_score),
                        }
                        if stack[d].alpha >= stack[d].beta {
                            return_value = tt_score;
                            stack.pop();
                            if stack.is_empty() {
                                return return_value;
                            }
                            continue;
                        }
                    }
                }
            }

            stack[d].original_alpha = stack[d].alpha;

            if stack[d].depth == 0 {
                return_value = q_search(
                    board,
                    stack[d].alpha,
                    stack[d].beta,
                    stack[d].player,
                    nodes_searched,
                    stop_flag,
                    tt,
                );
                stack.pop();
                if stack.is_empty() {
                    return return_value;
                }
                continue;
            }

            let moves = Rules::generate_legal_moves(board, stack[d].player);
            if moves.is_empty() {
                if let Some(king_pos) = board.get_king_coordinate(stack[d].player) {
                    if Rules::is_square_attacked(board, &king_pos, stack[d].player.opponent()) {
                        return_value = -30000 + (100 - stack[d].depth as i32);
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }
                }
                return_value = 0;
                stack.pop();
                if stack.is_empty() {
                    return return_value;
                }
                continue;
            }

            let tt_mc = stack[d].tt_move_coords;
            let mut sorted: Vec<(Move, i32)> = moves
                .into_iter()
                .map(|m| {
                    let from_idx = board.coords_to_index(&m.from.values).unwrap_or(0);
                    let to_idx = board.coords_to_index(&m.to.values).unwrap_or(0);
                    let victim = get_piece_value(board, to_idx);
                    let promo_bonus = if m.promotion.is_some() { 500 } else { 0 };
                    let tt_bonus = if let Some((t_from, t_to)) = tt_mc {
                        if from_idx as u16 == t_from && to_idx as u16 == t_to {
                            200000
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    (m, victim + promo_bonus + tt_bonus)
                })
                .collect();
            sorted.sort_unstable_by(|a, b| b.1.cmp(&a.1));

            stack[d].moves = sorted;
            stack[d].move_idx = 0;
            // Fall through to process moves
        }

        // --- Process moves ---
        while stack[d].move_idx < stack[d].moves.len() {
            let idx = stack[d].move_idx;
            let (ref mv, _) = stack[d].moves[idx];
            let mv = mv.clone();
            stack[d].move_idx += 1;

            let info = match board.apply_move(&mv) {
                Ok(i) => i,
                Err(_) => continue,
            };

            let child_depth = stack[d].depth - 1;
            let child_alpha = -stack[d].beta;
            let child_beta = -stack[d].alpha;
            let child_player = stack[d].player.opponent();

            stack[d].pending_unmake = Some((mv, info));

            if child_depth == 0 {
                // Leaf: call q_search directly
                return_value = q_search(
                    board,
                    child_alpha,
                    child_beta,
                    child_player,
                    nodes_searched,
                    stop_flag,
                    tt,
                );
                continue 'outer; // handled by child-return at top
            }

            stack.push(ShallowFrame {
                depth: child_depth,
                alpha: child_alpha,
                beta: child_beta,
                original_alpha: child_alpha,
                player: child_player,
                hash: 0,
                tt_move_coords: None,
                moves: Vec::new(),
                move_idx: usize::MAX,
                best_score: -i32::MAX,
                best_move_obj: None,
                pending_unmake: None,
            });
            continue 'outer;
        }

        // --- All moves exhausted ---
        store_shallow_tt(board, &stack[d], tt);
        return_value = stack[d].best_score;
        stack.pop();
        if stack.is_empty() {
            return return_value;
        }
    }
}

fn store_shallow_tt(board: &Board, frame: &ShallowFrame, tt: Option<&Arc<LockFreeTT>>) {
    if let Some(t) = tt {
        let flag = if frame.best_score <= frame.original_alpha {
            Flag::UpperBound
        } else if frame.best_score >= frame.beta {
            Flag::LowerBound
        } else {
            Flag::Exact
        };

        let packed = frame.best_move_obj.as_ref().and_then(|m| {
            let from = board.coords_to_index(&m.from.values)?;
            let to = board.coords_to_index(&m.to.values)?;
            let promo = if let Some(p) = m.promotion {
                match p {
                    crate::domain::models::PieceType::Queen => 1,
                    crate::domain::models::PieceType::Rook => 2,
                    crate::domain::models::PieceType::Bishop => 3,
                    crate::domain::models::PieceType::Knight => 4,
                    _ => 0,
                }
            } else {
                0
            };
            Some(PackedMove {
                from_idx: from as u16,
                to_idx: to as u16,
                promotion: promo,
            })
        });

        t.store(
            frame.hash,
            frame.best_score,
            frame.depth as u8,
            flag,
            packed,
        );
    }
}
