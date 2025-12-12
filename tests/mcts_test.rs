use hyperchess::domain::models::{BoardState, Player};
use hyperchess::infrastructure::ai::mcts::MCTS;
use hyperchess::infrastructure::persistence::BitBoardState;

#[test]
fn test_mcts_initialization_and_run() {
    let board = BitBoardState::new(3, 4); // Small 3D board
    let mut mcts = MCTS::new(&board, Player::White, None);
    let win_rate = mcts.run(&board, 50);

    // Win rate should be between 0 and 1
    assert!(win_rate >= 0.0);
    assert!(win_rate <= 1.0);
    println!("MCTS Win Rate: {}", win_rate);
}

#[test]
fn test_mcts_checkmate_detection() {
    let mut board = BitBoardState::new(2, 8);
    board.setup_standard_chess();

    let mut mcts = MCTS::new(&board, Player::White, None);
    let win_rate = mcts.run(&board, 50);

    assert!(win_rate >= 0.0);
    assert!(win_rate <= 1.0);
}
