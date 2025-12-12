use crate::domain::models::{BoardState, Move, Player};
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use crate::infrastructure::mechanics::MoveGenerator;
use crate::infrastructure::persistence::BitBoardState;
use rand::Rng; // Needed for Rng trait methods like gen/random
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub mod transposition;

const CHECKMATE_SCORE: i32 = 30000;
const TIMEOUT_CHECK_INTERVAL: usize = 2048;

// Material values
const VAL_PAWN: i32 = 100;
const VAL_KNIGHT: i32 = 320;
const VAL_BISHOP: i32 = 330;
const VAL_ROOK: i32 = 500;
const VAL_QUEEN: i32 = 900;
const VAL_KING: i32 = 20000;

struct ZobristKeys {
    piece_keys: Vec<u64>,
    black_to_move: u64,
}

impl ZobristKeys {
    fn new(total_cells: usize) -> Self {
        let mut rng = rand::thread_rng();
        let size = 12 * total_cells;
        let mut piece_keys = Vec::with_capacity(size);
        for _ in 0..size {
            // Using r#gen because 'gen' is a reserved keyword in Rust 2024
            piece_keys.push(rng.r#gen());
        }
        Self {
            piece_keys,
            black_to_move: rng.r#gen(),
        }
    }

    fn get_hash(&self, board: &BitBoardState, current_player: Player) -> u64 {
        let mut hash = 0;
        if current_player == Player::Black {
            hash ^= self.black_to_move;
        }

        for i in 0..board.total_cells {
            if board.white_occupancy.get_bit(i) {
                let offset = if board.pawns.get_bit(i) {
                    0
                } else if board.knights.get_bit(i) {
                    1
                } else if board.bishops.get_bit(i) {
                    2
                } else if board.rooks.get_bit(i) {
                    3
                } else if board.queens.get_bit(i) {
                    4
                } else if board.kings.get_bit(i) {
                    5
                } else {
                    continue;
                };
                hash ^= self.piece_keys[offset * board.total_cells + i];
            } else if board.black_occupancy.get_bit(i) {
                let offset = if board.pawns.get_bit(i) {
                    6
                } else if board.knights.get_bit(i) {
                    7
                } else if board.bishops.get_bit(i) {
                    8
                } else if board.rooks.get_bit(i) {
                    9
                } else if board.queens.get_bit(i) {
                    10
                } else if board.kings.get_bit(i) {
                    11
                } else {
                    continue;
                };
                hash ^= self.piece_keys[offset * board.total_cells + i];
            }
        }
        hash
    }
}

pub struct MinimaxBot {
    depth: usize,
    time_limit: Duration,
    tt: Arc<LockFreeTT>,
    zobrist: Arc<ZobristKeys>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: std::sync::atomic::AtomicUsize,
    _randomized: bool,
}

impl MinimaxBot {
    pub fn new(depth: usize, time_limit_ms: u64, dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        Self {
            depth,
            time_limit: Duration::from_millis(time_limit_ms),
            tt: Arc::new(LockFreeTT::new(64)),
            zobrist: Arc::new(ZobristKeys::new(total_cells)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: std::sync::atomic::AtomicUsize::new(0),
            _randomized: true,
        }
    }

    fn evaluate(&self, board: &BitBoardState) -> i32 {
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

    fn get_piece_value(&self, board: &BitBoardState, idx: usize) -> i32 {
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
        board: &mut BitBoardState,
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

        let hash = self.zobrist.get_hash(board, player);
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
                Player::White => self.evaluate(board),
                Player::Black => -self.evaluate(board),
            };
        }

        let moves = MoveGenerator::generate_legal_moves(board, player);

        if moves.is_empty() {
            if let Some(king_pos) = board.get_king_coordinate(player) {
                if MoveGenerator::is_square_attacked(board, &king_pos, player.opponent()) {
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

impl PlayerStrategy<BitBoardState> for MinimaxBot {
    fn get_move(&mut self, board: &BitBoardState, player: Player) -> Option<Move> {
        self.nodes_searched.store(0, Ordering::Relaxed);
        self.stop_flag.store(false, Ordering::Relaxed);

        let start_time = Instant::now();
        let mut best_move = None;
        let mut best_score = -i32::MAX;

        // Root Search
        let moves = MoveGenerator::generate_legal_moves(board, player);
        if moves.is_empty() {
            return None;
        }

        for mv in moves {
            let mut next_board = board.clone();
            if next_board.apply_move(&mv).is_ok() {
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
                    best_move = Some(mv);
                }
            }
        }

        best_move
    }
}
