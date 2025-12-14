use super::mcts::MCTS;
use crate::config::{MctsConfig, MinimaxConfig};
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::domain::rules::Rules;
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

    fn minimax(
        &self,
        board: &mut Board,
        depth: usize,
        mut alpha: i32,
        mut beta: i32,
        player: Player,
        start_time: Instant,
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

        if let Some((tt_score, tt_depth, tt_flag, _)) = self.tt.get(hash) {
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
            return match player {
                Player::White => self.evaluate(board, Some(Player::White)),
                Player::Black => -self.evaluate(board, Some(Player::Black)),
            };
        }

        let moves = Rules::generate_legal_moves(board, player);
        if moves.is_empty() {
            if let Some(king_pos) = board.get_king_coordinate(player) {
                if Rules::is_square_attacked(board, &king_pos, player.opponent()) {
                    return -CHECKMATE_SCORE + (self.depth - depth) as i32;
                }
            }
            return 0;
        }

        let mut best_score = -i32::MAX;
        let original_alpha = alpha;

        for mv in moves {
            let info = match board.apply_move(&mv) {
                Ok(i) => i,
                Err(_) => continue,
            };

            let score = -self.minimax(
                board,
                depth - 1,
                -beta,
                -alpha,
                player.opponent(),
                start_time,
            );

            board.unmake_move(&mv, info);

            if self.stop_flag.load(Ordering::Relaxed) {
                return 0;
            }

            if score > best_score {
                best_score = score;
            }
            alpha = alpha.max(score);
            if alpha >= beta {
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
        self.tt.store(hash, best_score, depth as u8, flag, None);

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

                for d in 1..=self.depth {
                    let mut alpha = -i32::MAX;
                    let beta = i32::MAX;
                    let mut best_score_this_depth = -i32::MAX;
                    let mut best_move_this_depth = None;

                    for mv in &my_moves {
                        let info = local_board.apply_move(mv).unwrap();

                        let score = -self.minimax(
                            &mut local_board,
                            d - 1,
                            -beta,
                            -alpha,
                            player.opponent(),
                            start_time,
                        );

                        local_board.unmake_move(mv, info);

                        if self.stop_flag.load(Ordering::Relaxed) {
                            break;
                        }

                        if score > best_score_this_depth {
                            best_score_this_depth = score;
                            best_move_this_depth = Some(mv.clone());
                        }
                        alpha = alpha.max(score);
                    }

                    if !self.stop_flag.load(Ordering::Relaxed) {
                        local_best_score = best_score_this_depth;
                        local_best_move = best_move_this_depth;
                    } else {
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
