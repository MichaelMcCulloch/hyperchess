use crate::domain::models::{BoardState, Move, Player};
use crate::infrastructure::ai::transposition::{Flag, LockFreeTT};
use crate::infrastructure::mechanics::MoveGenerator;
use crate::infrastructure::persistence::BitBoardState;
use rand::seq::SliceRandom;
use std::sync::Arc;

use std::f64;

const UCT_C: f64 = 1.4142; // Sqrt(2)

struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    visits: u32,
    score: f64, // Wins from the perspective of the player at the parent node?
    // Typically MCTS stores wins for the player who JUST moved to get here?
    // Or we can store wins for Root's player.

    // State info
    unexpanded_moves: Vec<Move>,
    is_terminal: bool,
    // Note: We don't store the full BoardState in every node significantly reduces memory,
    // but we need to re-play moves or clone responsibly.
    // For a leaf evaluator, the tree won't be huge. We can store board if needed, or just store the move.
    // Storing move is enough if we traverse down from root state.
    move_to_node: Option<Move>,
    player_to_move: Player, // Player who needs to move from this state
}

pub struct MCTS {
    nodes: Vec<Node>,
    root_player: Player,
    tt: Option<Arc<LockFreeTT>>,
}

impl MCTS {
    pub fn new(
        root_state: &BitBoardState,
        root_player: Player,
        tt: Option<Arc<LockFreeTT>>,
    ) -> Self {
        let mut moves = MoveGenerator::generate_legal_moves(root_state, root_player);
        // Shuffle for randomness in expansion
        let mut rng = rand::thread_rng();
        moves.shuffle(&mut rng);

        let root = Node {
            parent: None,
            children: Vec::new(),
            visits: 0,
            score: 0.0,
            unexpanded_moves: moves,
            is_terminal: false, // Assumed passed state is not terminal for simplicity, or check?
            move_to_node: None,
            player_to_move: root_player,
        };

        Self {
            nodes: vec![root],
            root_player,
            tt,
        }
    }

    pub fn run(&mut self, root_state: &BitBoardState, iterations: usize) -> f64 {
        let mut rng = rand::thread_rng();

        for _ in 0..iterations {
            let mut node_idx = 0;
            let mut current_state = root_state.clone();
            let mut current_player = self.root_player;

            // 1. Selection
            // Traverse down until we find a node that has unexpanded moves or is terminal
            while self.nodes[node_idx].unexpanded_moves.is_empty()
                && !self.nodes[node_idx].children.is_empty()
            {
                // Select best child
                let best_child = self.select_child(node_idx);
                node_idx = best_child;

                let mv = self.nodes[node_idx].move_to_node.as_ref().unwrap();
                current_state.apply_move(mv).unwrap(); // Should be valid
                current_player = current_player.opponent();
            }

            // 2. Expansion
            // If there are unexpanded moves, pick one and create a child
            if !self.nodes[node_idx].unexpanded_moves.is_empty() {
                let mv = self.nodes[node_idx].unexpanded_moves.pop().unwrap();

                // Clone state to apply move
                let mut next_state = current_state.clone();
                next_state.apply_move(&mv).unwrap();
                let next_player = current_player.opponent();

                let legal_moves = MoveGenerator::generate_legal_moves(&next_state, next_player);
                let is_terminal = legal_moves.is_empty(); // Check mate/stalemate broadly

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
                current_state = next_state;
                current_player = next_player;
            }

            // 3. Simulation
            // Random rollout
            let result_score = if self.nodes[node_idx].is_terminal {
                // Already terminal
                self.evaluate_terminal(&current_state, current_player)
            } else {
                self.rollout(&mut current_state, current_player, &mut rng)
            };

            // 4. Backpropagation
            self.backpropagate(node_idx, result_score);
        }

        // Return win rate of root
        let root = &self.nodes[0];
        if root.visits == 0 {
            0.0
        } else {
            root.score / root.visits as f64
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

            // Perspective correction:
            // Score is "Wins for Root". Range [0, 1].
            // If Maximize (Root's turn): We want high score (1.0).
            // If Minimize (Opponent's turn): We want to minimize Root score (make it 0.0), which means maximizing (1.0 - score).

            let exploitation = if maximize { win_rate } else { 1.0 - win_rate };

            let exploration = UCT_C * (log_n / (child.visits as f64 + 1e-6)).sqrt(); // Avoid div by zero
            let uct_value = exploitation + exploration;

            if uct_value > best_score {
                best_score = uct_value;
                best_child = child_idx;
            }
        }
        best_child
    }

    fn rollout(
        &self,
        state: &mut BitBoardState,
        mut player: Player,
        rng: &mut rand::rngs::ThreadRng,
    ) -> f64 {
        let mut depth = 0;
        const MAX_ROLLOUT_DEPTH: usize = 50;
        const VAL_KING_F: f64 = 20000.0;

        while depth < MAX_ROLLOUT_DEPTH {
            // Check TT logic
            if let Some(tt) = &self.tt {
                if let Some((score, _, flag, _)) = tt.get(state.hash) {
                    if flag == Flag::Exact {
                        let normalized = (score as f64 / VAL_KING_F) / 2.0 + 0.5;
                        // Clamp [0, 1]
                        return normalized.max(0.0).min(1.0);
                    }
                }
            }

            let moves = MoveGenerator::generate_legal_moves(state, player);
            if moves.is_empty() {
                return self.evaluate_terminal(state, player);
            }

            // Heuristic? Capture preferred? For now, completely random.
            let mv = moves.choose(rng).unwrap();
            state.apply_move(mv).unwrap();
            player = player.opponent();
            depth += 1;
        }

        // If not terminal, return heuristic eval (normalized 0 to 1) or 0.5 (draw)
        0.5 // Drawish / unknown
    }

    fn evaluate_terminal(&self, state: &BitBoardState, player_at_leaf: Player) -> f64 {
        if let Some(king_pos) = state.get_king_coordinate(player_at_leaf) {
            if MoveGenerator::is_square_attacked(state, &king_pos, player_at_leaf.opponent()) {
                if player_at_leaf == self.root_player {
                    return 0.0; // Root lost (Checkmate)
                } else {
                    return 1.0; // Root won (Opponent Checkmated)
                }
            }
        }
        0.5 // Stalemate/Draw
    }

    fn backpropagate(&mut self, mut node_idx: usize, score: f64) {
        // Score is usually from perspective of Root Player?
        // Or "win/loss/draw".
        // Let's say Score is 1.0 (Win for Root), -1.0 (Loss for Root).

        loop {
            let node = &mut self.nodes[node_idx];
            node.visits += 1;

            // If we store "Score for Root Player", we just add it?
            // Yes.
            // But for UCT selection, we need to correct for perspective?
            // If `Node` tracks `player_to_move`, that is the player who needs to CHOOSE a child.
            // If I am White, I want a child with high White Win Rate.
            // If I am Black, I want a child with high Black Win Rate (== low White Win Rate).
            // So if `score` is "White Wins", Black chooses child with LOWEST score?
            // OR we flip score for UCT.

            // Let's implement Backprop adding score.
            // And in Select, handle Min/Max UCT if needed using `player_to_move`.

            node.score += score;

            if let Some(parent) = node.parent {
                node_idx = parent;
            } else {
                break;
            }
        }
    }
}
