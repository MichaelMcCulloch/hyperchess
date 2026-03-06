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
use super::see::SEE;

const CHECKMATE_SCORE: i32 = 30000;
const TIMEOUT_CHECK_INTERVAL: usize = 2048;

/// History heuristic cap (Stockfish uses 7183; we use the same scale).
const MAX_HISTORY: i32 = 7183;

/// Razor margins by depth (indexed by depth: 1, 2, 3).
const RAZOR_MARGIN: [i32; 4] = [0, 300, 500, 700];

/// Number of continuation history plies (#11): track up to 4 plies back.
const CONT_HIST_PLIES: usize = 4;

/// Low-ply history depth (#13): separate history for plies 0..LOW_PLY_MAX near root.
const LOW_PLY_MAX: usize = 5;

/// Pawn history table size (#12): pawn-hash-indexed move ordering.
const PAWN_HIST_SIZE: usize = 8192;

/// Multi-cut parameters
const MC_M: usize = 6;
const MC_C: usize = 3;
const MC_DEPTH_MIN: usize = 5;
const MC_REDUCTION: usize = 4;

/// LMR reduction lookup table (indexed by depth and move count).
/// Initialized as: reductions[d][m] = (2809/128 * ln(d)) * (ln(m) / ln(64))
/// We use a 64×64 table covering practical ranges.
const LMR_TABLE_SIZE: usize = 64;

fn compute_lmr_table() -> [[i32; LMR_TABLE_SIZE]; LMR_TABLE_SIZE] {
    let mut table = [[0i32; LMR_TABLE_SIZE]; LMR_TABLE_SIZE];
    for d in 1..LMR_TABLE_SIZE {
        for m in 1..LMR_TABLE_SIZE {
            // Stockfish formula: 2809/128 * ln(d) * ln(m) / ln(64)
            // Simplified: ~21.95 * ln(d) * ln(m) / 4.16 = ~5.27 * ln(d) * ln(m)
            // But we want integer plies, so we scale differently.
            // Stockfish stores reductions * 1024 for fractional plies.
            // We'll store whole plies (simpler for our iterative framework).
            let r = (0.8 + (d as f64).ln() * (m as f64).ln() / 2.4) as i32;
            table[d][m] = r.max(0);
        }
    }
    table
}

/// Stockfish-style history update with gravity/decay.
/// val = val + clamp(bonus, -cap, cap) - val * abs(bonus) / cap
#[inline]
fn update_history(entry: &mut i32, bonus: i32) {
    let clamped = bonus.clamp(-MAX_HISTORY, MAX_HISTORY);
    *entry += clamped - *entry * clamped.abs() / MAX_HISTORY;
}

/// Build a child's ancestor_to_idx array: new_move_to becomes [0], parent's [0] becomes [1], etc.
#[inline]
fn shift_ancestors(
    parent: &[Option<usize>; CONT_HIST_PLIES],
    new_move_to: Option<usize>,
) -> [Option<usize>; CONT_HIST_PLIES] {
    let mut child = [None; CONT_HIST_PLIES];
    child[0] = new_move_to;
    for i in 1..CONT_HIST_PLIES {
        child[i] = parent[i - 1];
    }
    child
}

/// Correction history (#18): tracks static eval error by pawn structure hash.
/// Stores a weighted running average of (search_score - static_eval).
const CORRECTION_TABLE_SIZE: usize = 16384;

struct CorrectionHistory {
    table: Vec<(i32, i32)>, // (weighted_sum, total_weight) per slot
}

impl CorrectionHistory {
    fn new() -> Self {
        Self {
            table: vec![(0, 0); CORRECTION_TABLE_SIZE],
        }
    }

    fn get(&self, pawn_hash: u64) -> i32 {
        let idx = (pawn_hash as usize) & (CORRECTION_TABLE_SIZE - 1);
        let (sum, weight) = self.table[idx];
        if weight > 0 { sum / weight } else { 0 }
    }

    fn update(&mut self, pawn_hash: u64, error: i32, weight: i32) {
        let idx = (pawn_hash as usize) & (CORRECTION_TABLE_SIZE - 1);
        let entry = &mut self.table[idx];
        // Exponential decay: halve old data when weight grows large
        if entry.1 > 256 {
            entry.0 /= 2;
            entry.1 /= 2;
        }
        entry.0 += error * weight;
        entry.1 += weight;
    }
}

/// Map a piece at a board index to a 0-5 type index for capture history.
#[inline]
fn piece_type_index(board: &Board, idx: usize) -> usize {
    if board.pieces.pawns.get_bit(idx) {
        0
    } else if board.pieces.knights.get_bit(idx) {
        1
    } else if board.pieces.bishops.get_bit(idx) {
        2
    } else if board.pieces.rooks.get_bit(idx) {
        3
    } else if board.pieces.queens.get_bit(idx) {
        4
    } else {
        5
    } // king
}

pub struct MinimaxBot {
    depth: usize,
    time_limit: Duration,
    tt: Arc<LockFreeTT>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: Arc<AtomicUsize>,
    num_threads: usize,
    lmr_table: [[i32; LMR_TABLE_SIZE]; LMR_TABLE_SIZE],
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
            lmr_table: compute_lmr_table(),
        }
    }

    /// Create a MinimaxBot from explicit parameters (used by distributed workers).
    pub fn new_from_params(
        depth: usize,
        time_limit: Duration,
        memory_mb: usize,
        num_threads: usize,
    ) -> Self {
        Self {
            depth,
            time_limit,
            tt: Arc::new(LockFreeTT::new(memory_mb)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: Arc::new(AtomicUsize::new(0)),
            num_threads: num_threads.max(1),
            lmr_table: compute_lmr_table(),
        }
    }

    /// Search only a specified subset of root moves (used by distributed workers).
    /// Returns (best_move, score, nodes_searched, completed).
    pub fn search_subset(
        &mut self,
        board: &Board,
        player: Player,
        root_moves: Vec<Move>,
    ) -> (Move, i32, u64, bool) {
        if root_moves.is_empty() {
            // Should not happen, but return a safe default
            return (
                Move {
                    from: crate::domain::coordinate::Coordinate::new(smallvec::smallvec![0]),
                    to: crate::domain::coordinate::Coordinate::new(smallvec::smallvec![0]),
                    promotion: None,
                },
                -i32::MAX,
                0,
                true,
            );
        }

        self.nodes_searched.store(0, Ordering::Relaxed);
        self.stop_flag.store(false, Ordering::Relaxed);
        let start_time = Instant::now();

        let nodes_counter = self.nodes_searched.clone();
        let stop_flag = self.stop_flag.clone();
        let search_active = Arc::new(AtomicBool::new(true));
        let search_active_clone = search_active.clone();

        // Monitoring thread
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
                    eprint!(
                        "\r[worker] nodes {} nps {} time {:.1}s  ",
                        current_nodes,
                        nps_fmt,
                        start_time.elapsed().as_secs_f32()
                    );
                    use std::io::Write;
                    std::io::stderr().flush().unwrap_or(());
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
                let mut cont_history = vec![vec![0i32; hist_size]; CONT_HIST_PLIES * 2];
                // Capture history (#10): [player][to_square * 6 + captured_piece_type]
                let cap_hist_size = total_cells * 6;
                let mut capture_history =
                    vec![vec![0i32; cap_hist_size], vec![0i32; cap_hist_size]];
                let mut correction_history = CorrectionHistory::new();
                let pawn_hist_entry_size = PAWN_HIST_SIZE * total_cells;
                let mut pawn_history = vec![
                    vec![0i32; pawn_hist_entry_size],
                    vec![0i32; pawn_hist_entry_size],
                ];
                let mut low_ply_history = vec![vec![0i32; hist_size]; LOW_PLY_MAX * 2];

                // Depth staggering (#27)
                let start_depth = if thread_idx == 0 {
                    1
                } else {
                    1 + (thread_idx % 3)
                };
                for d in start_depth..=self.depth {
                    let mut delta = 50;
                    let (mut alpha, mut beta) = if d > 4 {
                        (prev_score - delta, prev_score + delta)
                    } else {
                        (-i32::MAX, i32::MAX)
                    };

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
                                &mut capture_history,
                                &mut correction_history,
                                &mut pawn_history,
                                &mut low_ply_history,
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
                                delta += delta / 3;
                                continue;
                            }
                            if failed_high {
                                beta += delta;
                                delta += delta / 3;
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
        let total_nodes = self.nodes_searched.load(Ordering::Relaxed) as u64;
        let completed = !self.stop_flag.load(Ordering::Relaxed);
        let best = results.into_iter().max_by_key(|r| r.1).unwrap();
        (best.0, best.1, total_nodes, completed)
    }

    /// Evaluate with correction history adjustment (#18).
    fn evaluate_corrected(
        &self,
        board: &Board,
        player_at_leaf: Option<Player>,
        correction_history: &CorrectionHistory,
    ) -> i32 {
        let raw = self.evaluate(board, player_at_leaf);
        let pawn_hash = Self::pawn_hash(board);
        let correction = correction_history.get(pawn_hash);
        // Apply correction scaled down to avoid overshooting
        (raw + correction / 16).clamp(-CHECKMATE_SCORE + 100, CHECKMATE_SCORE - 100)
    }

    /// Update correction history based on search result vs static eval error.
    fn update_correction(
        board: &Board,
        static_eval: i32,
        search_score: i32,
        depth: usize,
        correction_history: &mut CorrectionHistory,
    ) {
        // Only update for non-mate scores and sufficient depth
        if search_score.abs() > CHECKMATE_SCORE - 100 || depth < 2 {
            return;
        }
        let error = search_score - static_eval;
        let pawn_hash = Self::pawn_hash(board);
        let weight = (depth as i32).min(16);
        correction_history.update(pawn_hash, error, weight);
    }

    /// Compute a simple pawn hash for correction history indexing.
    fn pawn_hash(board: &Board) -> u64 {
        // XOR pawn Zobrist keys for both sides
        let mut hash = 0u64;
        for idx in board.pieces.pawns.iter_indices() {
            if board.pieces.white_occupancy.get_bit(idx) {
                hash ^= board.zobrist.piece_keys[idx]; // white pawn
            } else if board.pieces.black_occupancy.get_bit(idx) {
                hash ^= board.zobrist.piece_keys[idx].wrapping_mul(0x9E3779B97F4A7C15);
            }
        }
        hash
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

    /// Score and sort moves. Captures are split into good (SEE >= 0) and bad (SEE < 0).
    /// Bad captures are placed after all quiet moves (#9, #30).
    #[allow(clippy::too_many_arguments)]
    fn sort_moves(
        &self,
        board: &Board,
        moves: &mut [Move],
        tt_move: Option<PackedMove>,
        killers: Option<&[Option<Move>; 2]>,
        history: &[Vec<i32>],
        countermove: Option<&Move>,
        cont_history: &[Vec<i32>],
        capture_history: &[Vec<i32>],
        ancestors: &[Option<usize>; CONT_HIST_PLIES],
        player: Player,
        pawn_history: &[Vec<i32>],
        pawn_hash: u64,
        low_ply_history: &[Vec<i32>],
        ply: usize,
    ) {
        moves.sort_by_cached_key(|mv| {
            let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(0);
            let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);

            // TT move always first
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

            // Captures: split good/bad by SEE (#9), enhanced with capture history (#10)
            if enemy_occupancy.get_bit(to_idx) {
                let victim_val = self.get_piece_value(board, to_idx);
                let attacker_val = self.get_piece_value(board, from_idx);
                let see_val = SEE::static_exchange_evaluation(board, mv);

                // Capture history bonus: [player][to * 6 + captured_type]
                let cap_type = piece_type_index(board, to_idx);
                let cap_hist_idx = to_idx * 6 + cap_type;
                let cap_hist_bonus = if cap_hist_idx < capture_history[player as usize].len() {
                    capture_history[player as usize][cap_hist_idx] / 32
                } else {
                    0
                };

                if see_val >= 0 {
                    // Good capture: MVV-LVA + capture history
                    return -(1_000_000 + 10 * victim_val - attacker_val + cap_hist_bonus);
                } else {
                    // Bad capture: placed after all quiets
                    return -(see_val + cap_hist_bonus);
                }
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

            if let Some(cm) = countermove
                && cm == mv
            {
                return -350_000;
            }

            // Quiet move score: history + multi-ply continuation history + pawn + low-ply
            let mut quiet_score: i32 = 0;
            let total = board.total_cells();
            let hist_idx = from_idx * total + to_idx;

            if hist_idx < history[0].len() {
                quiet_score += history[player as usize][hist_idx];
            }

            // Multi-ply continuation history (#11)
            for ply_back in 0..CONT_HIST_PLIES {
                if let Some(anc_idx) = ancestors[ply_back] {
                    let cont_idx = anc_idx * total + to_idx;
                    let table_idx = ply_back * 2 + player as usize;
                    if table_idx < cont_history.len() && cont_idx < cont_history[table_idx].len() {
                        quiet_score += cont_history[table_idx][cont_idx];
                    }
                }
            }

            // Pawn history (#12)
            let ph_idx = (pawn_hash as usize & (PAWN_HIST_SIZE - 1)) * total + to_idx;
            if ph_idx < pawn_history[player as usize].len() {
                quiet_score += pawn_history[player as usize][ph_idx] / 2;
            }

            // Low-ply history (#13)
            if ply < LOW_PLY_MAX {
                let lp_table = ply * 2 + player as usize;
                if lp_table < low_ply_history.len() && hist_idx < low_ply_history[lp_table].len() {
                    quiet_score += low_ply_history[lp_table][hist_idx] / 2;
                }
            }

            -(200_000 + quiet_score)
        });
    }

    /// Iterative minimax with PVS, LMR, null-move pruning.
    /// Uses an explicit stack instead of call-stack recursion.
    #[allow(clippy::too_many_arguments)]
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
        capture_history: &mut [Vec<i32>],
        correction_history: &mut CorrectionHistory,
        pawn_history: &mut [Vec<i32>],
        low_ply_history: &mut [Vec<i32>],
        prev_move_to_idx: Option<usize>,
    ) -> i32 {
        let mut stack: Vec<SearchFrame> = Vec::with_capacity(depth + 1);
        let mut return_value: i32 = 0;

        let mut initial = SearchFrame::new(depth, alpha, beta, player, allow_null);
        initial.ancestor_to_idx[0] = prev_move_to_idx;
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
                            child.ancestor_to_idx =
                                shift_ancestors(&stack[d].ancestor_to_idx, re_search_to);
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
                            child.ancestor_to_idx =
                                shift_ancestors(&stack[d].ancestor_to_idx, re_search_to);
                            stack.push(child);
                            continue 'outer;
                        }
                    }

                    // Process the score
                    let ply = self.depth.saturating_sub(stack[d].depth);
                    self.process_move_result(
                        &mut stack[d],
                        board,
                        &mv,
                        score,
                        killers,
                        history,
                        countermoves,
                        cont_history,
                        capture_history,
                        pawn_history,
                        low_ply_history,
                        ply,
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
                        child.ancestor_to_idx = shift_ancestors(&stack[d].ancestor_to_idx, re_to);
                        stack.push(child);
                        continue 'outer;
                    }

                    // Process the score from LMR re-search
                    let (mv, info) = stack[d].pending_unmake.take().unwrap();
                    let mv_clone = mv.clone();

                    let ply = self.depth.saturating_sub(stack[d].depth);
                    self.process_move_result(
                        &mut stack[d],
                        board,
                        &mv_clone,
                        score,
                        killers,
                        history,
                        countermoves,
                        cont_history,
                        capture_history,
                        pawn_history,
                        low_ply_history,
                        ply,
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
                    // (#6): Enhanced singular extension handling with negative extensions
                    if return_value < stack[d].singular_beta {
                        // TT move is singular — extend it
                        stack[d].singular_extension = 1;
                        // Double extension if far below singular beta
                        if return_value < stack[d].singular_beta - 2 * stack[d].depth as i32 {
                            stack[d].singular_extension = 2;
                        }
                    } else if return_value >= stack[d].beta {
                        // Multicut: if even the verification search beats beta, prune
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    } else {
                        // Non-singular: apply negative extension to TT move (#6)
                        let is_pv_node = stack[d].beta > stack[d].alpha.saturating_add(1);
                        if !is_pv_node {
                            // Negative extension: reduce the TT move's effective depth
                            stack[d].singular_extension = 0; // no extension (effectively -1 vs normal)
                        }
                    }
                    stack[d].phase = SearchPhase::ProcessMoves;
                    // fall through to ProcessMoves
                }

                SearchPhase::PvsReSearchReturn => {
                    let score = -return_value;
                    let (mv, info) = stack[d].pending_unmake.take().unwrap();

                    let ply = self.depth.saturating_sub(stack[d].depth);
                    self.process_move_result(
                        &mut stack[d],
                        board,
                        &mv,
                        score,
                        killers,
                        history,
                        countermoves,
                        cont_history,
                        capture_history,
                        pawn_history,
                        low_ply_history,
                        ply,
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

                SearchPhase::TryTTMoveReturn => {
                    let score = -return_value;
                    let (mv, info) = stack[d].pending_unmake.take().unwrap();

                    let ply = self.depth.saturating_sub(stack[d].depth);
                    self.process_move_result(
                        &mut stack[d],
                        board,
                        &mv,
                        score,
                        killers,
                        history,
                        countermoves,
                        cont_history,
                        capture_history,
                        pawn_history,
                        low_ply_history,
                        ply,
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
                        // TT move caused cutoff — skip generating remaining moves
                        self.store_tt(board, &stack[d]);
                        return_value = stack[d].best_score;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }

                    // TT move didn't cause cutoff, proceed to generate all moves
                    stack[d].phase = SearchPhase::GenerateMoves;
                    // fall through
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

                // Mate distance pruning (#2): tighten bounds based on shortest possible mate
                {
                    let ply = self.depth - stack[d].depth;
                    let mating_value = CHECKMATE_SCORE - ply as i32;
                    if mating_value < stack[d].beta {
                        stack[d].beta = mating_value;
                        if stack[d].alpha >= mating_value {
                            return_value = mating_value;
                            stack.pop();
                            if stack.is_empty() {
                                return return_value;
                            }
                            continue;
                        }
                    }
                    let mated_value = -CHECKMATE_SCORE + ply as i32;
                    if mated_value > stack[d].alpha {
                        stack[d].alpha = mated_value;
                        if stack[d].beta <= mated_value {
                            return_value = mated_value;
                            stack.pop();
                            if stack.is_empty() {
                                return return_value;
                            }
                            continue;
                        }
                    }
                }

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

                    let mut iid_child = SearchFrame::new(
                        iid_depth,
                        child_alpha,
                        child_beta,
                        child_player,
                        stack[d].allow_null,
                    );
                    iid_child.ancestor_to_idx = stack[d].ancestor_to_idx;
                    stack.push(iid_child);
                    continue 'outer;
                }

                // Detect in-check at entry (used by null move, razor, etc.)
                stack[d].in_check_at_entry =
                    if let Some(king_pos) = board.get_king_coordinate(stack[d].player) {
                        Rules::is_square_attacked(board, &king_pos, stack[d].player.opponent())
                    } else {
                        false
                    };

                // Static eval (cached for use by null move, futility, razoring, probcut)
                // Uses correction history (#18) when available
                let static_eval =
                    self.evaluate_corrected(board, Some(stack[d].player), correction_history);
                stack[d].static_eval = static_eval;

                // Null move pruning (#4): adaptive R = 3 + depth/3
                if stack[d].allow_null
                    && stack[d].depth >= 3
                    && !stack[d].in_check_at_entry
                    && static_eval >= stack[d].beta
                {
                    let r = (3 + stack[d].depth / 3).min(stack[d].depth - 1);
                    let null_info = board.make_null_move();
                    stack[d].null_move_info = Some(null_info);
                    stack[d].phase = SearchPhase::NullMoveReturn;

                    let child_depth = stack[d].depth - 1 - r;
                    let child_alpha = -stack[d].beta;
                    let child_beta = -stack[d].beta + 1;
                    let child_player = stack[d].player.opponent();

                    let mut child =
                        SearchFrame::new(child_depth, child_alpha, child_beta, child_player, false);
                    // Null move verification (#4): set nmp_min_ply to prevent
                    // consecutive null moves near the verification boundary
                    child.nmp_min_ply = self.depth - stack[d].depth + 3 * (stack[d].depth - r) / 4;
                    child.ancestor_to_idx = shift_ancestors(&stack[d].ancestor_to_idx, None);
                    stack.push(child);
                    continue 'outer;
                }

                // ProbCut (#1): if a shallow search at beta+margin proves a cutoff,
                // skip the expensive full-depth search.
                let is_pv_here = stack[d].beta > stack[d].alpha.saturating_add(1);
                if !is_pv_here
                    && stack[d].depth >= 5
                    && !stack[d].in_check_at_entry
                    && stack[d].beta.abs() < CHECKMATE_SCORE - 100
                {
                    let probcut_beta = stack[d].beta + 200;
                    let pc_depth = stack[d].depth - 4;

                    // Generate loud moves (captures + promotions) with SEE >= probcut threshold
                    let loud = Rules::generate_loud_moves(board, stack[d].player);
                    for mv in &loud {
                        let see_val = SEE::static_exchange_evaluation(board, mv);
                        if see_val < probcut_beta - static_eval {
                            continue;
                        }
                        if let Ok(info) = board.apply_move(mv) {
                            let illegal = if let Some(kp) =
                                board.get_king_coordinate(stack[d].player)
                            {
                                Rules::is_square_attacked(board, &kp, stack[d].player.opponent())
                            } else {
                                false
                            };
                            if !illegal {
                                // Quick q-search first
                                let qval = -self.q_search(
                                    board,
                                    -probcut_beta,
                                    -probcut_beta + 1,
                                    stack[d].player.opponent(),
                                );
                                if qval >= probcut_beta {
                                    // Verify with a shallow search
                                    let val = -super::search_core::minimax_shallow(
                                        board,
                                        pc_depth,
                                        -probcut_beta,
                                        -probcut_beta + 1,
                                        stack[d].player.opponent(),
                                        &self.nodes_searched,
                                        &self.stop_flag,
                                        Some(&self.tt),
                                    );
                                    if val >= probcut_beta {
                                        board.unmake_move(mv, info);
                                        // Store in TT at depth+1 (Stockfish does this)
                                        self.tt.store(
                                            stack[d].hash,
                                            val - (probcut_beta - stack[d].beta),
                                            (stack[d].depth + 1) as u8,
                                            Flag::LowerBound,
                                            stack[d].tt_move,
                                        );
                                        return_value = val - (probcut_beta - stack[d].beta);
                                        stack.pop();
                                        if stack.is_empty() {
                                            return return_value;
                                        }
                                        continue 'outer;
                                    }
                                }
                            }
                            board.unmake_move(mv, info);
                        }
                    }
                }

                // Razor pruning: at shallow depths, if static eval is far below
                // alpha, drop straight to qsearch.
                if stack[d].depth <= 3
                    && !stack[d].in_check_at_entry
                    && stack[d].static_eval + RAZOR_MARGIN[stack[d].depth] < stack[d].alpha
                {
                    let qval = self.q_search(board, stack[d].alpha, stack[d].beta, stack[d].player);
                    if qval < stack[d].alpha {
                        return_value = qval;
                        stack.pop();
                        if stack.is_empty() {
                            return return_value;
                        }
                        continue;
                    }
                }

                // Staged move generation (#8): try TT move before generating all moves
                if let Some(tm) = stack[d].tt_move {
                    let from_coords = board.index_to_coords(tm.from_idx as usize);
                    let to_coords = board.index_to_coords(tm.to_idx as usize);
                    let promotion = match tm.promotion {
                        1 => Some(PieceType::Queen),
                        2 => Some(PieceType::Rook),
                        3 => Some(PieceType::Bishop),
                        4 => Some(PieceType::Knight),
                        _ => None,
                    };
                    let tt_mv = Move {
                        from: crate::domain::coordinate::Coordinate::new(from_coords),
                        to: crate::domain::coordinate::Coordinate::new(to_coords),
                        promotion,
                    };
                    if let Ok(info) = board.apply_move(&tt_mv) {
                        let illegal = if let Some(king_pos) =
                            board.get_king_coordinate(stack[d].player)
                        {
                            Rules::is_square_attacked(board, &king_pos, stack[d].player.opponent())
                        } else {
                            false
                        };
                        if !illegal {
                            stack[d].tt_move_tried = true;
                            stack[d].legal_count += 1;
                            stack[d].current_move_index = 0;
                            stack[d].current_reduction = 0;

                            let gives_check = if let Some(opp_king) =
                                board.get_king_coordinate(stack[d].player.opponent())
                            {
                                Rules::is_square_attacked(board, &opp_king, stack[d].player)
                            } else {
                                false
                            };
                            let extension: usize = if gives_check { 1 } else { 0 };
                            stack[d].current_extension = extension;

                            let move_to_idx = board.coords_to_index(&tt_mv.to.values);
                            stack[d].pending_unmake = Some((tt_mv, info));
                            stack[d].phase = SearchPhase::TryTTMoveReturn;

                            let child_depth = stack[d].depth - 1 + extension;
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
                            child.ancestor_to_idx =
                                shift_ancestors(&stack[d].ancestor_to_idx, move_to_idx);
                            stack.push(child);
                            continue 'outer;
                        } else {
                            board.unmake_move(&tt_mv, info);
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

                // Multi-depth futility pruning (#5): depth-scaled margin
                // Margin = 77*depth (Stockfish-inspired, simplified for N-dim)
                if stack[d].depth <= 6 && !stack[d].in_check {
                    let futility_margin = 77 * stack[d].depth as i32;
                    if stack[d].static_eval + futility_margin < stack[d].alpha {
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
                let cm = if let Some(prev_to) = stack[d].ancestor_to_idx[0] {
                    let opp = stack[d].player.opponent() as usize;
                    if prev_to < countermoves[opp].len() {
                        countermoves[opp][prev_to].as_ref()
                    } else {
                        None
                    }
                } else {
                    None
                };

                let sort_ply = self.depth.saturating_sub(stack[d].depth);
                let sort_pawn_hash = Self::pawn_hash(board);
                self.sort_moves(
                    board,
                    &mut moves,
                    stack[d].tt_move,
                    my_killers,
                    history,
                    cm,
                    cont_history,
                    capture_history,
                    &stack[d].ancestor_to_idx,
                    stack[d].player,
                    pawn_history,
                    sort_pawn_hash,
                    low_ply_history,
                    sort_ply,
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
                                    capture_history,
                                    correction_history,
                                    pawn_history,
                                    low_ply_history,
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
                if stack[d].depth >= 6
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

                    let mut se_child = SearchFrame::new(
                        se_depth,
                        se_alpha,
                        se_beta,
                        stack[d].player,
                        false, // no null move in verification
                    );
                    se_child.ancestor_to_idx = stack[d].ancestor_to_idx;
                    stack.push(se_child);
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

                    // Skip TT move if already tried in staged phase (#8)
                    if stack[d].tt_move_tried
                        && let Some(tm) = stack[d].tt_move
                    {
                        let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(usize::MAX);
                        let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(usize::MAX);
                        if tm.from_idx as usize == from_idx && tm.to_idx as usize == to_idx {
                            continue;
                        }
                    }

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

                    // LMR reduction (#3): log-based table with dynamic adjustments
                    let mut reduction: usize = 0;
                    let is_pv_node = stack[d].beta > stack[d].alpha.saturating_add(1);
                    if legal_idx >= 2
                        && stack[d].depth >= 3
                        && !is_capture
                        && !is_promotion
                        && !stack[d].in_check
                    {
                        let d_idx = stack[d].depth.min(LMR_TABLE_SIZE - 1);
                        let m_idx = legal_idx.min(LMR_TABLE_SIZE - 1);
                        let mut r = self.lmr_table[d_idx][m_idx];

                        // Dynamic adjustments:
                        // Reduce less for PV nodes
                        if is_pv_node {
                            r -= 1;
                        }
                        // Reduce less if giving check
                        if gives_check {
                            r -= 1;
                        }
                        // Reduce more at non-PV cut nodes
                        if !is_pv_node {
                            r += 1;
                        }
                        // Adjust by history score (good history = less reduction)
                        let from_idx_h = board.coords_to_index(&mv.from.values).unwrap_or(0);
                        let to_idx_h = board.coords_to_index(&mv.to.values).unwrap_or(0);
                        let hist_idx = from_idx_h * board.total_cells() + to_idx_h;
                        if hist_idx < history[stack[d].player as usize].len() {
                            let h = history[stack[d].player as usize][hist_idx];
                            r -= (h / 2048).clamp(-2, 2);
                        }

                        reduction = r.max(0) as usize;
                        // Don't reduce into negative depth
                        if stack[d].depth - 1 + extension < reduction {
                            reduction = (stack[d].depth - 1 + extension).saturating_sub(1);
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
                        child.ancestor_to_idx =
                            shift_ancestors(&stack[d].ancestor_to_idx, move_to_idx);
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
                        child.ancestor_to_idx =
                            shift_ancestors(&stack[d].ancestor_to_idx, move_to_idx);
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

                // Update correction history (#18) before storing TT
                Self::update_correction(
                    board,
                    stack[d].static_eval,
                    stack[d].best_score,
                    stack[d].depth,
                    correction_history,
                );
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
    /// Uses Stockfish-style history gravity (#29) and penalties for non-cutoff moves (#14).
    #[allow(clippy::too_many_arguments)]
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
        capture_history: &mut [Vec<i32>],
        pawn_history: &mut [Vec<i32>],
        low_ply_history: &mut [Vec<i32>],
        ply: usize,
    ) {
        let is_capture = frame
            .pending_unmake
            .as_ref()
            .map(|(_, info)| info.captured.is_some())
            .unwrap_or(false);
        let is_promotion = mv.promotion.is_some();
        let is_quiet = !is_capture && !is_promotion;

        if score > frame.best_score {
            frame.best_score = score;
            frame.best_move_obj = Some(mv.clone());
        }

        if score > frame.alpha {
            frame.alpha = score;
        }

        // Track searched quiet moves for fail-low penalty (#14)
        if is_quiet {
            let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(0);
            let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);
            frame.searched_quiets.push((from_idx, to_idx));
        }

        if frame.alpha >= frame.beta && is_quiet {
            let bonus = ((121 * frame.depth as i32 - 75).min(932)).max(0);
            let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(0);
            let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);
            let total = board.total_cells();

            // Bonus for the cutoff move (history gravity #29)
            let hist_idx = from_idx * total + to_idx;
            if hist_idx < history[frame.player as usize].len() {
                update_history(&mut history[frame.player as usize][hist_idx], bonus);
            }

            // Multi-ply continuation history bonus (#11)
            for ply_back in 0..CONT_HIST_PLIES {
                if let Some(anc_to) = frame.ancestor_to_idx[ply_back] {
                    let cont_idx = anc_to * total + to_idx;
                    let table_idx = ply_back * 2 + frame.player as usize;
                    if table_idx < cont_history.len() && cont_idx < cont_history[table_idx].len() {
                        update_history(&mut cont_history[table_idx][cont_idx], bonus);
                    }
                }
            }

            // Pawn history bonus (#12)
            let pawn_hash = Self::pawn_hash(board);
            let ph_idx = (pawn_hash as usize & (PAWN_HIST_SIZE - 1)) * total + to_idx;
            if ph_idx < pawn_history[frame.player as usize].len() {
                update_history(&mut pawn_history[frame.player as usize][ph_idx], bonus);
            }

            // Low-ply history bonus (#13)
            if ply < LOW_PLY_MAX {
                let lp_table = ply * 2 + frame.player as usize;
                if lp_table < low_ply_history.len() && hist_idx < low_ply_history[lp_table].len() {
                    update_history(&mut low_ply_history[lp_table][hist_idx], bonus);
                }
            }

            // Penalty for all previously searched quiets that didn't cause cutoff (#14)
            let penalty = -bonus;
            for &(f, t) in &frame.searched_quiets {
                if f == from_idx && t == to_idx {
                    continue;
                }
                let idx = f * total + t;
                if idx < history[frame.player as usize].len() {
                    update_history(&mut history[frame.player as usize][idx], penalty);
                }
                // Multi-ply cont history penalty
                for ply_back in 0..CONT_HIST_PLIES {
                    if let Some(anc_to) = frame.ancestor_to_idx[ply_back] {
                        let cidx = anc_to * total + t;
                        let ti = ply_back * 2 + frame.player as usize;
                        if ti < cont_history.len() && cidx < cont_history[ti].len() {
                            update_history(&mut cont_history[ti][cidx], penalty);
                        }
                    }
                }
            }

            // Killer moves
            if frame.depth < killers.len() {
                killers[frame.depth][1] = killers[frame.depth][0].clone();
                killers[frame.depth][0] = Some(mv.clone());
            }

            // Countermove
            if let Some(prev_to) = frame.ancestor_to_idx[0] {
                let opp = frame.player.opponent() as usize;
                if prev_to < countermoves[opp].len() {
                    countermoves[opp][prev_to] = Some(mv.clone());
                }
            }
        }

        // Capture history update (#10): bonus for captures that cause cutoff
        if frame.alpha >= frame.beta && is_capture {
            let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);
            let cap_type = piece_type_index(board, to_idx);
            let cap_hist_idx = to_idx * 6 + cap_type;
            let bonus = ((121 * frame.depth as i32 - 75).min(932)).max(0);
            if cap_hist_idx < capture_history[frame.player as usize].len() {
                update_history(
                    &mut capture_history[frame.player as usize][cap_hist_idx],
                    bonus,
                );
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
    TryTTMoveReturn,
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
    /// The to-index of the move that led to this node (from parent), up to CONT_HIST_PLIES ancestors.
    /// ancestor_to_idx[0] = parent's move-to, [1] = grandparent's, etc.
    ancestor_to_idx: [Option<usize>; CONT_HIST_PLIES],
    /// Singular extension fields
    singular_beta: i32,
    singular_tt_move: Option<PackedMove>,
    singular_extension: usize,
    /// Searched quiet moves (from_idx, to_idx) for fail-low penalty (#14)
    searched_quiets: Vec<(usize, usize)>,
    /// Cached static eval for this node
    static_eval: i32,
    /// Null-move verification: minimum ply before allowing another null move (#4)
    nmp_min_ply: usize,
    /// Whether the TT move was already tried in the staged phase (#8)
    tt_move_tried: bool,
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
            ancestor_to_idx: [None; CONT_HIST_PLIES],
            singular_beta: 0,
            singular_tt_move: None,
            singular_extension: 0,
            searched_quiets: Vec::new(),
            static_eval: 0,
            nmp_min_ply: 0,
            tt_move_tried: false,
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
                let mut cont_history = vec![vec![0i32; hist_size]; CONT_HIST_PLIES * 2];
                let cap_hist_size = total_cells * 6;
                let mut capture_history =
                    vec![vec![0i32; cap_hist_size], vec![0i32; cap_hist_size]];
                let mut correction_history = CorrectionHistory::new();
                let pawn_hist_entry_size = PAWN_HIST_SIZE * total_cells;
                let mut pawn_history = vec![
                    vec![0i32; pawn_hist_entry_size],
                    vec![0i32; pawn_hist_entry_size],
                ];
                let mut low_ply_history = vec![vec![0i32; hist_size]; LOW_PLY_MAX * 2];

                // Adaptive time management (#24, #25)
                let mut best_move_stable_count: usize = 0;
                let mut prev_best_move: Option<Move> = None;
                let mut prev_iter_score: i32 = 0;
                let base_time = self.time_limit;

                // Depth staggering (#27): helper threads start at higher depths
                // to diversify TT population. Thread 0 starts at 1, thread 1 at 2, etc.
                let start_depth = if thread_idx == 0 {
                    1
                } else {
                    1 + (thread_idx % 3)
                };
                for d in start_depth..=self.depth {
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
                                &mut capture_history,
                                &mut correction_history,
                                &mut pawn_history,
                                &mut low_ply_history,
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
                                delta += delta / 3;
                                continue;
                            }
                            if failed_high {
                                beta += delta;
                                delta += delta / 3;
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

                    // Adaptive time: check if we should stop early (#24, #25)
                    if d >= 5 && thread_idx == 0 {
                        // Best move stability (#25)
                        if local_best_move == prev_best_move {
                            best_move_stable_count += 1;
                        } else {
                            best_move_stable_count = 0;
                        }
                        prev_best_move = local_best_move.clone();

                        // Falling eval factor (#24): if eval is dropping, use more time
                        let eval_drop = (prev_iter_score - local_best_score).max(0);
                        prev_iter_score = local_best_score;
                        let falling_eval_factor = if eval_drop > 50 {
                            1.5_f64 // eval falling significantly: 50% more time
                        } else if eval_drop > 20 {
                            1.2
                        } else {
                            1.0
                        };

                        // Stability factor: stable best move → reduce time
                        let stability_factor = match best_move_stable_count {
                            0..=1 => 1.2, // unstable: more time
                            2..=3 => 1.0, // normal
                            4..=6 => 0.7, // stable: less time
                            _ => 0.5,     // very stable: much less time
                        };

                        let adjusted_time =
                            base_time.as_secs_f64() * falling_eval_factor * stability_factor;
                        if start_time.elapsed().as_secs_f64() > adjusted_time * 0.6 {
                            // Used 60% of adjusted time — stop iterating
                            break;
                        }
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
