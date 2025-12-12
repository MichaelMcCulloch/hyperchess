```./src/application/game_service.rs
use crate::domain::models::{Board, BoardState, GameResult, Player};
use crate::domain::services::PlayerStrategy;
use std::fmt::Display;

pub struct GameService<'a, S: BoardState> {
    board: Board<S>,
    player_x: Box<dyn PlayerStrategy<S> + 'a>, // Boxing traits requires lifetime if they capture env?
    player_o: Box<dyn PlayerStrategy<S> + 'a>,
    turn: Player,
}

impl<'a, S: BoardState + Display> GameService<'a, S> {
    pub fn new(
        board: Board<S>,
        player_x: Box<dyn PlayerStrategy<S> + 'a>,
        player_o: Box<dyn PlayerStrategy<S> + 'a>,
    ) -> Self {
        GameService {
            board,
            player_x,
            player_o,
            turn: Player::X,
        }
    }

    pub fn board(&self) -> &Board<S> {
        &self.board
    }

    pub fn turn(&self) -> Player {
        self.turn
    }

    pub fn is_game_over(&self) -> Option<GameResult> {
        match self.board.check_status() {
            GameResult::InProgress => None,
            result => Some(result),
        }
    }

    pub fn perform_next_move(&mut self) -> Result<(), String> {
        if self.is_game_over().is_some() {
            return Err("Game is over".to_string());
        }

        let strategy = match self.turn {
            Player::X => &mut self.player_x,
            Player::O => &mut self.player_o,
        };

        if let Some(coord) = strategy.get_best_move(self.board.state(), self.turn) {
            self.board.make_move(coord, self.turn)?;
            self.turn = self.turn.opponent();
            Ok(())
        } else {
            Err("No move available".to_string())
        }
    }
}
```
```./src/application/mod.rs
pub mod game_service;
```
```./src/domain/coordinate.rs
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Coordinate {
    pub values: Vec<usize>,
}

impl Coordinate {
    pub fn new(values: Vec<usize>) -> Self {
        Self { values }
    }

    pub fn dim(&self) -> usize {
        self.values.len()
    }
}

impl fmt::Debug for Coordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, v) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", v)?;
        }
        write!(f, ")")
    }
}
```
```./src/domain/game.rs
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Board, BoardState, GameResult, Player};

#[derive(Debug)]
pub enum GameError {
    InvalidMove(String),
}

/// The Game Aggregate Root.
/// It controls the lifecycle of the game, turns, and winning conditions.
pub struct Game<S: BoardState> {
    board: Board<S>,
    turn: Player,
    status: GameResult,
    move_history: Vec<(Player, Coordinate)>,
}

impl<S: BoardState> Game<S> {
    pub fn new(board: Board<S>) -> Self {
        Self {
            board,
            turn: Player::X,
            status: GameResult::InProgress,
            move_history: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        // Any initialization logic can go here
        self.status = GameResult::InProgress;
        self.turn = Player::X;
    }

    pub fn play_turn(&mut self, coord: Coordinate) -> Result<GameResult, GameError> {
        if self.status != GameResult::InProgress {
            return Err(GameError::InvalidMove("Game is already over".to_string()));
        }

        self.board
            .make_move(coord.clone(), self.turn)
            .map_err(GameError::InvalidMove)?;

        self.move_history.push((self.turn, coord));

        let result = self.board.check_status();
        self.status = result;

        if result == GameResult::InProgress {
            self.turn = self.turn.opponent();
        }

        Ok(result)
    }

    pub fn current_turn(&self) -> Player {
        self.turn
    }

    pub fn status(&self) -> GameResult {
        self.status
    }

    pub fn board(&self) -> &Board<S> {
        &self.board
    }

    // Expose inner state for read-only if needed, or projection
    pub fn state(&self) -> &S {
        self.board.state()
    }
}
```
```./src/domain/mod.rs
pub mod coordinate;
pub mod game;
pub mod models;
pub mod services;
```
```./src/domain/models.rs
use std::fmt::Debug;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Player {
    X,
    O,
}

impl Player {
    pub fn opponent(&self) -> Self {
        match self {
            Player::X => Player::O,
            Player::O => Player::X,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameResult {
    Win(Player),
    Draw,
    InProgress,
}

use crate::domain::coordinate::Coordinate;

/// Trait defining the storage and core mechanics of the board backend.
/// This allows us to strictly separate the "BitBoard" optimization (Infrastructure)
/// from the "Board" concept (Domain).
pub trait BoardState: Debug + Clone {
    fn new(dimension: usize) -> Self
    where
        Self: Sized;
    fn dimension(&self) -> usize;
    fn side(&self) -> usize;
    fn total_cells(&self) -> usize;
    fn get_cell(&self, coord: &Coordinate) -> Option<Player>;
    fn set_cell(&mut self, coord: &Coordinate, player: Player) -> Result<(), String>;
    fn clear_cell(&mut self, coord: &Coordinate);
    fn check_win(&self) -> Option<Player>;
    fn is_full(&self) -> bool;
}

/// The Domain Entity representing the Game Board.
/// It wraps a BoardState implementation.
#[derive(Clone, Debug)]
pub struct Board<S: BoardState> {
    state: S,
}

impl<S: BoardState> Board<S> {
    pub fn new(dimension: usize) -> Self {
        Self {
            state: S::new(dimension),
        }
    }

    pub fn dimension(&self) -> usize {
        self.state.dimension()
    }

    pub fn make_move(&mut self, coord: Coordinate, player: Player) -> Result<(), String> {
        self.state.set_cell(&coord, player)
    }

    pub fn get_cell(&self, coord: &Coordinate) -> Option<Player> {
        self.state.get_cell(coord)
    }

    pub fn check_status(&self) -> GameResult {
        if let Some(winner) = self.state.check_win() {
            return GameResult::Win(winner);
        }
        if self.state.is_full() {
            return GameResult::Draw;
        }
        GameResult::InProgress
    }

    pub fn state(&self) -> &S {
        &self.state
    }
}
```
```./src/domain/services.rs
use crate::domain::models::{BoardState, Player};
use std::time::Duration;

pub trait Clock {
    fn now(&self) -> Duration;
}

use crate::domain::coordinate::Coordinate;

pub trait PlayerStrategy<S: BoardState> {
    fn get_best_move(&mut self, board: &S, player: Player) -> Option<Coordinate>;
}
```
```./src/infrastructure/ai.rs
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
```
```./src/infrastructure/ai/transposition.rs
use std::sync::atomic::{AtomicU64, Ordering};

// Pack data into u64:
// 32 bits score | 8 bits depth | 2 bits flag | 22 bits partial hash/verification
// We will store the FULL key in a separate atomic for verification.
// The packed data is primarily for the value payload.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Flag {
    Exact = 0,
    LowerBound = 1,
    UpperBound = 2,
}

impl Flag {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Flag::Exact,
            1 => Flag::LowerBound,
            2 => Flag::UpperBound,
            _ => Flag::Exact, // Default/Fallback
        }
    }

    fn to_u8(self) -> u8 {
        self as u8
    }
}

#[repr(align(64))]
pub struct TTEntry {
    /// Stores the packed value: score (32), depth (8), flag (2), extra (22)
    pub data: AtomicU64,
    /// Stores the full 64-bit Zobrist key to resolve collisions
    pub key: AtomicU64,
}

pub struct LockFreeTT {
    table: Vec<TTEntry>,
    size: usize,
}

impl LockFreeTT {
    pub fn new(size_mb: usize) -> Self {
        let entry_size = std::mem::size_of::<TTEntry>(); // Should be 16 bytes
        let num_entries = (size_mb * 1024 * 1024) / entry_size;

        let mut table = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            table.push(TTEntry {
                data: AtomicU64::new(0),
                key: AtomicU64::new(0),
            });
        }

        Self {
            table,
            size: num_entries,
        }
    }

    pub fn get(&self, hash: u64) -> Option<(i32, u8, Flag, Option<u16>)> {
        let index = (hash as usize) % self.size;
        // RELAXED ordering is sufficient because we strictly check the key *after* reading data?
        // Actually, to ensure consistency between key and data, we might need stronger ordering or accept tearing.
        // But the standard "lockless" TT in chess engines often accepts some race conditions.
        // A common pattern is:
        // 1. Read key.
        // 2. If match, read data.
        // 3. Verify key again? Or just XOR check?
        //
        // With struct of atomics:
        // We can't guarantee that `key` and `data` are updated atomically together.
        // But `key` check is the guard.
        // If we read `key` == hash, then we read `data`.
        // If `data` was from a previous entry, `key` would be different (mostly).
        // If `data` is being written while we read, we might get torn data? No, `AtomicU64` load is atomic.
        // We might get data from a NEW entry that overwrote the OLD entry but `key` hasn't been updated yet?
        // Or `key` updated but `data` not?
        //
        // High performance engines often bundle `key ^ data` to detect inconsistency,
        // OR just accept that data races are rare enough or benign.
        //
        // Let's stick to the user's suggestion: "Relaxed load is fine for TT; occasional data races are acceptable".

        let entry = &self.table[index];
        let stored_key = entry.key.load(Ordering::Relaxed);

        if stored_key != hash {
            return None;
        }

        let data = entry.data.load(Ordering::Relaxed);

        // Unpack
        // Low 32 bits: score (i32 cast to u32)
        // Next 8 bits: depth
        // Next 2 bits: flag
        // Next 16 bits: best_move (u16, 0xFFFF = None)
        let score_u32 = (data & 0xFFFFFFFF) as u32;
        let score = score_u32 as i32;
        let depth = ((data >> 32) & 0xFF) as u8;
        let flag_u8 = ((data >> 40) & 0x3) as u8;
        let best_move_raw = ((data >> 42) & 0xFFFF) as u16;

        let best_move = if best_move_raw == 0xFFFF {
            None
        } else {
            Some(best_move_raw)
        };

        Some((score, depth, Flag::from_u8(flag_u8), best_move))
    }

    pub fn store(&self, hash: u64, score: i32, depth: u8, flag: Flag, best_move: Option<u16>) {
        let index = (hash as usize) % self.size;
        let entry = &self.table[index];

        // Packing
        let score_u32 = score as u32;
        let depth_u64 = depth as u64;
        let flag_u64 = flag.to_u8() as u64;
        let best_move_val = best_move.unwrap_or(0xFFFF) as u64;

        let packed =
            (score_u32 as u64) | (depth_u64 << 32) | (flag_u64 << 40) | (best_move_val << 42);

        // Store
        // We overwrite unconditionally or based on depth?
        // Simple replacement strategy: always overwrite.
        // Or "depth-preferred" replacement?
        // For now, simple overwrite as per user snippet.

        entry.key.store(hash, Ordering::Relaxed);
        entry.data.store(packed, Ordering::Relaxed);
    }
}
```
```./src/infrastructure/console.rs
use crate::domain::models::{BoardState, Player};
use crate::domain::services::PlayerStrategy;
use std::io::{self, Write};

pub struct HumanConsolePlayer;

impl HumanConsolePlayer {
    pub fn new() -> Self {
        Self
    }
}

use crate::domain::coordinate::Coordinate;

impl<S: BoardState> PlayerStrategy<S> for HumanConsolePlayer {
    fn get_best_move(&mut self, board: &S, _player: Player) -> Option<Coordinate> {
        loop {
            print!("Enter move index (0-{}): ", board.total_cells() - 1);
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            match input.trim().parse::<usize>() {
                Ok(idx) => {
                    // Temporarily using index for input, converting to coordinate
                    // Ideally we'd ask for coordinates (x,y,z) but for now let's keep index input for simpler UI
                    // or implement a conversion.
                    // Since BoardState doesn't expose index conversion directly (it's infrastructure hidden),
                    // we need a way.
                    // But wait, Coordinate is generic. We need to construct it.
                    // We can construct it if we know dimensions.

                    // Actually, for HumanConsolePlayer, we might want to ask for Coordinates?
                    // Or keep index and convert.
                    // But `index_to_coords` is in `infrastructure::persistence`.
                    // We should probably rely on `Coordinate` constructor if we know the dimension.

                    // For now, let's assume we can map index to Coordinate if we knew how.
                    // BUT `BoardState` trait doesn't have `from_index`.

                    // We can compute it manually if we know board dimensions.
                    let dim = board.dimension();
                    let side = board.side();

                    // Re-implement index_to_coords here or make it a domain utility?
                    // It fits in Coordinate logic?
                    // Let's implement it here locally or move it to Coordinate.

                    let mut coords = vec![0; dim];
                    let mut temp = idx;
                    for i in 0..dim {
                        coords[i] = temp % side;
                        temp /= side;
                    }
                    let coord = Coordinate::new(coords);

                    if idx < board.total_cells() && board.get_cell(&coord).is_none() {
                        return Some(coord);
                    } else if idx >= board.total_cells() {
                        println!("Index out of bounds");
                    } else {
                        println!("Cell already occupied");
                    }
                }
                Err(_) => println!("Invalid number"),
            }
        }
    }
}
```
```./src/infrastructure/display.rs
use crate::domain::models::{BoardState, Player};
use std::fmt;

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_X: &str = "\x1b[31m";
const COLOR_O: &str = "\x1b[36m";
const COLOR_DIM: &str = "\x1b[90m";

struct Canvas {
    width: usize,
    height: usize,
    buffer: Vec<String>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Canvas {
            width,
            height,
            buffer: vec![" ".to_string(); width * height],
        }
    }

    fn put(&mut self, x: usize, y: usize, s: &str) {
        if x < self.width && y < self.height {
            self.buffer[y * self.width + x] = s.to_string();
        }
    }
}

impl fmt::Display for Canvas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for y in 0..self.height {
            for x in 0..self.width {
                write!(f, "{}", self.buffer[y * self.width + x])?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

pub fn render_board<S: BoardState>(board: &S) -> String {
    let dim = board.dimension();
    let (w, h) = calculate_size(dim);
    let mut canvas = Canvas::new(w, h);

    draw_recursive(board, dim, &mut canvas, 0, 0, 0);

    canvas.to_string()
}

fn calculate_size(dim: usize) -> (usize, usize) {
    if dim == 0 {
        return (1, 1);
    }
    if dim == 1 {
        return (3, 1);
    }

    if dim == 2 {
        return (5, 3);
    }

    let (child_w, child_h) = calculate_size(dim - 1);

    if dim % 2 != 0 {
        let gap = 2;
        (child_w * 3 + gap * 2, child_h)
    } else {
        let gap = 1;
        (child_w, child_h * 3 + gap * 2)
    }
}

fn draw_recursive<S: BoardState>(
    board: &S,
    current_dim: usize,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
) {
    let side = 3;

    if current_dim == 2 {
        for dy in 0..3 {
            for dx in 0..3 {
                let cell_idx = base_index + dx + dy * side;
                let coord_vals = crate::infrastructure::persistence::index_to_coords(
                    cell_idx,
                    board.dimension(),
                    board.side(),
                );
                let coord = crate::domain::coordinate::Coordinate::new(coord_vals);

                let s = match board.get_cell(&coord) {
                    Some(Player::X) => format!("{}X{}", COLOR_X, COLOR_RESET),
                    Some(Player::O) => format!("{}O{}", COLOR_O, COLOR_RESET),
                    None => format!("{}.{}", COLOR_DIM, COLOR_RESET),
                };
                canvas.put(x + dx * 2, y + dy, &s);
            }
        }
        return;
    }

    let (child_w, child_h) = calculate_size(current_dim - 1);
    let stride = side.pow((current_dim - 1) as u32);

    if current_dim % 2 != 0 {
        let gap = 2;
        for i in 0..3 {
            let next_x = x + i * (child_w + gap);
            let next_y = y;
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < 2 {
                let sep_x = next_x + child_w + gap / 2 - 1;
                for k in 0..child_h {
                    canvas.put(sep_x, next_y + k, &format!("{}|{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    } else {
        let gap = 1;
        for i in 0..3 {
            let next_x = x;
            let next_y = y + i * (child_h + gap);
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < 2 {
                let sep_y = next_y + child_h;
                for k in 0..child_w {
                    canvas.put(next_x + k, sep_y, &format!("{}-{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    }
}
```
```./src/infrastructure/mod.rs
pub mod ai;
pub mod console;
pub mod display;
pub mod persistence;
pub mod symmetries;
pub mod time;
```
```./src/infrastructure/persistence.rs
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{BoardState, Player};
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::fmt;
use std::sync::Arc;

// Removed Copy: Vec<u64> cannot be Copy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BitBoard {
    Small(u32),
    Medium(u128),
    Large { data: Vec<u64> },
}

#[derive(Clone, Debug)]
pub enum WinningMasks {
    Small {
        masks: Vec<u32>,
        map_flat: Vec<usize>,
        map_offsets: Vec<(u32, u32)>, // (start, count)
        cell_mask_lookup: Vec<Vec<u32>>,
    },
    Medium {
        masks: Vec<u128>,
        map_flat: Vec<usize>,
        map_offsets: Vec<(u32, u32)>,
        cell_mask_lookup: Vec<Vec<u128>>,
    },
    Large {
        masks: Vec<Vec<u64>>,
        map_flat: Vec<usize>,
        map_offsets: Vec<(u32, u32)>,
    },
}

#[derive(Clone, Debug)]
pub struct BitBoardState {
    pub dimension: usize,
    pub side: usize,
    pub total_cells: usize,
    pub p1: BitBoard,
    pub p2: BitBoard,
    pub winning_masks: Arc<WinningMasks>,
}

impl BitBoardState {
    pub fn get_cell_index(&self, index: usize) -> Option<Player> {
        if self.p1.get_bit(index) {
            Some(Player::X)
        } else if self.p2.get_bit(index) {
            Some(Player::O)
        } else {
            None
        }
    }

    pub fn set_cell_index(&mut self, index: usize, player: Player) -> Result<(), String> {
        if index >= self.total_cells {
            return Err("Index out of bounds".to_string());
        }
        if self.p1.get_bit(index) || self.p2.get_bit(index) {
            return Err("Cell already occupied".to_string());
        }

        match player {
            Player::X => self.p1.set_bit(index),
            Player::O => self.p2.set_bit(index),
        }
        Ok(())
    }

    pub fn clear_cell_index(&mut self, index: usize) {
        self.p1.clear_bit(index);
        self.p2.clear_bit(index);
    }
}

impl BoardState for BitBoardState {
    fn new(dimension: usize) -> Self {
        let side: usize = 3;
        let total_cells = side.pow(dimension as u32);

        let p1 = BitBoard::new_empty(dimension, side);
        let p2 = BitBoard::new_empty(dimension, side);

        let winning_masks = Arc::new(generate_winning_masks(dimension, side));

        BitBoardState {
            dimension,
            side,
            total_cells,
            p1,
            p2,
            winning_masks,
        }
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn side(&self) -> usize {
        self.side
    }

    fn total_cells(&self) -> usize {
        self.total_cells
    }

    fn get_cell(&self, coord: &Coordinate) -> Option<Player> {
        let index = coords_to_index(&coord.values, self.side)?;
        self.get_cell_index(index)
    }

    fn set_cell(&mut self, coord: &Coordinate, player: Player) -> Result<(), String> {
        let index = coords_to_index(&coord.values, self.side)
            .ok_or_else(|| "Invalid coordinate".to_string())?;
        self.set_cell_index(index, player)
    }

    fn clear_cell(&mut self, coord: &Coordinate) {
        if let Some(index) = coords_to_index(&coord.values, self.side) {
            self.clear_cell_index(index);
        }
    }

    fn check_win(&self) -> Option<Player> {
        if self.p1.check_win(&self.winning_masks) {
            return Some(Player::X);
        }
        if self.p2.check_win(&self.winning_masks) {
            return Some(Player::O);
        }
        None
    }

    fn is_full(&self) -> bool {
        // Since BitBoard is no longer Copy, we must use reference or clone appropriately.
        // or_with now takes &self and other by value or ref?
        // Let's refactor or_with to take reference to avoid clone.
        let combined = self.p1.or_with(&self.p2);
        combined.is_full(self.total_cells)
    }
}

impl fmt::Display for BitBoardState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", crate::infrastructure::display::render_board(self))
    }
}

// --- BitBoard Implementation ---

impl BitBoard {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        if total_cells <= 32 {
            BitBoard::Small(0)
        } else if total_cells <= 128 {
            BitBoard::Medium(0)
        } else {
            let len = (total_cells + 63) / 64;
            // Removed panic, allocated vector based on required length
            BitBoard::Large { data: vec![0; len] }
        }
    }

    pub fn set_bit(&mut self, index: usize) {
        match self {
            BitBoard::Small(b) => *b |= 1 << index,
            BitBoard::Medium(b) => *b |= 1 << index,
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if vec_idx < data.len() {
                    data[vec_idx] |= 1 << (index % 64);
                }
            }
        }
    }

    pub fn clear_bit(&mut self, index: usize) {
        match self {
            BitBoard::Small(b) => *b &= !(1 << index),
            BitBoard::Medium(b) => *b &= !(1 << index),
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if vec_idx < data.len() {
                    data[vec_idx] &= !(1 << (index % 64));
                }
            }
        }
    }

    pub fn get_bit(&self, index: usize) -> bool {
        match self {
            BitBoard::Small(b) => (*b & (1 << index)) != 0,
            BitBoard::Medium(b) => (*b & (1 << index)) != 0,
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if let Some(chunk) = data.get(vec_idx) {
                    (chunk & (1 << (index % 64))) != 0
                } else {
                    false
                }
            }
        }
    }

    pub fn count_ones(&self) -> u32 {
        match self {
            BitBoard::Small(b) => b.count_ones(),
            BitBoard::Medium(b) => b.count_ones(),
            BitBoard::Large { data } => data.iter().map(|c| c.count_ones()).sum(),
        }
    }

    pub fn or_with(&self, other: &BitBoard) -> BitBoard {
        match (self, other) {
            (BitBoard::Small(a), BitBoard::Small(b)) => BitBoard::Small(a | b),
            (BitBoard::Medium(a), BitBoard::Medium(b)) => BitBoard::Medium(a | b),
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = a.len().max(b.len());
                let mut new_data = vec![0; len];
                for i in 0..len {
                    let v1 = if i < a.len() { a[i] } else { 0 };
                    let v2 = if i < b.len() { b[i] } else { 0 };
                    new_data[i] = v1 | v2;
                }
                BitBoard::Large { data: new_data }
            }
            // Fallback for mismatched types (should not happen in valid state)
            _ => self.clone(),
        }
    }

    pub fn is_full(&self, total_cells: usize) -> bool {
        self.count_ones() as usize >= total_cells
    }

    pub fn check_win(&self, winning_masks: &WinningMasks) -> bool {
        match (self, winning_masks) {
            (BitBoard::Small(board), WinningMasks::Small { masks, .. }) => unsafe {
                check_win_u32_opt(*board, masks)
            },
            (BitBoard::Medium(board), WinningMasks::Medium { masks, .. }) => unsafe {
                check_win_u128_opt(*board, masks)
            },
            (BitBoard::Large { data: board }, WinningMasks::Large { masks, .. }) => {
                masks.iter().any(|mask_chunks| {
                    if board.len() < mask_chunks.len() {
                        return false;
                    }
                    // Zip iteration is safer and cleaner
                    mask_chunks
                        .iter()
                        .zip(board.iter())
                        .all(|(m, b)| (b & m) == *m)
                })
            }
            _ => false,
        }
    }

    // Unused in current AI but kept for API consistency
    pub fn check_win_at(&self, winning_masks: &WinningMasks, index: usize) -> bool {
        match (self, winning_masks) {
            (
                BitBoard::Small(board),
                WinningMasks::Small {
                    cell_mask_lookup, ..
                },
            ) => {
                if let Some(masks_for_cell) = cell_mask_lookup.get(index) {
                    for &m in masks_for_cell {
                        if (board & m) == m {
                            return true;
                        }
                    }
                }
                false
            }
            (
                BitBoard::Medium(board),
                WinningMasks::Medium {
                    cell_mask_lookup, ..
                },
            ) => {
                if let Some(masks_for_cell) = cell_mask_lookup.get(index) {
                    for &m in masks_for_cell {
                        if (board & m) == m {
                            return true;
                        }
                    }
                }
                false
            }
            (
                BitBoard::Large { data: board },
                WinningMasks::Large {
                    masks,
                    map_flat,
                    map_offsets,
                },
            ) => {
                if index < map_offsets.len() {
                    let (start, count) = map_offsets[index];
                    let range = start as usize..(start + count) as usize;
                    for &i in &map_flat[range] {
                        let mask_chunks = &masks[i];
                        let mut match_all = true;
                        for (k, m) in mask_chunks.iter().enumerate() {
                            if let Some(b) = board.get(k) {
                                if (b & *m) != *m {
                                    match_all = false;
                                    break;
                                }
                            } else {
                                // Board smaller than mask (shouldn't happen)
                                match_all = false;
                                break;
                            }
                        }
                        if match_all {
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
}

// --- Mask Generation Logic ---

fn generate_winning_masks(dimension: usize, side: usize) -> WinningMasks {
    let lines_indices = generate_winning_lines_indices(dimension, side);
    let total_cells = side.pow(dimension as u32);

    let mut map_flat = Vec::new();
    let mut map_offsets = Vec::with_capacity(total_cells);

    // Build temporary map to group lines by cell
    let mut temp_map: Vec<Vec<usize>> = vec![vec![]; total_cells];
    for (line_idx, line) in lines_indices.iter().enumerate() {
        for &cell_idx in line {
            temp_map[cell_idx].push(line_idx);
        }
    }

    // Flatten
    for indices in temp_map {
        let start = map_flat.len() as u32;
        let count = indices.len() as u32;
        map_flat.extend(indices);
        map_offsets.push((start, count));
    }

    if total_cells <= 32 {
        let mut masks = Vec::new();
        let mut cell_mask_lookup = vec![Vec::new(); total_cells];
        for line in lines_indices {
            let mut mask: u32 = 0;
            for &idx in &line {
                mask |= 1 << idx;
            }
            masks.push(mask);
            for idx in line {
                cell_mask_lookup[idx].push(mask);
            }
        }
        WinningMasks::Small {
            masks,
            map_flat,
            map_offsets,
            cell_mask_lookup,
        }
    } else if total_cells <= 128 {
        let mut masks = Vec::new();
        let mut cell_mask_lookup = vec![Vec::new(); total_cells];
        for line in lines_indices {
            let mut mask: u128 = 0;
            for &idx in &line {
                mask |= 1 << idx;
            }
            masks.push(mask);
            for idx in line {
                cell_mask_lookup[idx].push(mask);
            }
        }
        WinningMasks::Medium {
            masks,
            map_flat,
            map_offsets,
            cell_mask_lookup,
        }
    } else {
        let mut masks = Vec::new();
        let num_u64s = (total_cells + 63) / 64;
        for line in lines_indices {
            let mut mask_chunks = vec![0u64; num_u64s];
            for idx in line {
                let vec_idx = idx / 64;
                mask_chunks[vec_idx] |= 1 << (idx % 64);
            }
            masks.push(mask_chunks);
        }
        WinningMasks::Large {
            masks,
            map_flat,
            map_offsets,
        }
    }
}

// ... [Existing helper functions: generate_winning_lines_indices, get_canonical_directions, etc.] ...
fn generate_winning_lines_indices(dimension: usize, side: usize) -> Vec<Vec<usize>> {
    let mut lines = Vec::new();
    let all_directions = get_canonical_directions(dimension);

    for dir in all_directions {
        let valid_starts = get_valid_starts(dimension, side, &dir);
        for start in valid_starts {
            let mut line = Vec::new();
            let mut current = start.clone();
            let mut valid = true;

            for _ in 0..side {
                if let Some(idx) = coords_to_index(&current, side) {
                    line.push(idx);

                    for (i, d) in dir.iter().enumerate() {
                        let next_val = current[i] as isize + d;
                        current[i] = next_val as usize;
                    }
                } else {
                    valid = false;
                    break;
                }
            }

            if valid && line.len() == side {
                lines.push(line);
            }
        }
    }
    lines
}

fn get_canonical_directions(dimension: usize) -> Vec<Vec<isize>> {
    let mut dirs = Vec::new();
    let num_dirs = 3_usize.pow(dimension as u32);
    for i in 0..num_dirs {
        let mut dir = Vec::new();
        let mut temp = i;
        let mut has_nonzero = false;
        let mut first_nonzero_is_positive = false;
        for _ in 0..dimension {
            let digit = temp % 3;
            temp /= 3;
            let val = match digit {
                0 => 0,
                1 => 1,
                2 => -1,
                _ => unreachable!(),
            };
            dir.push(val);
        }
        for &val in &dir {
            if val != 0 {
                has_nonzero = true;
                if val > 0 {
                    first_nonzero_is_positive = true;
                }
                break;
            }
        }
        if has_nonzero && first_nonzero_is_positive {
            dirs.push(dir);
        }
    }
    dirs
}

fn get_valid_starts(dimension: usize, side: usize, dir: &[isize]) -> Vec<Vec<usize>> {
    let num_cells = side.pow(dimension as u32);
    let mut starts = Vec::new();
    for i in 0..num_cells {
        let coords = index_to_coords(i, dimension, side);
        let end_coords = coords.clone();
        let mut possible = true;
        for (c_idx, &d) in dir.iter().enumerate() {
            let start_val = end_coords[c_idx] as isize;
            let end_val = start_val + d * (side as isize - 1);
            if end_val < 0 || end_val >= side as isize {
                possible = false;
                break;
            }
        }
        if possible {
            starts.push(coords);
        }
    }
    starts
}

pub fn index_to_coords(index: usize, dimension: usize, side: usize) -> Vec<usize> {
    let mut coords = vec![0; dimension];
    let mut temp = index;
    for i in 0..dimension {
        coords[i] = temp % side;
        temp /= side;
    }
    coords
}

pub fn coords_to_index(coords: &[usize], side: usize) -> Option<usize> {
    let mut index = 0;
    let mut multiplier = 1;
    for &c in coords {
        if c >= side {
            return None;
        }
        index += c * multiplier;
        multiplier *= side;
    }
    Some(index)
}

// ... [Include existing intrinsic optimized check_win implementations here] ...
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u32_opt(board: u32, masks: &[u32]) -> bool {
    let board_vec = unsafe { _mm256_set1_epi32(board as i32) };
    let chunks = masks.chunks_exact(8);
    let remainder = chunks.remainder();
    for chunk in chunks {
        unsafe {
            let mask_vec = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            let and_res = _mm256_and_si256(board_vec, mask_vec);
            let cmp = _mm256_cmpeq_epi32(and_res, mask_vec);
            if _mm256_movemask_epi8(cmp) != 0 {
                return true;
            }
        }
    }
    for &m in remainder {
        if (board & m) == m {
            return true;
        }
    }
    false
}

// Fallbacks for non-AVX... (omitted for brevity, keep existing implementations)
#[cfg(not(target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u32_opt(board: u32, masks: &[u32]) -> bool {
    masks.iter().any(|&m| (board & m) == m)
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u128_opt(board: u128, masks: &[u128]) -> bool {
    let board_low = board as u64;
    let board_high = (board >> 64) as u64;
    let board_vec = unsafe {
        _mm256_set_epi64x(
            board_high as i64,
            board_low as i64,
            board_high as i64,
            board_low as i64,
        )
    };
    let chunks = masks.chunks_exact(2);
    let remainder = chunks.remainder();
    for chunk in chunks {
        unsafe {
            let mask_vec = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            let and_res = _mm256_and_si256(board_vec, mask_vec);
            let cmp = _mm256_cmpeq_epi64(and_res, mask_vec);
            let mask_bits = _mm256_movemask_epi8(cmp);
            if (mask_bits & 0xFFFF) == 0xFFFF {
                return true;
            }
            if (mask_bits as u32 & 0xFFFF0000) == 0xFFFF0000 {
                return true;
            }
        }
    }
    for &m in remainder {
        if (board & m) == m {
            return true;
        }
    }
    false
}

#[cfg(not(target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u128_opt(board: u128, masks: &[u128]) -> bool {
    masks.iter().any(|&m| (board & m) == m)
}
```
```./src/infrastructure/symmetries.rs
pub struct SymmetryHandler {
    pub maps: Vec<Vec<usize>>,
}

impl SymmetryHandler {
    pub fn new(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let mut maps = Vec::new();

        let mut axes: Vec<usize> = (0..dimension).collect();
        let permutations = permute(&mut axes);

        let num_reflections = 1 << dimension;

        for perm in &permutations {
            for ref_mask in 0..num_reflections {
                let mut map = vec![0; total_cells];

                for i in 0..total_cells {
                    let coords = index_to_coords(i, dimension, side);

                    let mut new_coords = vec![0; dimension];
                    for (dest_axis, &src_axis) in perm.iter().enumerate() {
                        new_coords[dest_axis] = coords[src_axis];
                    }

                    for (axis, val) in new_coords.iter_mut().enumerate() {
                        if (ref_mask >> axis) & 1 == 1 {
                            *val = side - 1 - *val;
                        }
                    }

                    map[i] = coords_to_index(&new_coords, side);
                }
                maps.push(map);
            }
        }

        SymmetryHandler { maps }
    }
}

fn permute(arr: &mut [usize]) -> Vec<Vec<usize>> {
    let mut res = Vec::new();
    heap_permute(arr.len(), arr, &mut res);
    res
}

fn heap_permute(k: usize, arr: &mut [usize], res: &mut Vec<Vec<usize>>) {
    if k == 1 {
        res.push(arr.to_vec());
    } else {
        heap_permute(k - 1, arr, res);
        for i in 0..k - 1 {
            if k % 2 == 0 {
                arr.swap(i, k - 1);
            } else {
                arr.swap(0, k - 1);
            }
            heap_permute(k - 1, arr, res);
        }
    }
}

fn index_to_coords(mut index: usize, dim: usize, side: usize) -> Vec<usize> {
    let mut coords = Vec::with_capacity(dim);
    for _ in 0..dim {
        coords.push(index % side);
        index /= side;
    }
    coords
}

fn coords_to_index(coords: &[usize], side: usize) -> usize {
    let mut idx = 0;
    let mut mul = 1;
    for &c in coords {
        idx += c * mul;
        mul *= side;
    }
    idx
}
```
```./src/infrastructure/time.rs
use crate::domain::services::Clock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
    }
}

pub struct FakeClock {
    current_time: Duration,
}

impl FakeClock {
    pub fn new(start_time: Duration) -> Self {
        Self {
            current_time: start_time,
        }
    }

    pub fn advance(&mut self, amount: Duration) {
        self.current_time += amount;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.current_time
    }
}
```
```./src/interface/console.rs
use crate::application::game_service::GameService;
use crate::domain::models::{BoardState, GameResult};
use std::fmt::Display;

pub struct ConsoleInterface;

impl ConsoleInterface {
    pub fn run<S>(mut game_service: GameService<S>)
    where
        S: BoardState + Display,
    {
        println!("Starting Game...");
        println!("{}", game_service.board().state());

        loop {
            if let Some(result) = game_service.is_game_over() {
                match result {
                    GameResult::Win(p) => println!("Player {:?} Wins!", p),
                    GameResult::Draw => println!("It's a Draw!"),
                    _ => {}
                }
                break;
            }

            println!("Player {:?}'s turn", game_service.turn());

            match game_service.perform_next_move() {
                Ok(_) => {
                    println!("{}", game_service.board().state());
                }
                Err(e) => {
                    println!("Error: {}", e);
                    // In a real game we might want to retry immediately if it was input error,
                    // but here the strategy (HumanConsolePlayer) loops internally for valid input.
                    // If we get an error here it's likely "No move available" or "Game Over".
                    if e == "No move available" {
                        break;
                    }
                }
            }
        }
    }
}
```
```./src/interface/mod.rs
pub mod console;
```
```./src/lib.rs
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interface;
```
```./src/main.rs
use hyperchess::application::game_service::GameService;
use hyperchess::domain::models::{Board, BoardState};
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;
use hyperchess::infrastructure::console::HumanConsolePlayer;
use hyperchess::infrastructure::persistence::BitBoardState;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dimension = 3;
    let mut player_x_type = "h";
    let mut player_o_type = "c";
    let mut depth = usize::MAX;

    if args.len() > 1 {
        if let Ok(d) = args[1].parse::<usize>() {
            dimension = d;
        }
    }
    if args.len() > 2 {
        let mode = args[2].as_str();
        if mode.len() >= 2 {
            player_x_type = &mode[0..1];
            player_o_type = &mode[1..2];
        }
    }
    if args.len() > 3 {
        if let Ok(d) = args[3].parse::<usize>() {
            depth = d;
        }
    }

    let _board_state = BitBoardState::new(dimension);

    let player_x: Box<dyn PlayerStrategy<BitBoardState>> = match player_x_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth)),
        _ => Box::new(HumanConsolePlayer::new()),
    };

    let player_o: Box<dyn PlayerStrategy<BitBoardState>> = match player_o_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth)),
        _ => Box::new(MinimaxBot::new(depth)),
    };

    // Board generic param inference?
    // We need to explicitly type the Board or let it infer from GameService.
    let board = Board::<BitBoardState>::new(dimension);

    let game = GameService::new(board, player_x, player_o);
    hyperchess::interface::console::ConsoleInterface::run(game);
}
```
```./tests/minimax_optimality.rs
use hyperchess::domain::models::{BoardState, Player};
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;
use hyperchess::infrastructure::persistence::{BitBoardState, coords_to_index};

fn create_board(dimension: usize, moves: &[(usize, Player)]) -> BitBoardState {
    let mut board = BitBoardState::new(dimension);
    for &(idx, player) in moves {
        board
            .set_cell_index(idx, player)
            .expect("Failed to set cell in test setup");
    }
    board
}

fn assert_best_move(
    best_move: Option<hyperchess::domain::coordinate::Coordinate>,
    expected_index: usize,
    side: usize,
    msg: &str,
) {
    let move_idx = best_move.and_then(|c| coords_to_index(&c.values, side));
    assert_eq!(move_idx, Some(expected_index), "{}", msg);
}

#[test]
fn test_2d_win_in_1() {
    let moves = vec![
        (0, Player::X),
        (3, Player::O),
        (1, Player::X),
        (4, Player::O),
    ];
    let board = create_board(2, &moves);
    let mut bot = MinimaxBot::new(9);

    let best_move = bot.get_best_move(&board, Player::X);
    assert_best_move(
        best_move,
        2,
        3,
        "Minimax failed to find immediate win in 2D",
    );
}

#[test]
fn test_2d_block_in_1() {
    let moves = vec![(0, Player::X), (3, Player::O), (1, Player::X)];
    let board = create_board(2, &moves);
    let mut bot = MinimaxBot::new(9);

    let best_move = bot.get_best_move(&board, Player::O);
    assert_best_move(
        best_move,
        2,
        3,
        "Minimax failed to block immediate loss in 2D",
    );
}

#[test]
fn test_2d_win_in_2_fork() {
    let moves = vec![(0, Player::X), (4, Player::O), (8, Player::X)];
    let board = create_board(2, &moves);
    let mut bot = MinimaxBot::new(9);

    let best_move = bot.get_best_move(&board, Player::X);
    let move_idx = best_move.and_then(|c| coords_to_index(&c.values, 3));

    assert!(
        [2, 6].contains(&move_idx.unwrap()),
        "Minimax failed to find fork move in 2D. Got {:?}",
        move_idx
    );
}

#[test]
fn test_3d_win_in_1() {
    let moves = vec![
        (0, Player::X),
        (1, Player::O),
        (9, Player::X),
        (2, Player::O),
    ];
    let board = create_board(3, &moves);
    let mut bot = MinimaxBot::new(3);

    let best_move = bot.get_best_move(&board, Player::X);
    assert_best_move(
        best_move,
        18,
        3,
        "Minimax failed to find immediate win in 3D",
    );
}

#[test]
fn test_3d_block_in_1() {
    let moves = vec![(0, Player::X), (1, Player::O), (9, Player::X)];
    let board = create_board(3, &moves);
    let mut bot = MinimaxBot::new(3);

    let best_move = bot.get_best_move(&board, Player::O);
    assert_best_move(
        best_move,
        18,
        3,
        "Minimax failed to block immediate loss in 3D",
    );
}

#[test]
fn test_4d_win_in_1() {
    let moves = vec![
        (0, Player::X),
        (1, Player::O),
        (27, Player::X),
        (2, Player::O),
    ];
    let board = create_board(4, &moves);
    let mut bot = MinimaxBot::new(2);

    let best_move = bot.get_best_move(&board, Player::X);
    assert_best_move(
        best_move,
        54,
        3,
        "Minimax failed to find immediate win in 4D",
    );
}

#[test]
fn test_4d_block_in_1() {
    let moves = vec![(0, Player::X), (1, Player::O), (27, Player::X)];
    let board = create_board(4, &moves);
    let mut bot = MinimaxBot::new(2);

    let best_move = bot.get_best_move(&board, Player::O);
    assert_best_move(
        best_move,
        54,
        3,
        "Minimax failed to block immediate loss in 4D",
    );
}
```
