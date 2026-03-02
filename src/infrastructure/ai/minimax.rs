use super::eval::Evaluator;
use crate::config::AppConfig;
use crate::domain::board::{Board, UnmakeInfo};
use crate::domain::models::{Move, PieceType, Player};
use crate::domain::rules::{MoveList, Rules};
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT, PackedMove};
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::search_core::{VAL_BISHOP, VAL_KNIGHT, VAL_QUEEN, VAL_ROOK};

const CHECKMATE_SCORE: i32 = 30000;
const TIMEOUT_CHECK_INTERVAL: usize = 2048;

const MAX_HISTORY: i32 = 2000;

/// Razor margins by depth (indexed by depth: 1, 2, 3).
/// If static_eval + margin < alpha, drop to qsearch.
const RAZOR_MARGIN: [i32; 4] = [0, 300, 500, 700];

/// Multi-cut: at non-PV cut nodes, try the first MC_M moves at reduced depth.
/// If MC_C of them beat beta, prune the entire node.
const MC_M: usize = 6; // number of moves to try
const MC_C: usize = 3; // cutoff threshold
const MC_DEPTH_MIN: usize = 5; // minimum depth to apply multi-cut
const MC_REDUCTION: usize = 4; // depth reduction for verification

pub struct MinimaxBot {
    depth: usize,
    time_limit: Duration,
    tt: Arc<LockFreeTT>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: Arc<AtomicUsize>,
    num_threads: usize,
}

impl MinimaxBot {
    pub fn new(config: &AppConfig, _dimension: usize, _side: usize) -> Self {
        Self {
            depth: config.minimax.depth,
            time_limit: Duration::from_secs(config.compute.minutes * 60),
            tt: Arc::new(LockFreeTT::new(config.compute.memory)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: Arc::new(AtomicUsize::new(0)),
            num_threads: config.compute.concurrency.max(1),
        }
    }

    fn evaluate(&self, board: &Board, player_at_leaf: Option<Player>) -> i32 {
        let score = Evaluator::evaluate(board);

        if let Some(p) = player_at_leaf
            && p == Player::Black
        {
            return -score;
        }
        score
    }

    fn get_piece_value(&self, board: &Board, idx: usize) -> i32 {
        super::search_core::get_piece_value(board, idx)
    }

    fn q_search(&self, board: &mut Board, alpha: i32, beta: i32, player: Player) -> i32 {
        super::search_core::q_search(
            board,
            alpha,
            beta,
            player,
            &self.nodes_searched,
            &self.stop_flag,
            Some(&self.tt),
        )
    }

    fn sort_moves(
        &self,
        board: &Board,
        moves: &mut [Move],
        tt_move: Option<PackedMove>,
        killers: Option<&[Option<Move>; 2]>,
        history: &[Vec<i32>],
        countermove: Option<&Move>,
        cont_history: &[Vec<i32>],
        prev_move_idx: Option<usize>,
        player: Player,
    ) {
        moves.sort_by_cached_key(|mv| {
            let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(0);
            let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);

            if let Some(tm) = tt_move
                && tm.from_idx as usize == from_idx
                && tm.to_idx as usize == to_idx
            {
                return -2_000_000_000;
            }

            let enemy_occupancy = match player {
                Player::White => &board.pieces.black_occupancy,
                Player::Black => &board.pieces.white_occupancy,
            };

            if enemy_occupancy.get_bit(to_idx) {
                let victim_val = self.get_piece_value(board, to_idx);
                let attacker_val = self.get_piece_value(board, from_idx);

                return -(1_000_000 + 10 * victim_val - attacker_val);
            }

            if let Some(p) = mv.promotion {
                let val = match p {
                    PieceType::Queen => VAL_QUEEN,
                    PieceType::Rook => VAL_ROOK,
                    PieceType::Bishop => VAL_BISHOP,
                    PieceType::Knight => VAL_KNIGHT,
                    _ => 0,
                };
                return -(800_000 + val);
            }

            if let Some(ks) = killers {
                if let Some(k) = &ks[0]
                    && k == mv
                {
                    return -500_000;
                }
                if let Some(k) = &ks[1]
                    && k == mv
                {
                    return -400_000;
                }
            }

            // Countermove bonus
            if let Some(cm) = countermove
                && cm == mv
            {
                return -350_000;
            }

            // Combine history + continuation history for quiet move ordering
            let mut quiet_score: i32 = 0;
            let hist_idx = from_idx * board.total_cells() + to_idx;

            if hist_idx < history[0].len() {
                quiet_score += history[player as usize][hist_idx];
            }

            if let Some(prev_idx) = prev_move_idx {
                let cont_idx = prev_idx * board.total_cells() + to_idx;
                if cont_idx < cont_history[player as usize].len() {
                    quiet_score += cont_history[player as usize][cont_idx];
                }
            }

            -quiet_score
        });
    }

    /// Iterative minimax with PVS, LMR, null-move pruning.
    /// Uses an explicit stack instead of call-stack recursion.
    fn minimax(
        &self,
        board: &mut Board,
        depth: usize,
        alpha: i32,
        beta: i32,
        player: Player,
        start_time: Instant,
        allow_null: bool,
        killers: &mut [[Option<Move>; 2]],
        history: &mut [Vec<i32>],
        countermoves: &mut [Vec<Option<Move>>],
        cont_history: &mut [Vec<i32>],
        prev_move_to_idx: Option<usize>,
    ) -> i32 {
        let mut stack: Vec<SearchFrame> = Vec::with_capacity(depth + 1);
        let mut return_value: i32 = 0;

        let mut initial = SearchFrame::new(depth, alpha, beta, player, allow_null);
        initial.prev_move_to_idx = prev_move_to_idx;
        stack.push(initial);

        'outer: loop {
            let d = stack.len() - 1;

            // ========== HANDLE CHILD RETURNS ==========
            match stack[d].phase {
                SearchPhase::NullMoveReturn => {
                    let score = -return_value;
                    let null_info = stack[d].null_move_info.take().unwrap();
                    board.unmake_null_move(null_info);

                    if score >= stack[d].beta {
                        return_value = stack[d].beta;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }
                    // Null move didn't cause cutoff, proceed to generate moves
                    stack[d].phase = SearchPhase::GenerateMoves;
                    // fall through
                }

                SearchPhase::MoveSearchReturn => {
                    let score = -return_value;
                    let (mv, info) = stack[d].pending_unmake.take().unwrap();

                    // For non-PV moves (i > 0), check if we need LMR re-search or PVS re-search
                    let move_index = stack[d].current_move_index;
                    let reduction = stack[d].current_reduction;

                    if move_index > 0 {
                        let ext = stack[d].current_extension;
                        let re_search_to = board.coords_to_index(&mv.to.values);
                        // This was a scout/LMR search
                        if score > stack[d].alpha && reduction > 0 {
                            // LMR re-search needed: search at full depth with scout window
                            stack[d].phase = SearchPhase::LmrReSearchReturn;
                            stack[d].pending_unmake = Some((mv, info));

                            let child_depth = stack[d].depth - 1 + ext;
                            let child_alpha = -stack[d].alpha - 1;
                            let child_beta = -stack[d].alpha;
                            let child_player = stack[d].player.opponent();

                            let mut child = SearchFrame::new(
                                child_depth,
                                child_alpha,
                                child_beta,
                                child_player,
                                true,
                            );
                            child.prev_move_to_idx = re_search_to;
                            stack.push(child);
                            continue 'outer;
                        }

                        if score > stack[d].alpha && score < stack[d].beta {
                            // PVS re-search needed: search with full window
                            stack[d].phase = SearchPhase::PvsReSearchReturn;
                            stack[d].pending_unmake = Some((mv, info));

                            let child_depth = stack[d].depth - 1 + ext;
                            let child_alpha = -stack[d].beta;
                            let child_beta = -stack[d].alpha;
                            let child_player = stack[d].player.opponent();

                            let mut child = SearchFrame::new(
                                child_depth,
                                child_alpha,
                                child_beta,
                                child_player,
                                true,
                            );
                            child.prev_move_to_idx = re_search_to;
                            stack.push(child);
                            continue 'outer;
                        }
                    }

                    // Process the score
                    self.process_move_result(
                        &mut stack[d],
                        board,
                        &mv,
                        score,
                        killers,
                        history,
                        countermoves,
                        cont_history,
                    );
                    board.unmake_move(&mv, info);

                    if self.stop_flag.load(Ordering::Relaxed) {
                        return_value = 0;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }

                    if stack[d].alpha >= stack[d].beta {
                        // Beta cutoff — store TT and return
                        self.store_tt(board, &stack[d]);
                        return_value = stack[d].best_score;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }

                    stack[d].phase = SearchPhase::ProcessMoves;
                    // fall through to next move
                }

                SearchPhase::LmrReSearchReturn => {
                    let score = -return_value;
                    let to_vals = stack[d]
                        .pending_unmake
                        .as_ref()
                        .unwrap()
                        .0
                        .to
                        .values
                        .clone();

                    if score > stack[d].alpha && score < stack[d].beta {
                        // PVS re-search needed
                        stack[d].phase = SearchPhase::PvsReSearchReturn;

                        let re_to = board.coords_to_index(&to_vals);
                        let child_depth = stack[d].depth - 1 + stack[d].current_extension;
                        let child_alpha = -stack[d].beta;
                        let child_beta = -stack[d].alpha;
                        let child_player = stack[d].player.opponent();

                        let mut child = SearchFrame::new(
                            child_depth,
                            child_alpha,
                            child_beta,
                            child_player,
                            true,
                        );
                        child.prev_move_to_idx = re_to;
                        stack.push(child);
                        continue 'outer;
                    }

                    // Process the score from LMR re-search
                    let (mv, info) = stack[d].pending_unmake.take().unwrap();
                    let mv_clone = mv.clone();

                    self.process_move_result(
                        &mut stack[d],
                        board,
                        &mv_clone,
                        score,
                        killers,
                        history,
                        countermoves,
                        cont_history,
                    );
                    board.unmake_move(&mv, info);

                    if self.stop_flag.load(Ordering::Relaxed) {
                        return_value = 0;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }

                    if stack[d].alpha >= stack[d].beta {
                        self.store_tt(board, &stack[d]);
                        return_value = stack[d].best_score;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }

                    stack[d].phase = SearchPhase::ProcessMoves;
                }

                SearchPhase::IidReturn => {
                    // IID search complete. Re-probe TT for the move it found.
                    let hash = stack[d].hash;
                    if let Some((_, _, _, best_m)) = self.tt.get(hash) {
                        stack[d].tt_move = best_m;
                    }
                    // Now proceed to check detection → null move → razor → generate moves
                    // We need to go through the rest of Init logic, so set phase
                    // and jump to the in-check detection.
                    stack[d].phase = SearchPhase::GenerateMoves;
                    // Detect in-check at entry
                    stack[d].in_check_at_entry =
                        if let Some(king_pos) = board.get_king_coordinate(stack[d].player) {
                            Rules::is_square_attacked(board, &king_pos, stack[d].player.opponent())
                        } else {
                            false
                        };
                    // fall through to GenerateMoves
                }

                SearchPhase::SingularSearchReturn => {
                    // Verification search returned. If the score is below singular_beta,
                    // the TT move is singular — give it an extra extension ply.
                    if return_value < stack[d].singular_beta {
                        stack[d].singular_extension = 1;
                    }
                    stack[d].phase = SearchPhase::ProcessMoves;
                    // fall through to ProcessMoves
                }

                SearchPhase::PvsReSearchReturn => {
                    let score = -return_value;
                    let (mv, info) = stack[d].pending_unmake.take().unwrap();

                    self.process_move_result(
                        &mut stack[d],
                        board,
                        &mv,
                        score,
                        killers,
                        history,
                        countermoves,
                        cont_history,
                    );
                    board.unmake_move(&mv, info);

                    if self.stop_flag.load(Ordering::Relaxed) {
                        return_value = 0;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }

                    if stack[d].alpha >= stack[d].beta {
                        self.store_tt(board, &stack[d]);
                        return_value = stack[d].best_score;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }

                    stack[d].phase = SearchPhase::ProcessMoves;
                }

                _ => {} // Init, GenerateMoves, ProcessMoves — handled below
            }

            // ========== INITIALIZATION ==========
            if stack[d].phase == SearchPhase::Init {
                if self
                    .nodes_searched
                    .fetch_add(1, Ordering::Relaxed)
                    .is_multiple_of(TIMEOUT_CHECK_INTERVAL)
                    && start_time.elapsed() > self.time_limit
                {
                    self.stop_flag.store(true, Ordering::Relaxed);
                    return_value = 0;
                    stack.pop();
                    if stack.is_empty() {
                        return return_value;
                    }
                    continue;
                }
                if self.stop_flag.load(Ordering::Relaxed) {
                    return_value = 0;
                    stack.pop();
                    if stack.is_empty() {
                        return return_value;
                    }
                    continue;
                }

                let hash = board.state.hash;
                stack[d].hash = hash;

                if let Some((tt_score, tt_depth, tt_flag, best_m)) = self.tt.get(hash) {
                    stack[d].tt_move = best_m;
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

                stack[d].original_alpha = stack[d].alpha;

                if stack[d].depth == 0 {
                    return_value =
                        self.q_search(board, stack[d].alpha, stack[d].beta, stack[d].player);
                    stack.pop();
                    if stack.is_empty() {
                        return return_value;
                    }
                    continue;
                }

                // Internal Iterative Deepening: at PV nodes with no TT move
                // and sufficient depth, do a reduced search to get a move to try first.
                let is_pv = stack[d].beta > stack[d].alpha.saturating_add(1);
                if is_pv && stack[d].tt_move.is_none() && stack[d].depth >= 5 {
                    stack[d].phase = SearchPhase::IidReturn;

                    let iid_depth = stack[d].depth - 2;
                    let child_alpha = stack[d].alpha;
                    let child_beta = stack[d].beta;
                    let child_player = stack[d].player;

                    stack.push(SearchFrame::new(
                        iid_depth,
                        child_alpha,
                        child_beta,
                        child_player,
                        stack[d].allow_null,
                    ));
                    continue 'outer;
                }

                // Detect in-check at entry (used by null move, razor, etc.)
                stack[d].in_check_at_entry =
                    if let Some(king_pos) = board.get_king_coordinate(stack[d].player) {
                        Rules::is_square_attacked(board, &king_pos, stack[d].player.opponent())
                    } else {
                        false
                    };

                // Null move pruning
                if stack[d].allow_null && stack[d].depth >= 3 {
                    let static_eval = self.evaluate(board, Some(stack[d].player));
                    if static_eval >= stack[d].beta && !stack[d].in_check_at_entry {
                        let r = if stack[d].depth > 6 { 3 } else { 2 };
                        let null_info = board.make_null_move();
                        stack[d].null_move_info = Some(null_info);
                        stack[d].phase = SearchPhase::NullMoveReturn;

                        let child_depth = stack[d].depth - 1 - r;
                        let child_alpha = -stack[d].beta;
                        let child_beta = -stack[d].beta + 1;
                        let child_player = stack[d].player.opponent();

                        stack.push(SearchFrame::new(
                            child_depth,
                            child_alpha,
                            child_beta,
                            child_player,
                            false,
                        ));
                        continue 'outer;
                    }
                }

                // Razor pruning: at shallow depths, if static eval is far below
                // alpha, drop straight to qsearch.
                if stack[d].depth <= 3 && !stack[d].in_check_at_entry {
                    let static_eval = self.evaluate(board, Some(stack[d].player));
                    if static_eval + RAZOR_MARGIN[stack[d].depth] < stack[d].alpha {
                        let qval =
                            self.q_search(board, stack[d].alpha, stack[d].beta, stack[d].player);
                        if qval < stack[d].alpha {
                            return_value = qval;
                            stack.pop();
                            if stack.is_empty() {
                                return return_value;
                            }
                            continue;
                        }
                    }
                }

                stack[d].phase = SearchPhase::GenerateMoves;
                // fall through
            }

            // ========== GENERATE MOVES ==========
            if stack[d].phase == SearchPhase::GenerateMoves {
                let mut moves = Rules::generate_pseudo_legal_moves(board, stack[d].player);

                if moves.is_empty() {
                    if let Some(king_pos) = board.get_king_coordinate(stack[d].player)
                        && Rules::is_square_attacked(board, &king_pos, stack[d].player.opponent())
                    {
                        return_value = -CHECKMATE_SCORE + (self.depth - stack[d].depth) as i32;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }
                    return_value = 0;
                    stack.pop();
                    if stack.is_empty() {
                        return return_value;
                    }
                    continue;
                }

                // Futility pruning setup
                if stack[d].depth == 1 {
                    let eval = self.evaluate(board, Some(stack[d].player));
                    if eval + 500 < stack[d].alpha {
                        stack[d].do_futility = true;
                    }
                }

                // Reuse the in-check detection from Init phase
                stack[d].in_check = stack[d].in_check_at_entry;

                let my_killers = if stack[d].depth < killers.len() {
                    Some(&killers[stack[d].depth])
                } else {
                    None
                };

                // Look up countermove for this position
                let cm = if let Some(prev_to) = stack[d].prev_move_to_idx {
                    let opp = stack[d].player.opponent() as usize;
                    if prev_to < countermoves[opp].len() {
                        countermoves[opp][prev_to].as_ref()
                    } else {
                        None
                    }
                } else {
                    None
                };

                self.sort_moves(
                    board,
                    &mut moves,
                    stack[d].tt_move,
                    my_killers,
                    history,
                    cm,
                    cont_history,
                    stack[d].prev_move_to_idx,
                    stack[d].player,
                );

                stack[d].moves = moves;
                stack[d].move_idx = 0;

                // Multi-cut pruning: at non-PV cut nodes with sufficient depth,
                // try the first MC_M moves at reduced depth. If MC_C of them
                // beat beta, this node almost certainly fails high — prune it.
                let is_pv_node = stack[d].beta > stack[d].alpha.saturating_add(1);
                if !is_pv_node
                    && stack[d].depth >= MC_DEPTH_MIN
                    && !stack[d].in_check
                    && stack[d].allow_null
                {
                    let mc_depth = stack[d].depth.saturating_sub(MC_REDUCTION);
                    let mut cutoffs = 0;
                    let tries = stack[d].moves.len().min(MC_M);

                    for mi in 0..tries {
                        let mv = stack[d].moves[mi].clone();
                        if let Ok(info) = board.apply_move(&mv) {
                            // Legality check
                            let illegal = if let Some(kp) =
                                board.get_king_coordinate(stack[d].player)
                            {
                                Rules::is_square_attacked(board, &kp, stack[d].player.opponent())
                            } else {
                                false
                            };
                            if !illegal {
                                let child_score = -self.minimax(
                                    board,
                                    mc_depth,
                                    -stack[d].beta,
                                    -stack[d].beta + 1,
                                    stack[d].player.opponent(),
                                    start_time,
                                    false,
                                    killers,
                                    history,
                                    countermoves,
                                    cont_history,
                                    board.coords_to_index(&mv.to.values),
                                );
                                if child_score >= stack[d].beta {
                                    cutoffs += 1;
                                }
                            }
                            board.unmake_move(&mv, info);

                            if cutoffs >= MC_C {
                                return_value = stack[d].beta;
                                stack.pop();
                                if stack.is_empty() {
                                    return return_value;
                                }
                                continue 'outer;
                            }
                        }
                    }
                }

                // Singular extension verification:
                // If we have a TT move at sufficient depth with a non-upper-bound score,
                // verify it's singular by searching all other moves at reduced depth.
                // If they all fail low by a margin, extend the TT move.
                if stack[d].depth >= 8
                    && stack[d].tt_move.is_some()
                    && !stack[d].in_check
                    && let Some((tt_score, tt_depth, tt_flag, _)) = self.tt.get(stack[d].hash)
                    && tt_depth as usize >= stack[d].depth - 3
                    && (tt_flag == Flag::Exact || tt_flag == Flag::LowerBound)
                    && tt_score.abs() < CHECKMATE_SCORE - 100
                {
                    stack[d].singular_beta = tt_score - 2 * stack[d].depth as i32;
                    stack[d].singular_tt_move = stack[d].tt_move;
                    stack[d].phase = SearchPhase::SingularSearchReturn;

                    // Do a reduced-depth search excluding the TT move
                    // We'll do this manually: search all moves except TT move at reduced depth
                    // Using a separate minimax call is expensive in iterative framework.
                    // Instead, we'll use the TT-populated shallow search approach:
                    // search at depth/2 with the singular beta as the window.
                    let se_depth = stack[d].depth / 2;
                    let se_alpha = stack[d].singular_beta - 1;
                    let se_beta = stack[d].singular_beta;

                    stack.push(SearchFrame::new(
                        se_depth,
                        se_alpha,
                        se_beta,
                        stack[d].player,
                        false, // no null move in verification
                    ));
                    // The singular verification child will search the full position.
                    // The TT move will get its TT cutoff; if others all fail low,
                    // the result will be < singular_beta.
                    continue 'outer;
                }

                stack[d].phase = SearchPhase::ProcessMoves;
                // fall through
            }

            // ========== PROCESS MOVES ==========
            if stack[d].phase == SearchPhase::ProcessMoves {
                while stack[d].move_idx < stack[d].moves.len() {
                    let i = stack[d].move_idx;
                    let mv = stack[d].moves[i].clone();
                    stack[d].move_idx += 1;

                    let info = match board.apply_move(&mv) {
                        Ok(info) => info,
                        Err(_) => continue,
                    };

                    // Legality check
                    if let Some(king_pos) = board.get_king_coordinate(stack[d].player)
                        && Rules::is_square_attacked(board, &king_pos, stack[d].player.opponent())
                    {
                        board.unmake_move(&mv, info);
                        continue;
                    }
                    stack[d].legal_count += 1;
                    let legal_idx = stack[d].legal_count - 1; // 0-based legal move index

                    let is_capture = info.captured.is_some();
                    let is_promotion = mv.promotion.is_some();

                    // Check extension: if this move gives check, extend depth by 1
                    let gives_check = if let Some(opp_king) =
                        board.get_king_coordinate(stack[d].player.opponent())
                    {
                        Rules::is_square_attacked(board, &opp_king, stack[d].player)
                    } else {
                        false
                    };
                    let mut extension: usize = if gives_check { 1 } else { 0 };

                    // Apply singular extension to the TT move (always first after sorting)
                    if legal_idx == 0
                        && stack[d].singular_extension > 0
                        && let Some(tm) = stack[d].singular_tt_move
                    {
                        let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(usize::MAX);
                        let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(usize::MAX);
                        if tm.from_idx as usize == from_idx && tm.to_idx as usize == to_idx {
                            extension += stack[d].singular_extension;
                        }
                    }

                    // Futility pruning
                    if stack[d].do_futility
                        && !is_capture
                        && !is_promotion
                        && !stack[d].in_check
                        && !gives_check
                    {
                        board.unmake_move(&mv, info);
                        continue;
                    }

                    // LMR reduction
                    let mut reduction = 0;
                    if legal_idx >= 4
                        && stack[d].depth >= 3
                        && !is_capture
                        && !is_promotion
                        && !stack[d].in_check
                        && !gives_check
                    {
                        reduction = if stack[d].depth > 6 { 2 } else { 1 };
                        if stack[d].depth - 1 + extension < reduction {
                            reduction = 0;
                        }
                    }

                    stack[d].current_move_index = legal_idx;
                    stack[d].current_reduction = reduction;
                    stack[d].current_extension = extension;
                    let move_to_idx = board.coords_to_index(&mv.to.values);
                    stack[d].pending_unmake = Some((mv, info));
                    stack[d].phase = SearchPhase::MoveSearchReturn;

                    let child_player = stack[d].player.opponent();
                    let base_depth = stack[d].depth - 1 + extension;

                    if legal_idx == 0 {
                        // PV move: full window
                        let child_depth = base_depth;
                        let child_alpha = -stack[d].beta;
                        let child_beta = -stack[d].alpha;

                        let mut child = SearchFrame::new(
                            child_depth,
                            child_alpha,
                            child_beta,
                            child_player,
                            true,
                        );
                        child.prev_move_to_idx = move_to_idx;
                        stack.push(child);
                    } else {
                        // Scout/LMR search: null window
                        let child_depth = base_depth - reduction;
                        let child_alpha = -stack[d].alpha - 1;
                        let child_beta = -stack[d].alpha;

                        let mut child = SearchFrame::new(
                            child_depth,
                            child_alpha,
                            child_beta,
                            child_player,
                            true,
                        );
                        child.prev_move_to_idx = move_to_idx;
                        stack.push(child);
                    }
                    continue 'outer;
                }

                // All moves exhausted
                if stack[d].legal_count == 0 {
                    if stack[d].in_check {
                        return_value = -CHECKMATE_SCORE + (self.depth - stack[d].depth) as i32;
                    } else {
                        return_value = 0;
                    }
                    stack.pop();
                    if stack.is_empty() {
                        return return_value;
                    }
                    continue;
                }

                self.store_tt(board, &stack[d]);
                return_value = stack[d].best_score;
                stack.pop();
                if stack.is_empty() {
                    return return_value;
                }
                continue;
            }
        }
    }

    /// Process the result of a child search for a move.
    fn process_move_result(
        &self,
        frame: &mut SearchFrame,
        board: &Board,
        mv: &Move,
        score: i32,
        killers: &mut [[Option<Move>; 2]],
        history: &mut [Vec<i32>],
        countermoves: &mut [Vec<Option<Move>>],
        cont_history: &mut [Vec<i32>],
    ) {
        if score > frame.best_score {
            frame.best_score = score;
            frame.best_move_obj = Some(mv.clone());
        }

        if score > frame.alpha {
            frame.alpha = score;

            let is_capture = frame
                .pending_unmake
                .as_ref()
                .map(|(_, info)| info.captured.is_some())
                .unwrap_or(false);
            let is_promotion = mv.promotion.is_some();

            if !is_capture && !is_promotion {
                let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(0);
                let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);
                let hist_idx = from_idx * board.total_cells() + to_idx;

                if hist_idx < history[frame.player as usize].len() {
                    let bonus = (frame.depth * frame.depth) as i32;
                    let current = history[frame.player as usize][hist_idx];
                    if current + bonus < MAX_HISTORY {
                        history[frame.player as usize][hist_idx] += bonus;
                    }
                }
            }
        }

        if frame.alpha >= frame.beta {
            let is_capture = frame
                .pending_unmake
                .as_ref()
                .map(|(_, info)| info.captured.is_some())
                .unwrap_or(false);
            let is_promotion = mv.promotion.is_some();

            if !is_capture && !is_promotion {
                // Killer moves
                if frame.depth < killers.len() {
                    killers[frame.depth][1] = killers[frame.depth][0].clone();
                    killers[frame.depth][0] = Some(mv.clone());
                }

                // Countermove: store this move as the refutation of the previous move
                if let Some(prev_to) = frame.prev_move_to_idx {
                    let opp = frame.player.opponent() as usize;
                    if prev_to < countermoves[opp].len() {
                        countermoves[opp][prev_to] = Some(mv.clone());
                    }
                }

                // Continuation history: update score for (prev_move_to, this_move_to) pair
                let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);
                if let Some(prev_to) = frame.prev_move_to_idx {
                    let cont_idx = prev_to * board.total_cells() + to_idx;
                    if cont_idx < cont_history[frame.player as usize].len() {
                        let bonus = (frame.depth * frame.depth) as i32;
                        let current = cont_history[frame.player as usize][cont_idx];
                        if current + bonus < MAX_HISTORY {
                            cont_history[frame.player as usize][cont_idx] += bonus;
                        }
                    }
                }
            }
        }
    }

    fn store_tt(&self, board: &Board, frame: &SearchFrame) {
        let flag = if frame.best_score <= frame.original_alpha {
            Flag::UpperBound
        } else if frame.best_score >= frame.beta {
            Flag::LowerBound
        } else {
            Flag::Exact
        };

        let packed_move = frame.best_move_obj.as_ref().and_then(|m| {
            let from = board.coords_to_index(&m.from.values)?;
            let to = board.coords_to_index(&m.to.values)?;
            let promo = match m.promotion {
                None => 0,
                Some(PieceType::Queen) => 1,
                Some(PieceType::Rook) => 2,
                Some(PieceType::Bishop) => 3,
                Some(PieceType::Knight) => 4,
                Some(PieceType::King) | Some(PieceType::Pawn) => 0,
            };
            Some(PackedMove {
                from_idx: from as u16,
                to_idx: to as u16,
                promotion: promo,
            })
        });

        self.tt.store(
            frame.hash,
            frame.best_score,
            frame.depth as u8,
            flag,
            packed_move,
        );
    }
}

// ---------------------------------------------------------------------------
// Search frame for iterative minimax
// ---------------------------------------------------------------------------

#[derive(PartialEq, Eq, Clone, Copy)]
enum SearchPhase {
    Init,
    NullMoveReturn,
    IidReturn,
    SingularSearchReturn,
    GenerateMoves,
    ProcessMoves,
    MoveSearchReturn,
    LmrReSearchReturn,
    PvsReSearchReturn,
}

struct SearchFrame {
    depth: usize,
    alpha: i32,
    beta: i32,
    original_alpha: i32,
    player: Player,
    allow_null: bool,
    phase: SearchPhase,
    hash: u64,
    tt_move: Option<PackedMove>,
    moves: MoveList,
    move_idx: usize,
    best_score: i32,
    best_move_obj: Option<Move>,
    legal_count: usize,
    in_check: bool,
    in_check_at_entry: bool,
    do_futility: bool,
    current_move_index: usize,
    current_reduction: usize,
    current_extension: usize,
    pending_unmake: Option<(Move, UnmakeInfo)>,
    null_move_info: Option<UnmakeInfo>,
    /// The to-index of the move that led to this node (from parent).
    prev_move_to_idx: Option<usize>,
    /// Singular extension fields
    singular_beta: i32,
    singular_tt_move: Option<PackedMove>,
    singular_extension: usize,
}

impl SearchFrame {
    fn new(depth: usize, alpha: i32, beta: i32, player: Player, allow_null: bool) -> Self {
        Self {
            depth,
            alpha,
            beta,
            original_alpha: alpha,
            player,
            allow_null,
            phase: SearchPhase::Init,
            hash: 0,
            tt_move: None,
            moves: MoveList::new(),
            move_idx: 0,
            best_score: -i32::MAX,
            best_move_obj: None,
            legal_count: 0,
            in_check: false,
            in_check_at_entry: false,
            do_futility: false,
            current_move_index: 0,
            current_reduction: 0,
            current_extension: 0,
            pending_unmake: None,
            null_move_info: None,
            prev_move_to_idx: None,
            singular_beta: 0,
            singular_tt_move: None,
            singular_extension: 0,
        }
    }
}

impl PlayerStrategy for MinimaxBot {
    fn get_move(&mut self, board: &Board, player: Player) -> Option<Move> {
        self.nodes_searched.store(0, Ordering::Relaxed);
        self.stop_flag.store(false, Ordering::Relaxed);

        let start_time = Instant::now();
        let root_moves = Rules::generate_legal_moves(&mut board.clone(), player);
        if root_moves.is_empty() {
            return None;
        }

        let nodes_counter = self.nodes_searched.clone();
        let stop_flag = self.stop_flag.clone();

        let search_active = Arc::new(AtomicBool::new(true));
        let search_active_clone = search_active.clone();

        thread::spawn(move || {
            let mut last_nodes = 0;
            let mut last_time = Instant::now();

            while search_active_clone.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(500));

                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                let current_nodes = nodes_counter.load(Ordering::Relaxed);
                let now = Instant::now();
                let duration = now.duration_since(last_time).as_secs_f64();

                if duration > 0.0 {
                    let nps = (current_nodes - last_nodes) as f64 / duration;
                    let nps_fmt = if nps > 1_000_000.0 {
                        format!("{:.2} MN/s", nps / 1_000_000.0)
                    } else {
                        format!("{:.2} kN/s", nps / 1_000.0)
                    };

                    print!(
                        "\rinfo nodes {} nps {} time {:.1}s  ",
                        current_nodes,
                        nps_fmt,
                        start_time.elapsed().as_secs_f32()
                    );
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                }

                last_nodes = current_nodes;
                last_time = now;
            }
        });

        let results: Vec<(Move, i32)> = (0..self.num_threads)
            .into_par_iter()
            .map(|thread_idx| {
                let mut local_board = board.clone();
                let mut local_best_move = None;
                let mut local_best_score = -i32::MAX;

                let mut my_moves = root_moves.clone();
                if thread_idx > 0 {
                    use rand::seq::SliceRandom;
                    let mut rng = rand::thread_rng();
                    my_moves.shuffle(&mut rng);
                }

                let mut prev_score = 0;
                let mut killers = (0..=self.depth).map(|_| [None, None]).collect::<Vec<_>>();

                let total_cells = local_board.total_cells();
                let hist_size = total_cells * total_cells;
                let mut history = vec![vec![0i32; hist_size], vec![0i32; hist_size]];
                let mut countermoves = vec![vec![None; total_cells], vec![None; total_cells]];
                let mut cont_history = vec![vec![0i32; hist_size], vec![0i32; hist_size]];

                for d in 1..=self.depth {
                    let mut delta = 50;
                    let mut alpha;
                    let mut beta;

                    if d > 4 {
                        alpha = prev_score - delta;
                        beta = prev_score + delta;
                    } else {
                        alpha = -i32::MAX;
                        beta = i32::MAX;
                    }

                    loop {
                        let mut best_score_this_iter = -i32::MAX;
                        let mut best_move_this_iter = None;
                        let mut alpha_inner = alpha;
                        let mut failed_high = false;
                        let mut failed_low = false;

                        for mv in &my_moves {
                            let mv_to_idx = local_board.coords_to_index(&mv.to.values);
                            let info = local_board.apply_move(mv).unwrap();
                            let score = -self.minimax(
                                &mut local_board,
                                d - 1,
                                -beta,
                                -alpha_inner,
                                player.opponent(),
                                start_time,
                                true,
                                &mut killers,
                                &mut history,
                                &mut countermoves,
                                &mut cont_history,
                                mv_to_idx,
                            );
                            local_board.unmake_move(mv, info);

                            if self.stop_flag.load(Ordering::Relaxed) {
                                break;
                            }

                            if score > best_score_this_iter {
                                best_score_this_iter = score;
                                best_move_this_iter = Some(mv.clone());
                            }
                            if score > alpha_inner {
                                alpha_inner = score;
                            }
                            if score >= beta {
                                failed_high = true;
                                break;
                            }
                        }

                        if self.stop_flag.load(Ordering::Relaxed) {
                            local_best_score = best_score_this_iter;
                            break;
                        }

                        if best_score_this_iter <= alpha {
                            failed_low = true;
                        }

                        if d > 4 {
                            if failed_low {
                                beta = (alpha + beta) / 2;
                                alpha -= delta;
                                delta += delta / 2;
                                continue;
                            }
                            if failed_high {
                                beta += delta;
                                delta += delta / 2;
                                continue;
                            }
                        }

                        local_best_score = best_score_this_iter;
                        local_best_move = best_move_this_iter;
                        prev_score = local_best_score;
                        break;
                    }
                    if self.stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                }

                (
                    local_best_move.unwrap_or(my_moves[0].clone()),
                    local_best_score,
                )
            })
            .collect();

        search_active.store(false, Ordering::Relaxed);

        println!();

        let best = results.into_iter().max_by_key(|r| r.1);

        best.map(|(m, _)| m)
    }
}
