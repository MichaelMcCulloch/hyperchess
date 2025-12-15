use hyperchess::config::{AppConfig, MctsConfig};
use hyperchess::domain::board::Board;
use hyperchess::domain::models::Player;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::mcts_bot::MctsBot;

#[test]
fn test_mcts_minimax_integration_mate_in_one() {
    let mut board = Board::new(2, 8);

    let mcts_config = MctsConfig {
        depth: 10,
        iterations: 100,
        iter_per_thread: 10.0,
        prior_weight: 1.41,
        rollout_depth: 0,
    };

    let mut bot = MctsBot::new(&mcts_config, 1000);

    let mv = bot.get_move(&board, Player::White);
    assert!(mv.is_some());
}
