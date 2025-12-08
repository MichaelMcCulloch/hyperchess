use crate::domain::models::{BoardState, Player};
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::persistence::{BitBoard, BitBoardState, WinningMasks};
use crate::infrastructure::symmetries::SymmetryHandler;
use rayon::prelude::*;
use std::sync::Arc;

pub mod transposition;
use transposition::{Flag, LockFreeTT};

use std::sync::atomic::{AtomicUsize, Ordering};

pub struct MinimaxBot {
    transposition_table: Arc<LockFreeTT>,
    zobrist_keys: Vec<[u64; 2]>,
    symmetries: Option<SymmetryHandler>,
    max_depth: usize,
    strategic_values: Vec<usize>,
    killer_moves: Vec<[AtomicUsize; 2]>,
    sorted_indices: Vec<usize>,
}

impl MinimaxBot {
    pub fn new(max_depth: usize) -> Self {
        let killer_storage_depth = std::cmp::min(max_depth, 64);
        let mut killer_moves = Vec::with_capacity(killer_storage_depth + 1);
        for _ in 0..=killer_storage_depth {
            killer_moves.push([AtomicUsize::new(usize::MAX), AtomicUsize::new(usize::MAX)]);
        }

        MinimaxBot {
            transposition_table: Arc::new(LockFreeTT::new(64)),
            zobrist_keys: Vec::new(),
            symmetries: None,
            max_depth,
            strategic_values: Vec::new(),
            killer_moves,
            sorted_indices: Vec::new(),
        }
    }

    // ... [Existing store_killer, ensure_initialized, get_strategic_value methods are fine] ...
    fn store_killer(&self, depth: usize, move_idx: usize) {
        if depth >= self.killer_moves.len() {
            return;
        }
        let k0 = self.killer_moves[depth][0].load(Ordering::Relaxed);
        if k0 != move_idx {
            self.killer_moves[depth][1].store(k0, Ordering::Relaxed);
            self.killer_moves[depth][0].store(move_idx, Ordering::Relaxed);
        }
    }

    fn ensure_initialized(&mut self, board: &BitBoardState) {
        if self.zobrist_keys.len() < board.total_cells() {
            let mut rng_state: u64 = 0xDEADBEEF + board.total_cells() as u64;
            let mut next_rand = || -> u64 {
                rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
                rng_state
            };
            self.zobrist_keys
                .resize_with(board.total_cells(), || [next_rand(), next_rand()]);
        }
        if self.symmetries.is_none() {
            self.symmetries = Some(SymmetryHandler::new(board.dimension, board.side));
        }
        if self.strategic_values.len() < board.total_cells() {
            self.strategic_values = (0..board.total_cells())
                .map(|i| self.get_strategic_value(board, i))
                .collect();
            self.sorted_indices = (0..board.total_cells()).collect();
            self.sorted_indices
                .sort_by(|&a, &b| self.strategic_values[b].cmp(&self.strategic_values[a]));
        }
    }

    fn get_strategic_value(&self, board: &BitBoardState, index: usize) -> usize {
        match &*board.winning_masks {
            WinningMasks::Small { map_offsets, .. }
            | WinningMasks::Medium { map_offsets, .. }
            | WinningMasks::Large { map_offsets, .. } => map_offsets
                .get(index)
                .map_or(0, |&(_, count)| count as usize),
        }
    }

    fn calculate_zobrist_hash(&self, board: &BitBoardState) -> u64 {
        let mut h = 0;
        for cell_idx in 0..board.total_cells() {
            if let Some(p) = board.get_cell_index(cell_idx) {
                let p_idx = match p {
                    Player::X => 0,
                    Player::O => 1,
                };
                h ^= self.zobrist_keys[cell_idx][p_idx];
            }
        }
        h
    }

    fn get_sorted_moves_into(
        &self,
        board: &BitBoardState,
        buffer: &mut Vec<usize>,
        best_move_hint: Option<usize>,
        depth: usize,
        _current_player: Player,
    ) {
        buffer.clear();

        let (k0, k1) = if depth < self.killer_moves.len() {
            (
                self.killer_moves[depth][0].load(Ordering::Relaxed),
                self.killer_moves[depth][1].load(Ordering::Relaxed),
            )
        } else {
            (usize::MAX, usize::MAX)
        };

        for &idx in &self.sorted_indices {
            if board.get_cell_index(idx).is_none() {
                buffer.push(idx);
            }
        }

        if buffer.is_empty() {
            return;
        }

        let bring_to_front = |slice: &mut [usize], target: usize, target_index: usize| {
            if target_index >= slice.len() {
                return;
            }
            if let Some(pos) = slice[target_index..].iter().position(|&m| m == target) {
                slice.swap(target_index, target_index + pos);
            }
        };

        if let Some(tt_move) = best_move_hint {
            bring_to_front(buffer, tt_move, 0);
        }
        if k0 != usize::MAX && buffer.get(0) != Some(&k0) {
            bring_to_front(buffer, k0, 1);
        }
        if k1 != usize::MAX && buffer.get(0) != Some(&k1) && buffer.get(1) != Some(&k1) {
            bring_to_front(buffer, k1, 2);
        }
    }

    #[inline]
    fn get_line_score(x: u32, o: u32) -> i32 {
        if o == 0 {
            if x == 2 {
                return 10;
            }
            if x == 1 {
                return 1;
            }
        } else if x == 0 {
            if o == 2 {
                return -10;
            }
            if o == 1 {
                return -1;
            }
        }
        0
    }

    fn calculate_score_delta(
        &self,
        board: &BitBoardState,
        index: usize,
        player: Player,
    ) -> (i32, bool) {
        let mut delta = 0;
        let mut is_win = false;
        let side = board.side as u32;

        match &*board.winning_masks {
            WinningMasks::Small {
                cell_mask_lookup, ..
            } => {
                let p1 = match &board.p1 {
                    BitBoard::Small(b) => *b,
                    _ => 0,
                };
                let p2 = match &board.p2 {
                    BitBoard::Small(b) => *b,
                    _ => 0,
                };
                if let Some(masks) = cell_mask_lookup.get(index) {
                    for &m in masks {
                        let x = (p1 & m).count_ones();
                        let o = (p2 & m).count_ones();
                        let old_score = Self::get_line_score(x, o);
                        let (nx, no) = match player {
                            Player::X => (x + 1, o),
                            Player::O => (x, o + 1),
                        };
                        if match player {
                            Player::X => nx == side,
                            Player::O => no == side,
                        } {
                            is_win = true;
                        }
                        delta += Self::get_line_score(nx, no) - old_score;
                    }
                }
            }
            WinningMasks::Medium {
                cell_mask_lookup, ..
            } => {
                let p1 = match &board.p1 {
                    BitBoard::Medium(b) => *b,
                    _ => 0,
                };
                let p2 = match &board.p2 {
                    BitBoard::Medium(b) => *b,
                    _ => 0,
                };
                if let Some(masks) = cell_mask_lookup.get(index) {
                    for &m in masks {
                        let x = (p1 & m).count_ones();
                        let o = (p2 & m).count_ones();
                        let old_score = Self::get_line_score(x, o);
                        let (nx, no) = match player {
                            Player::X => (x + 1, o),
                            Player::O => (x, o + 1),
                        };
                        if match player {
                            Player::X => nx == side,
                            Player::O => no == side,
                        } {
                            is_win = true;
                        }
                        delta += Self::get_line_score(nx, no) - old_score;
                    }
                }
            }
            WinningMasks::Large {
                masks,
                map_flat,
                map_offsets,
            } => {
                if index < map_offsets.len() {
                    let (start, count) = map_offsets[index];
                    let range = start as usize..(start + count) as usize;

                    // Match on references to avoid move
                    match (&board.p1, &board.p2) {
                        (BitBoard::Large { data: v1 }, BitBoard::Large { data: v2 }) => {
                            for &i in &map_flat[range] {
                                let mask_chunks = &masks[i];
                                let mut x = 0;
                                let mut o = 0;
                                for (k, m) in mask_chunks.iter().enumerate() {
                                    if let Some(chunk1) = v1.get(k) {
                                        x += (chunk1 & m).count_ones();
                                    }
                                    if let Some(chunk2) = v2.get(k) {
                                        o += (chunk2 & m).count_ones();
                                    }
                                }
                                let old_score = Self::get_line_score(x, o);
                                let (nx, no) = match player {
                                    Player::X => (x + 1, o),
                                    Player::O => (x, o + 1),
                                };
                                if match player {
                                    Player::X => nx == side,
                                    Player::O => no == side,
                                } {
                                    is_win = true;
                                }
                                delta += Self::get_line_score(nx, no) - old_score;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        (delta, is_win)
    }

    fn minimax(
        &self,
        board: &mut BitBoardState,
        depth: usize,
        current_player: Player,
        mut alpha: i32,
        mut beta: i32,
        current_hash: u64,
        current_score: i32,
    ) -> i32 {
        let alpha_orig = alpha;
        let remaining_depth = if self.max_depth > depth {
            (self.max_depth - depth) as u8
        } else {
            0
        };
        let mut best_move_hint = None;

        if let Some((score, entry_depth, flag, best_move)) =
            self.transposition_table.get(current_hash)
        {
            if entry_depth >= remaining_depth {
                match flag {
                    Flag::Exact => return score,
                    Flag::LowerBound => alpha = alpha.max(score),
                    Flag::UpperBound => beta = beta.min(score),
                }
                if alpha >= beta {
                    return score;
                }
            }
            best_move_hint = best_move.map(|idx| idx as usize);
        }

        if board.is_full() {
            return 0;
        }
        if depth >= self.max_depth {
            return current_score;
        }

        let opponent = current_player.opponent();

        // Use Heap (Vec) instead of Stack Array to support arbitrary sizes.
        let mut moves = Vec::with_capacity(board.total_cells());
        self.get_sorted_moves_into(board, &mut moves, best_move_hint, depth, current_player);

        let mut best_val = match current_player {
            Player::X => i32::MIN,
            Player::O => i32::MAX,
        };
        let mut best_move_idx = best_move_hint;
        let p_idx = match current_player {
            Player::X => 0,
            Player::O => 1,
        };

        for &idx in &moves {
            let (score_delta, is_win) = self.calculate_score_delta(board, idx, current_player);
            let next_score = current_score + score_delta;

            board.set_cell_index(idx, current_player).unwrap();
            let new_hash = current_hash ^ self.zobrist_keys[idx][p_idx];
            let win = is_win;

            let val = if win {
                match current_player {
                    Player::X => 1000 - depth as i32,
                    Player::O => -1000 + depth as i32,
                }
            } else {
                self.minimax(
                    board,
                    depth + 1,
                    opponent,
                    alpha,
                    beta,
                    new_hash,
                    next_score,
                )
            };

            board.clear_cell_index(idx);

            match current_player {
                Player::X => {
                    if val > best_val {
                        best_val = val;
                        best_move_idx = Some(idx);
                    }
                    alpha = alpha.max(val);
                    if val > 900 {
                        break;
                    }
                }
                Player::O => {
                    if val < best_val {
                        best_val = val;
                        best_move_idx = Some(idx);
                    }
                    beta = beta.min(val);
                    if val < -900 {
                        break;
                    }
                }
            }
            if beta <= alpha {
                self.store_killer(depth, idx);
                break;
            }
        }

        if (current_player == Player::X && best_val == i32::MIN)
            || (current_player == Player::O && best_val == i32::MAX)
        {
            best_val = 0;
        }

        let flag = if best_val <= alpha_orig {
            Flag::UpperBound
        } else if best_val >= beta {
            Flag::LowerBound
        } else {
            Flag::Exact
        };

        let best_move_u16 = best_move_idx.map(|i| i as u16);
        self.transposition_table.store(
            current_hash,
            best_val,
            remaining_depth,
            flag,
            best_move_u16,
        );
        best_val
    }

    fn evaluate(&self, board: &BitBoardState) -> i32 {
        let mut score = 0;
        match &*board.winning_masks {
            WinningMasks::Small { masks, .. } => {
                let p1 = match &board.p1 {
                    BitBoard::Small(b) => *b,
                    _ => 0,
                };
                let p2 = match &board.p2 {
                    BitBoard::Small(b) => *b,
                    _ => 0,
                };
                for &mask in masks {
                    let x = (p1 & mask).count_ones();
                    let o = (p2 & mask).count_ones();
                    score += Self::get_line_score(x, o);
                }
            }
            WinningMasks::Medium { masks, .. } => {
                let p1 = match &board.p1 {
                    BitBoard::Medium(b) => *b,
                    _ => 0,
                };
                let p2 = match &board.p2 {
                    BitBoard::Medium(b) => *b,
                    _ => 0,
                };
                for &mask in masks {
                    let x = (p1 & mask).count_ones();
                    let o = (p2 & mask).count_ones();
                    score += Self::get_line_score(x, o);
                }
            }
            _ => {} // Large board evaluation logic implicit in minimax deltas for now
        }
        score
    }
}

use crate::domain::coordinate::Coordinate;
use crate::infrastructure::persistence::index_to_coords;

impl PlayerStrategy<BitBoardState> for MinimaxBot {
    fn get_best_move(&mut self, board: &BitBoardState, player: Player) -> Option<Coordinate> {
        self.ensure_initialized(board);

        let mut best_move = None;
        let time_limit = std::time::Duration::from_millis(1000);
        let start_time = std::time::Instant::now();
        let global_max_depth = self.max_depth;

        for d in 1..=global_max_depth {
            self.max_depth = d;

            let root_hash = self.calculate_zobrist_hash(board);
            let best_move_hint =
                if let Some((_, _, _, mv)) = self.transposition_table.get(root_hash) {
                    mv.map(|m| m as usize)
                } else {
                    best_move
                };

            let mut available_moves = Vec::with_capacity(board.total_cells());
            self.get_sorted_moves_into(board, &mut available_moves, best_move_hint, 0, player);

            if available_moves.is_empty() {
                self.max_depth = global_max_depth;
                return None;
            }

            let first_move = available_moves[0];
            let mut work_board = board.clone();

            let initial_hash = self.calculate_zobrist_hash(&work_board);
            let initial_score = self.evaluate(board);
            let p_idx = match player {
                Player::X => 0,
                Player::O => 1,
            };

            let (first_delta, first_is_win) = self.calculate_score_delta(board, first_move, player);
            let first_next_score = initial_score + first_delta;

            work_board.set_cell_index(first_move, player).unwrap();
            let next_hash = initial_hash ^ self.zobrist_keys[first_move][p_idx];

            let first_score = if first_is_win {
                match player {
                    Player::X => 1000,
                    Player::O => -1000,
                }
            } else {
                self.minimax(
                    &mut work_board,
                    0,
                    player.opponent(),
                    i32::MIN + 1,
                    i32::MAX - 1,
                    next_hash,
                    first_next_score,
                )
            };

            let (alpha, beta) = match player {
                Player::X => (first_score, i32::MAX - 1),
                Player::O => (i32::MIN + 1, first_score),
            };

            let mut current_best = first_move;
            let current_best_score = first_score;

            if available_moves.len() > 1 {
                let use_parallel = self.max_depth >= 4;
                let best_move_entry = if use_parallel {
                    available_moves[1..]
                        .par_iter()
                        .map(|&mv| {
                            let mut work_board = board.clone();
                            let (delta, is_win) = self.calculate_score_delta(board, mv, player);
                            let next_score = initial_score + delta;

                            work_board.set_cell_index(mv, player).unwrap();
                            let next_hash = initial_hash ^ self.zobrist_keys[mv][p_idx];

                            let score = if is_win {
                                match player {
                                    Player::X => 1000,
                                    Player::O => -1000,
                                }
                            } else {
                                self.minimax(
                                    &mut work_board,
                                    0,
                                    player.opponent(),
                                    alpha,
                                    beta,
                                    next_hash,
                                    next_score,
                                )
                            };
                            (mv, score)
                        })
                        .max_by(|a, b| match player {
                            Player::X => a.1.cmp(&b.1),
                            Player::O => b.1.cmp(&a.1),
                        })
                } else {
                    available_moves[1..]
                        .iter()
                        .map(|&mv| {
                            let mut work_board = board.clone();
                            let (delta, is_win) = self.calculate_score_delta(board, mv, player);
                            let next_score = initial_score + delta;
                            work_board.set_cell_index(mv, player).unwrap();
                            let next_hash = initial_hash ^ self.zobrist_keys[mv][p_idx];
                            let score = if is_win {
                                match player {
                                    Player::X => 1000,
                                    Player::O => -1000,
                                }
                            } else {
                                self.minimax(
                                    &mut work_board,
                                    0,
                                    player.opponent(),
                                    alpha,
                                    beta,
                                    next_hash,
                                    next_score,
                                )
                            };
                            (mv, score)
                        })
                        .max_by(|a, b| match player {
                            Player::X => a.1.cmp(&b.1),
                            Player::O => b.1.cmp(&a.1),
                        })
                };

                if let Some((best_parallel_move, best_parallel_score)) = best_move_entry {
                    match player {
                        Player::X => {
                            if best_parallel_score > current_best_score {
                                current_best = best_parallel_move;
                            }
                        }
                        Player::O => {
                            if best_parallel_score < current_best_score {
                                current_best = best_parallel_move;
                            }
                        }
                    }
                }
            }
            best_move = Some(current_best);
            if start_time.elapsed() > time_limit {
                break;
            }
        }
        self.max_depth = global_max_depth;
        best_move.map(|idx| Coordinate::new(index_to_coords(idx, board.dimension, board.side)))
    }
}
