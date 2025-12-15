use super::eval::Evaluator;
use crate::config::MinimaxConfig;
use crate::domain::board::Board;
use crate::domain::models::{Move, PieceType, Player};
use crate::domain::rules::Rules;
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

pub struct MinimaxBot {
    depth: usize,
    time_limit: Duration,
    tt: Arc<LockFreeTT>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: Arc<AtomicUsize>,
    num_threads: usize,
}

impl MinimaxBot {
    pub fn new(
        config: &MinimaxConfig,
        time_limit_ms: u64,
        _dimension: usize,
        _side: usize,
    ) -> Self {
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).max(1))
            .unwrap_or(1);

        Self {
            depth: config.depth,
            time_limit: Duration::from_millis(time_limit_ms),
            tt: Arc::new(LockFreeTT::new(256)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: Arc::new(AtomicUsize::new(0)),
            num_threads,
        }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.num_threads = concurrency;
        self
    }

    fn evaluate(&self, board: &Board, player_at_leaf: Option<Player>) -> i32 {
        let score = Evaluator::evaluate(board);

        if let Some(p) = player_at_leaf {
            if p == Player::Black {
                return -score;
            }
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
        player: Player,
    ) {
        moves.sort_by_cached_key(|mv| {
            let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(0);
            let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);

            if let Some(tm) = tt_move {
                if tm.from_idx as usize == from_idx && tm.to_idx as usize == to_idx {
                    return -2_000_000_000;
                }
            }

            let enemy_occupancy = match player {
                Player::White => &board.black_occupancy,
                Player::Black => &board.white_occupancy,
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
                if let Some(k) = &ks[0] {
                    if k == mv {
                        return -500_000;
                    }
                }
                if let Some(k) = &ks[1] {
                    if k == mv {
                        return -400_000;
                    }
                }
            }

            if from_idx < history[0].len() && to_idx < history[0].len() {
                let hist_idx = from_idx * board.total_cells() + to_idx;
                if hist_idx < history[0].len() {
                    let score = history[player as usize][hist_idx];
                    return -score;
                }
            }

            0
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn minimax(
        &self,
        board: &mut Board,
        depth: usize,
        mut alpha: i32,
        mut beta: i32,
        player: Player,
        start_time: Instant,
        allow_null: bool,
        killers: &mut [[Option<Move>; 2]],
        history: &mut [Vec<i32>],
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

        let mut tt_move = None;
        if let Some((tt_score, tt_depth, tt_flag, best_m)) = self.tt.get(hash) {
            tt_move = best_m;
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
            return self.q_search(board, alpha, beta, player);
        }

        if allow_null && depth >= 3 {
            let static_eval = self.evaluate(board, Some(player));
            if static_eval >= beta {
                let in_check = if let Some(king_pos) = board.get_king_coordinate(player) {
                    Rules::is_square_attacked(board, &king_pos, player.opponent())
                } else {
                    false
                };

                if !in_check {
                    let r = if depth > 6 { 3 } else { 2 };
                    let null_info = board.make_null_move();
                    let score = -self.minimax(
                        board,
                        depth - 1 - r,
                        -beta,
                        -beta + 1,
                        player.opponent(),
                        start_time,
                        false,
                        killers,
                        history,
                    );
                    board.unmake_null_move(null_info);
                    if score >= beta {
                        return beta;
                    }
                }
            }
        }

        let mut moves = Rules::generate_legal_moves(board, player);
        if moves.is_empty() {
            if let Some(king_pos) = board.get_king_coordinate(player) {
                if Rules::is_square_attacked(board, &king_pos, player.opponent()) {
                    return -CHECKMATE_SCORE + (self.depth - depth) as i32;
                }
            }
            return 0;
        }

        let mut do_futility = false;
        if depth == 1 {
            let eval = self.evaluate(board, Some(player));
            if eval + 500 < alpha {
                do_futility = true;
            }
        }

        let my_killers = if depth < killers.len() {
            Some(&killers[depth])
        } else {
            None
        };

        self.sort_moves(board, &mut moves, tt_move, my_killers, history, player);

        let mut best_score = -i32::MAX;
        let original_alpha = alpha;
        let mut best_move_obj: Option<Move> = None;

        let in_check = if let Some(king_pos) = board.get_king_coordinate(player) {
            Rules::is_square_attacked(board, &king_pos, player.opponent())
        } else {
            false
        };

        for (i, mv) in moves.iter().enumerate() {
            let info = match board.apply_move(mv) {
                Ok(i) => i,
                Err(_) => continue,
            };

            let is_capture = info.captured.is_some();
            let is_promotion = mv.promotion.is_some();

            if do_futility && !is_capture && !is_promotion && !in_check {
                board.unmake_move(mv, info);
                continue;
            }

            let mut score;
            let mut reduction = 0;

            if i >= 4 && depth >= 3 && !is_capture && !is_promotion && !in_check {
                reduction = if depth > 6 { 2 } else { 1 };
                if depth - 1 < reduction {
                    reduction = 0;
                }
            }

            if i == 0 {
                score = -self.minimax(
                    board,
                    depth - 1,
                    -beta,
                    -alpha,
                    player.opponent(),
                    start_time,
                    true,
                    killers,
                    history,
                );
            } else {
                score = -self.minimax(
                    board,
                    depth - 1 - reduction,
                    -alpha - 1,
                    -alpha,
                    player.opponent(),
                    start_time,
                    true,
                    killers,
                    history,
                );

                if score > alpha && reduction > 0 {
                    score = -self.minimax(
                        board,
                        depth - 1,
                        -alpha - 1,
                        -alpha,
                        player.opponent(),
                        start_time,
                        true,
                        killers,
                        history,
                    );
                }

                if score > alpha && score < beta {
                    score = -self.minimax(
                        board,
                        depth - 1,
                        -beta,
                        -alpha,
                        player.opponent(),
                        start_time,
                        true,
                        killers,
                        history,
                    );
                }
            }

            board.unmake_move(mv, info);

            if self.stop_flag.load(Ordering::Relaxed) {
                return 0;
            }

            if score > best_score {
                best_score = score;
                best_move_obj = Some(mv.clone());
            }

            if score > alpha {
                alpha = score;

                if !is_capture && !is_promotion {
                    let from_idx = board.coords_to_index(&mv.from.values).unwrap_or(0);
                    let to_idx = board.coords_to_index(&mv.to.values).unwrap_or(0);
                    let hist_idx = from_idx * board.total_cells() + to_idx;

                    if hist_idx < history[player as usize].len() {
                        let bonus = (depth * depth) as i32;
                        let current = history[player as usize][hist_idx];
                        if current + bonus < MAX_HISTORY {
                            history[player as usize][hist_idx] += bonus;
                        }
                    }
                }
            }

            if alpha >= beta {
                if !is_capture && !is_promotion && depth < killers.len() {
                    killers[depth][1] = killers[depth][0].clone();
                    killers[depth][0] = Some(mv.clone());
                }
                break;
            }
        }

        let flag = if best_score <= original_alpha {
            Flag::UpperBound
        } else if best_score >= beta {
            Flag::LowerBound
        } else {
            Flag::Exact
        };

        let packed_move = best_move_obj.and_then(|m| {
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

        self.tt
            .store(hash, best_score, depth as u8, flag, packed_move);
        best_score
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

                let hist_size = local_board.total_cells() * local_board.total_cells();
                let mut history = vec![vec![0; hist_size], vec![0; hist_size]];

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
