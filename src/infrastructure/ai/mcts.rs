use crate::config::MctsConfig;
use crate::domain::board::{Board, UnmakeInfo};
use crate::domain::models::{Move, Player};
use crate::domain::rules::{MoveList, Rules};
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};

use std::sync::Arc;

use std::f64;

const UCT_C: f64 = 1.4142;
const CHECKMATE_SCORE: i32 = 30000;

struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    visits: u32,
    score: f64,
    prior: f64,
    unexpanded_moves: MoveList,
    is_terminal: bool,
    move_to_node: Option<Move>,
    player_to_move: Player,
}

pub struct MCTS {
    nodes: Vec<Node>,
    root_player: Player,
    tt: Option<Arc<LockFreeTT>>,
    serial: bool,
    config: Option<MctsConfig>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: Arc<AtomicUsize>,
    rollout_depth: usize,
    num_threads: usize,
}

use super::search_core::get_piece_value;
use super::search_core::minimax_shallow;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize};

impl MCTS {
    pub fn new(
        root_state: &Board,
        root_player: Player,
        tt: Option<Arc<LockFreeTT>>,
        config: Option<MctsConfig>,
        stop_flag: Option<Arc<AtomicBool>>,
        nodes_searched: Option<Arc<AtomicUsize>>,
        rollout_depth: usize,
    ) -> Self {
        let mut root_clone = root_state.clone();
        let moves = Rules::generate_legal_moves(&mut root_clone, root_player);

        let mut sorted_moves: Vec<(Move, i32)> = moves
            .into_iter()
            .map(|m| {
                let to_idx = root_clone.coords_to_index(&m.to.values).unwrap_or(0);
                let victim = get_piece_value(&root_clone, to_idx);
                (m, victim)
            })
            .collect();
        sorted_moves.sort_by(|a, b| a.1.cmp(&b.1));

        let mut moves = MoveList::new();
        for (m, _) in sorted_moves {
            moves.push(m);
        }

        let is_terminal = moves.is_empty() || root_clone.is_repetition();

        let root = Node {
            parent: None,
            children: Vec::new(),
            visits: 0,
            score: 0.0,
            prior: 1.0,
            unexpanded_moves: moves,
            is_terminal,
            move_to_node: None,
            player_to_move: root_player,
        };

        Self {
            nodes: vec![root],
            root_player,
            tt,
            serial: false,
            config,
            stop_flag: stop_flag.unwrap_or_else(|| Arc::new(AtomicBool::new(false))),
            nodes_searched: nodes_searched.unwrap_or_else(|| Arc::new(AtomicUsize::new(0))),
            rollout_depth,
            num_threads: 0,
        }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.num_threads = concurrency;
        self
    }

    pub fn with_serial(mut self) -> Self {
        self.serial = true;
        self
    }

    pub fn run(&mut self, root_state: &Board, iterations: usize) -> (f64, Option<Move>) {
        if iterations == 0 {
            return (0.5, None);
        }

        let num_threads = if self.num_threads > 0 {
            self.num_threads
        } else {
            rayon::current_num_threads()
        };

        let min_iter = self
            .config
            .as_ref()
            .map(|c| c.iter_per_thread as usize)
            .unwrap_or(5);
        let num_tasks = if self.serial {
            1
        } else {
            (iterations / min_iter).clamp(1, num_threads)
        };

        if num_tasks <= 1 {
            self.execute_iterations(root_state, iterations);
            return (self.get_win_rate(), self.get_best_move());
        }

        let chunk_size = iterations / num_tasks;
        let remainder = iterations % num_tasks;

        let results: Vec<(u32, f64, Vec<(Move, u32, f64)>)> = (0..num_tasks)
            .into_par_iter()
            .map(|i| {
                let count = if i < remainder {
                    chunk_size + 1
                } else {
                    chunk_size
                };
                if count == 0 {
                    return (0, 0.0, Vec::new());
                }

                let mut local_mcts = MCTS::new(
                    root_state,
                    self.root_player,
                    self.tt.clone(),
                    self.config.clone(),
                    Some(self.stop_flag.clone()),
                    Some(self.nodes_searched.clone()),
                    self.rollout_depth,
                );
                local_mcts.execute_iterations(root_state, count);

                let root = &local_mcts.nodes[0];

                let child_stats = root
                    .children
                    .iter()
                    .map(|&c_idx| {
                        let child = &local_mcts.nodes[c_idx];
                        (
                            child.move_to_node.clone().unwrap(),
                            child.visits,
                            child.score,
                        )
                    })
                    .collect();

                (root.visits, root.score, child_stats)
            })
            .collect();

        let mut aggregated_children: Vec<(Move, u32, f64)> = Vec::new();

        let mut total_visits = 0;
        let mut total_score = 0.0;

        for (v, s, children) in results {
            total_visits += v;
            total_score += s;
            for (m, cv, cs) in children {
                if let Some(existing) = aggregated_children.iter_mut().find(|(em, _, _)| em == &m) {
                    existing.1 += cv;
                    existing.2 += cs;
                } else {
                    aggregated_children.push((m, cv, cs));
                }
            }
        }

        let win_rate = if total_visits == 0 {
            0.5
        } else {
            total_score / total_visits as f64
        };

        let best_move = aggregated_children
            .into_iter()
            .max_by_key(|(_, visits, _)| *visits)
            .map(|(m, _, _)| m);

        (win_rate, best_move)
    }

    fn get_win_rate(&self) -> f64 {
        let root = &self.nodes[0];
        if root.visits == 0 {
            0.5
        } else {
            root.score / root.visits as f64
        }
    }

    pub fn get_best_move(&self) -> Option<Move> {
        let root = &self.nodes[0];
        if root.children.is_empty() {
            return None;
        }

        let mut best_visits = 0;
        let mut best_move = None;

        for &child_idx in &root.children {
            let child = &self.nodes[child_idx];
            if child.visits > best_visits {
                best_visits = child.visits;
                best_move = child.move_to_node.clone();
            }
        }
        best_move
    }

    fn execute_iterations(&mut self, root_state: &Board, iterations: usize) {
        let mut current_state = root_state.clone();

        for _ in 0..iterations {
            if self.stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let mut node_idx = 0;
            let mut current_player = self.root_player;

            let mut path_stack: Vec<(Move, UnmakeInfo)> = Vec::with_capacity(64);

            while self.nodes[node_idx].unexpanded_moves.is_empty()
                && !self.nodes[node_idx].children.is_empty()
            {
                if self.nodes[node_idx].is_terminal {
                    break;
                }

                let best_child = self.select_child(node_idx);
                node_idx = best_child;

                let mv = self.nodes[node_idx].move_to_node.as_ref().unwrap();

                let info = current_state.apply_move(mv).unwrap();
                path_stack.push((mv.clone(), info));

                current_player = current_player.opponent();
            }

            if !self.nodes[node_idx].is_terminal
                && !self.nodes[node_idx].unexpanded_moves.is_empty()
            {
                let mv = self.nodes[node_idx].unexpanded_moves.pop().unwrap();

                let info = current_state.apply_move(&mv).unwrap();
                path_stack.push((mv.clone(), info));

                let next_player = current_player.opponent();

                let is_repetition = current_state.is_repetition();

                let legal_moves = if is_repetition {
                    MoveList::new()
                } else {
                    Rules::generate_legal_moves(&mut current_state, next_player)
                };

                let is_terminal = is_repetition || legal_moves.is_empty();

                let mut prior = 1.0;
                let to_idx = current_state.coords_to_index(&mv.to.values).unwrap_or(0);
                if current_state.black_occupancy.get_bit(to_idx)
                    || current_state.white_occupancy.get_bit(to_idx)
                {
                    prior = 2.0;
                }

                let mut sorted_moves: Vec<(Move, i32)> = legal_moves
                    .into_iter()
                    .map(|m| {
                        let to_idx = current_state.coords_to_index(&m.to.values).unwrap_or(0);
                        let victim = get_piece_value(&current_state, to_idx);
                        (m, victim)
                    })
                    .collect();

                sorted_moves.sort_by(|a, b| a.1.cmp(&b.1));

                let mut ordered_moves = MoveList::new();
                for (m, _) in sorted_moves {
                    ordered_moves.push(m);
                }

                let new_node = Node {
                    parent: Some(node_idx),
                    children: Vec::new(),
                    visits: 0,
                    score: 0.0,
                    prior,
                    unexpanded_moves: ordered_moves,
                    is_terminal,
                    move_to_node: Some(mv),
                    player_to_move: next_player,
                };

                let new_node_idx = self.nodes.len();
                self.nodes.push(new_node);
                self.nodes[node_idx].children.push(new_node_idx);

                node_idx = new_node_idx;
                current_player = next_player;
            }

            let result_score = if self.nodes[node_idx].is_terminal {
                self.evaluate_terminal(&current_state, current_player)
            } else {
                self.rollout_q_search(&mut current_state, current_player)
            };

            self.backpropagate(node_idx, result_score);

            while let Some((mv, info)) = path_stack.pop() {
                current_state.unmake_move(&mv, info);
            }
        }
    }

    fn select_child(&self, parent_idx: usize) -> usize {
        let parent = &self.nodes[parent_idx];
        let sqrt_n = (parent.visits as f64).sqrt();

        let mut best_score = -f64::INFINITY;
        let mut best_child = 0;

        let maximize = self.root_player == parent.player_to_move;

        let c_puct = self
            .config
            .as_ref()
            .map(|c| c.prior_weight)
            .unwrap_or(UCT_C);

        for &child_idx in &parent.children {
            let child = &self.nodes[child_idx];
            let mean_score = if child.visits > 0 {
                child.score / child.visits as f64
            } else {
                0.5
            };

            let exploitation = if maximize {
                mean_score
            } else {
                1.0 - mean_score
            };

            let exploration = c_puct * child.prior * (sqrt_n / (1.0 + child.visits as f64));
            let uct_value = exploitation + exploration;

            if uct_value > best_score {
                best_score = uct_value;
                best_child = child_idx;
            }
        }
        best_child
    }

    fn rollout_q_search(&self, state: &mut Board, player: Player) -> f64 {
        let score_cp = minimax_shallow(
            state,
            self.rollout_depth,
            -i32::MAX,
            i32::MAX,
            player,
            &self.nodes_searched,
            &self.stop_flag,
            self.tt.as_ref(),
        );

        let k = 0.003;
        let sigmoid = 1.0 / (1.0 + (-k * score_cp as f64).exp());

        let win_rate_for_player = sigmoid;

        if player == self.root_player {
            win_rate_for_player
        } else {
            1.0 - win_rate_for_player
        }
    }

    fn evaluate_terminal(&self, state: &Board, player_at_leaf: Player) -> f64 {
        if state.is_repetition() {
            return 0.5;
        }

        if let Some(king_pos) = state.get_king_coordinate(player_at_leaf) {
            if Rules::is_square_attacked(state, &king_pos, player_at_leaf.opponent()) {
                if let Some(tt) = &self.tt {
                    tt.store(state.hash, -CHECKMATE_SCORE, 255, Flag::Exact, None);
                }

                if player_at_leaf == self.root_player {
                    return 0.0;
                } else {
                    return 1.0;
                }
            }
        }

        if let Some(tt) = &self.tt {
            tt.store(state.hash, 0, 255, Flag::Exact, None);
        }

        0.5
    }

    fn backpropagate(&mut self, mut node_idx: usize, score: f64) {
        loop {
            let node = &mut self.nodes[node_idx];
            node.visits += 1;
            node.score += score;

            if let Some(parent) = node.parent {
                node_idx = parent;
            } else {
                break;
            }
        }
    }
}
