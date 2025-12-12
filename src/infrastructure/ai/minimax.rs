use super::mcts::MCTS;
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::domain::rules::Rules;
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CHECKMATE_SCORE: i32 = 30000;
const TIMEOUT_CHECK_INTERVAL: usize = 2048;

// Material values
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
    _randomized: bool,
    use_mcts: bool,
    mcts_iterations: usize,
}

impl MinimaxBot {
    pub fn new(depth: usize, time_limit_ms: u64, _dimension: usize, _side: usize) -> Self {
        Self {
            depth,
            time_limit: Duration::from_millis(time_limit_ms),
            tt: Arc::new(LockFreeTT::new(64)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: std::sync::atomic::AtomicUsize::new(0),
            _randomized: true,
            use_mcts: false,      // Default off
            mcts_iterations: 100, // Default 100
        }
    }

    pub fn with_mcts(mut self, iterations: usize) -> Self {
        self.use_mcts = true;
        self.mcts_iterations = iterations;
        Self {
            depth: if self.use_mcts { 2 } else { self.depth }, // Reduce depth if MCTS is on to compensate?
            ..self
        }
    }

    fn evaluate(&self, board: &Board, player_at_leaf: Option<Player>) -> i32 {
        if self.use_mcts {
            if let Some(player) = player_at_leaf {
                // Run MCTS
                // Note: MCTS is expensive.
                let mut mcts = MCTS::new(board, player, Some(self.tt.clone()));
                let win_rate = mcts.run(board, self.mcts_iterations);

                // win_rate is [0, 1] for `player`.
                // Map to score. 1.0 -> 20000, 0.0 -> -20000.
                // value = (win_rate - 0.5) * 2 * 20000
                let val_f = (win_rate - 0.5) * 2.0 * (VAL_KING as f64);
                let val = val_f as i32;

                // Return White-centric score
                if player == Player::Black {
                    return -val;
                } else {
                    return val;
                }
            }
        }

        let mut score = 0;
        for i in 0..board.total_cells {
            if board.white_occupancy.get_bit(i) {
                score += self.get_piece_value(board, i);
            } else if board.black_occupancy.get_bit(i) {
                score -= self.get_piece_value(board, i);
            }
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
                return 0; // Abort
            }
        }
        if self.stop_flag.load(Ordering::Relaxed) {
            return 0;
        }

        // Check for repetition
        if board.is_repetition() {
            return 0; // Draw
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
            return 0; // Stalemate
        }

        let mut best_score = -i32::MAX;
        let original_alpha = alpha;

        for mv in moves {
            let mut next_board = board.clone();
            if next_board.apply_move(&mv).is_ok() {
                let score = -self.minimax(
                    &mut next_board,
                    depth - 1,
                    -beta,
                    -alpha,
                    player.opponent(),
                    start_time,
                );

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
        let mut best_score = -i32::MAX;
        let mut best_moves = Vec::new(); // Collect all best moves

        // Root Search
        let moves = Rules::generate_legal_moves(board, player);
        if moves.is_empty() {
            return None;
        }

        for mv in moves {
            let mut next_board = board.clone();
            if next_board.apply_move(&mv).is_ok() {
                if next_board.is_repetition() {
                    // Repetition handling logic (implicit via minimax returning 0 for it usually)
                }

                let score = -self.minimax(
                    &mut next_board,
                    self.depth - 1,
                    -i32::MAX,
                    i32::MAX,
                    player.opponent(),
                    start_time,
                );

                if score > best_score {
                    best_score = score;
                    best_moves.clear();
                    best_moves.push(mv);
                } else if score == best_score {
                    best_moves.push(mv);
                }
            }
        }

        // Pick random best move
        if !best_moves.is_empty() {
            let mut rng = rand::thread_rng();
            use rand::seq::SliceRandom;
            best_moves.choose(&mut rng).cloned()
        } else {
            None
        }
    }
}
