use crate::domain::board::{Board, UnmakeInfo};
use crate::domain::models::{Move, Player};
use crate::domain::rules::{MoveList, Rules};
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use rand::seq::SliceRandom;
use std::sync::Arc;

use std::f64;

const UCT_C: f64 = 1.4142;
const CHECKMATE_SCORE: i32 = 30000;

struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    visits: u32,
    score: f64,
    unexpanded_moves: MoveList,
    is_terminal: bool,
    move_to_node: Option<Move>,
    player_to_move: Player,
}

pub struct MCTS {
    nodes: Vec<Node>,
    root_player: Player,
    tt: Option<Arc<LockFreeTT>>,
}

use rayon::prelude::*;

impl MCTS {
    pub fn new(root_state: &Board, root_player: Player, tt: Option<Arc<LockFreeTT>>) -> Self {
        let mut root_clone = root_state.clone();
        let mut moves = Rules::generate_legal_moves(&mut root_clone, root_player);
        let mut rng = rand::thread_rng();
        moves.shuffle(&mut rng);

        let root = Node {
            parent: None,
            children: Vec::new(),
            visits: 0,
            score: 0.0,
            unexpanded_moves: moves,
            is_terminal: false,
            move_to_node: None,
            player_to_move: root_player,
        };

        Self {
            nodes: vec![root],
            root_player,
            tt,
        }
    }

    pub fn run(&mut self, root_state: &Board, iterations: usize) -> f64 {
        if iterations == 0 {
            return 0.5;
        }

        let num_threads = rayon::current_num_threads();
        let chunk_size = iterations / num_threads;
        let remainder = iterations % num_threads;

        let results: Vec<(u32, f64)> = (0..num_threads)
            .into_par_iter()
            .map(|i| {
                let count = if i < remainder {
                    chunk_size + 1
                } else {
                    chunk_size
                };
                if count == 0 {
                    return (0, 0.0);
                }

                let mut local_mcts = MCTS::new(root_state, self.root_player, self.tt.clone());
                local_mcts.execute_iterations(root_state, count);

                let root = &local_mcts.nodes[0];
                (root.visits, root.score)
            })
            .collect();

        let (total_visits, total_score) = results
            .into_iter()
            .fold((0, 0.0), |acc, x| (acc.0 + x.0, acc.1 + x.1));

        if total_visits == 0 {
            0.5
        } else {
            total_score / total_visits as f64
        }
    }

    fn execute_iterations(&mut self, root_state: &Board, iterations: usize) {
        let mut rng = rand::thread_rng();

        let mut current_state = root_state.clone();

        for _ in 0..iterations {
            let mut node_idx = 0;
            let mut current_player = self.root_player;

            let mut path_stack: Vec<(Move, UnmakeInfo)> = Vec::with_capacity(64);

            while self.nodes[node_idx].unexpanded_moves.is_empty()
                && !self.nodes[node_idx].children.is_empty()
            {
                let best_child = self.select_child(node_idx);
                node_idx = best_child;

                let mv = self.nodes[node_idx].move_to_node.as_ref().unwrap();

                let info = current_state.apply_move(mv).unwrap();
                path_stack.push((mv.clone(), info));

                current_player = current_player.opponent();
            }

            if !self.nodes[node_idx].unexpanded_moves.is_empty() {
                let mv = self.nodes[node_idx].unexpanded_moves.pop().unwrap();

                let info = current_state.apply_move(&mv).unwrap();
                path_stack.push((mv.clone(), info));

                let next_player = current_player.opponent();

                let legal_moves = Rules::generate_legal_moves(&mut current_state, next_player);
                let is_terminal = legal_moves.is_empty();

                let new_node = Node {
                    parent: Some(node_idx),
                    children: Vec::new(),
                    visits: 0,
                    score: 0.0,
                    unexpanded_moves: legal_moves,
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
                self.rollout_inplace(
                    &mut current_state,
                    current_player,
                    &mut rng,
                    &mut path_stack,
                )
            };

            self.backpropagate(node_idx, result_score);

            while let Some((mv, info)) = path_stack.pop() {
                current_state.unmake_move(&mv, info);
            }
        }
    }

    fn select_child(&self, parent_idx: usize) -> usize {
        let parent = &self.nodes[parent_idx];
        let log_n = (parent.visits as f64).ln();

        let mut best_score = -f64::INFINITY;
        let mut best_child = 0;

        let maximize = parent.player_to_move == self.root_player;

        for &child_idx in &parent.children {
            let child = &self.nodes[child_idx];
            let win_rate = if child.visits > 0 {
                child.score / child.visits as f64
            } else {
                0.0
            };

            let exploitation = if maximize { win_rate } else { 1.0 - win_rate };

            let exploration = UCT_C * (log_n / (child.visits as f64 + 1e-6)).sqrt();
            let uct_value = exploitation + exploration;

            if uct_value > best_score {
                best_score = uct_value;
                best_child = child_idx;
            }
        }
        best_child
    }

    fn rollout_inplace(
        &self,
        state: &mut Board,
        mut player: Player,
        rng: &mut rand::rngs::ThreadRng,
        stack: &mut Vec<(Move, UnmakeInfo)>,
    ) -> f64 {
        let mut depth = 0;
        const MAX_ROLLOUT_DEPTH: usize = 50;
        const VAL_KING_F: f64 = 20000.0;

        while depth < MAX_ROLLOUT_DEPTH {
            if let Some(tt) = &self.tt {
                if let Some((score, _, flag, _)) = tt.get(state.hash) {
                    if flag == Flag::Exact {
                        let normalized = (score as f64 / VAL_KING_F) / 2.0 + 0.5;
                        return normalized.max(0.0).min(1.0);
                    }
                }
            }

            let moves = Rules::generate_legal_moves(state, player);
            if moves.is_empty() {
                return self.evaluate_terminal(state, player);
            }

            let mv = moves.choose(rng).unwrap();
            let info = state.apply_move(mv).unwrap();
            stack.push((mv.clone(), info));

            player = player.opponent();
            depth += 1;
        }

        0.5
    }

    fn evaluate_terminal(&self, state: &Board, player_at_leaf: Player) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::Board;

    #[test]
    fn test_mcts_smoke() {
        let board = Board::new(2, 8);
        let mut mcts = MCTS::new(&board, Player::White, None);
        let score = mcts.run(&board, 10);
        assert!(score >= 0.0 && score <= 1.0);
    }

    #[test]
    fn test_mcts_parallel_execution() {
        let board = Board::new(2, 8);
        let mut mcts = MCTS::new(&board, Player::White, None);

        let score = mcts.run(&board, 100);
        assert!(score >= 0.0 && score <= 1.0);
    }
}
