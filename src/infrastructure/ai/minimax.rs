use super::mcts::MCTS;
use crate::config::{MctsConfig, MinimaxConfig};
use crate::domain::board::Board;
use crate::domain::models::{Move, PieceType, Player};
use crate::domain::rules::Rules;
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT, PackedMove};
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CHECKMATE_SCORE: i32 = 30000;
const TIMEOUT_CHECK_INTERVAL: usize = 2048;

const VAL_PAWN: i32 = 100;
const VAL_KNIGHT: i32 = 320;
const VAL_BISHOP: i32 = 330;
const VAL_ROOK: i32 = 500;
const VAL_QUEEN: i32 = 900;
const VAL_KING: i32 = 20000;

pub struct MinimaxBot {
    depth: usize,
    time_limit: Duration,
    tt: Arc<LockFreeTT>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: std::sync::atomic::AtomicUsize,
    mcts_config: Option<MctsConfig>,
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
            nodes_searched: std::sync::atomic::AtomicUsize::new(0),
            mcts_config: None,
            num_threads,
        }
    }

    pub fn with_mcts(mut self, config: Option<MctsConfig>) -> Self {
        self.mcts_config = config;
        self
    }

    fn evaluate(&self, board: &Board, player_at_leaf: Option<Player>) -> i32 {
        if let Some(mcts_config) = &self.mcts_config {
            if let Some(player) = player_at_leaf {
                let mut mcts =
                    MCTS::new(board, player, None, Some(mcts_config.clone())).with_serial();
                let win_rate = mcts.run(board, mcts_config.iterations);

                let val_f = (win_rate - 0.5) * 2.0 * (VAL_KING as f64);
                let val = val_f as i32;

                return if player == Player::Black { -val } else { val };
            }
        }

        let mut score = 0;
        for i in board.white_occupancy.iter_indices() {
            score += self.get_piece_value(board, i);
        }
        for i in board.black_occupancy.iter_indices() {
            score -= self.get_piece_value(board, i);
        }
        score
    }

    fn get_piece_value(&self, board: &Board, idx: usize) -> i32 {
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

    fn q_search(&self, board: &mut Board, mut alpha: i32, beta: i32, player: Player) -> i32 {
        if self.nodes_searched.fetch_add(1, Ordering::Relaxed) % TIMEOUT_CHECK_INTERVAL == 0 {
            if self.stop_flag.load(Ordering::Relaxed) {
                return 0;
            }
        }

        let stand_pat = match player {
            Player::White => self.evaluate(board, Some(Player::White)),
            Player::Black => -self.evaluate(board, Some(Player::Black)),
        };

        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        let moves = Rules::generate_loud_moves(board, player);

        for mv in moves {
            let info = match board.apply_move(&mv) {
                Ok(i) => i,
                Err(_) => continue,
            };

            let score = -self.q_search(board, -beta, -alpha, player.opponent());

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

    fn sort_moves(
        &self,
        board: &Board,
        moves: &mut [Move],
        tt_move: Option<PackedMove>,
        killers: Option<&[Option<Move>; 2]>,
        player: Player,
    ) {
        moves.sort_by_cached_key(|mv| {
            let mut score = 0;
            let from_idx = board.coords_to_index(&mv.from.values);
            let to_idx = board.coords_to_index(&mv.to.values);

            if let (Some(f), Some(t)) = (from_idx, to_idx) {
                if let Some(tm) = tt_move {
                    if tm.from_idx as usize == f && tm.to_idx as usize == t {
                        // Hash move - highest priority
                        return -2_000_000_000;
                    }
                }

                if let Some(ks) = killers {
                    if let Some(k) = &ks[0] {
                        if k == mv {
                            return -1_900_000_000;
                        }
                    }
                    if let Some(k) = &ks[1] {
                        if k == mv {
                            return -1_800_000_000;
                        }
                    }
                }

                let enemy_occupancy = match player {
                    Player::White => &board.black_occupancy,
                    Player::Black => &board.white_occupancy,
                };

                if enemy_occupancy.get_bit(t) {
                    let victim_val = self.get_piece_value(board, t);
                    let attacker_val = self.get_piece_value(board, f);
                    score = 1000 + 10 * victim_val - attacker_val;
                }

                if let Some(p) = mv.promotion {
                    let val = match p {
                        PieceType::Queen => VAL_QUEEN,
                        PieceType::Rook => VAL_ROOK,
                        PieceType::Bishop => VAL_BISHOP,
                        PieceType::Knight => VAL_KNIGHT,
                        _ => 0,
                    };
                    score += val + 500;
                }
            }

            -score
        });
    }

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

        // TT Read
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

        // Null Move Pruning
        // Only if depth >= 3, allow_null, and static eval suggests position seems good.
        if allow_null && depth >= 3 {
            let static_eval = self.evaluate(board, None);
            if static_eval >= beta {
                // Only do expensive check check if static eval passed
                let in_check = if let Some(king_pos) = board.get_king_coordinate(player) {
                    Rules::is_square_attacked(board, &king_pos, player.opponent())
                } else {
                    false
                };

                if !in_check {
                    let r = if depth > 6 { 3 } else { 2 };

                    // Make null move
                    let null_info = board.make_null_move(player);

                    let score = -self.minimax(
                        board,
                        depth - 1 - r,
                        -beta,
                        -beta + 1, // Null window around beta
                        player.opponent(),
                        start_time,
                        false, // Disable null move in recursive call
                        killers,
                    );

                    board.unmake_null_move(null_info);

                    if score >= beta {
                        return beta; // Cutoff
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

        // Futility Pruning Pre-check
        let mut do_futility = false;
        if depth == 1 {
            let eval = self.evaluate(board, None);
            if eval + 500 < alpha {
                do_futility = true;
            }
        }

        let my_killers = if depth < killers.len() {
            Some(&killers[depth])
        } else {
            None
        };
        self.sort_moves(board, &mut moves, tt_move, my_killers, player);

        let mut best_score = -i32::MAX;
        let original_alpha = alpha;
        let mut best_move: Option<Move> = None;
        let mut in_check_cache: Option<bool> = None;

        for (i, mv) in moves.iter().enumerate() {
            let info = match board.apply_move(mv) {
                Ok(i) => i,
                Err(_) => continue,
            };

            let is_capture = info.captured.is_some();
            let is_promotion = mv.promotion.is_some();

            // Futility Pruning check
            if do_futility && !is_capture && !is_promotion {
                if in_check_cache.is_none() {
                    in_check_cache =
                        Some(if let Some(king_pos) = board.get_king_coordinate(player) {
                            Rules::is_square_attacked(board, &king_pos, player.opponent())
                        } else {
                            false
                        });
                }
                if !in_check_cache.unwrap() {
                    board.unmake_move(mv, info);
                    continue;
                }
            }

            let mut score;
            let mut reduction = 0;

            // Late Move Reduction (LMR)
            if i >= 4 && depth >= 3 {
                // Check if move is quiet (no capture, no promotion)

                if !is_capture && !is_promotion {
                    if in_check_cache.is_none() {
                        in_check_cache =
                            Some(if let Some(king_pos) = board.get_king_coordinate(player) {
                                Rules::is_square_attacked(board, &king_pos, player.opponent())
                            } else {
                                false
                            });
                    }

                    if !in_check_cache.unwrap() {
                        reduction = if depth > 6 { 2 } else { 1 };
                        // Don't reduce below depth 1
                        if depth - 1 < reduction {
                            reduction = 0;
                        }
                    }
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
                );
            } else {
                // PVS: Null window search with LMR
                score = -self.minimax(
                    board,
                    depth - 1 - reduction,
                    -alpha - 1,
                    -alpha,
                    player.opponent(),
                    start_time,
                    true,
                    killers,
                );

                if score > alpha && reduction > 0 {
                    // LMR failed, re-search with full depth (still null window)
                    score = -self.minimax(
                        board,
                        depth - 1,
                        -alpha - 1,
                        -alpha,
                        player.opponent(),
                        start_time,
                        true,
                        killers,
                    );
                }

                if score > alpha && score < beta {
                    // Fail high, re-search with full window
                    score = -self.minimax(
                        board,
                        depth - 1,
                        -beta,
                        -alpha,
                        player.opponent(),
                        start_time,
                        true,
                        killers,
                    );
                }
            }

            board.unmake_move(mv, info);

            if self.stop_flag.load(Ordering::Relaxed) {
                return 0;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(mv.clone());
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                if !is_capture && !is_promotion && depth < killers.len() {
                    let slot1 = killers[depth][0].clone();
                    killers[depth][1] = slot1;
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

        let packed_move = best_move.and_then(|m| {
            let from = board.coords_to_index(&m.from.values)?;
            let to = board.coords_to_index(&m.to.values)?;
            let promo = match m.promotion {
                None => 0,
                Some(PieceType::Queen) => 1,
                Some(PieceType::Rook) => 2,
                Some(PieceType::Bishop) => 3,
                Some(PieceType::Knight) => 4,
                _ => 0,
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

                for d in 1..=self.depth {
                    let mut delta = 50;
                    let mut alpha;
                    let mut beta;

                    if d > 1 {
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

                        for mv in &my_moves {
                            let info = local_board.apply_move(mv).unwrap();

                            let score = -self.minimax(
                                &mut local_board,
                                d - 1,
                                -beta, // Pass full beta? No, use window.
                                -alpha_inner,
                                player.opponent(),
                                start_time,
                                true,
                                &mut killers,
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
                                break; // Fail high at root
                            }
                        }

                        if self.stop_flag.load(Ordering::Relaxed) {
                            local_best_score = best_score_this_iter; // Partial result
                            break;
                        }

                        // Check aspiration results
                        if d > 1 {
                            if best_score_this_iter <= alpha {
                                // Fail low
                                beta = (alpha + beta) / 2;
                                alpha -= delta;
                                delta += delta / 2;
                                continue;
                            }
                            if failed_high {
                                // Fail high
                                beta += delta;
                                delta += delta / 2;
                                continue;
                            }
                        }

                        local_best_score = best_score_this_iter;
                        local_best_move = best_move_this_iter;
                        prev_score = local_best_score;
                        break; // Window OK
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

        let best = results.into_iter().max_by_key(|r| r.1);

        best.map(|(m, _)| m)
    }
}
