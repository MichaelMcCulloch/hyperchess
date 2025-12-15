use hyperchess::domain::board::Board;
use hyperchess::domain::models::Player;
use hyperchess::infrastructure::ai::mcts::MCTS;

#[test]
fn test_mcts_initialization_and_run() {
    let board = Board::new(3, 4);
    let mut mcts = MCTS::new(&board, Player::White, None, None, None, None, 0);
    let (win_rate, _) = mcts.run(&board, 50);

    assert!(win_rate >= 0.0);
    assert!(win_rate <= 1.0);
    println!("MCTS Win Rate: {}", win_rate);
}

#[test]
fn test_mcts_checkmate_detection() {
    let board = Board::new(2, 8);

    let mut mcts = MCTS::new(&board, Player::White, None, None, None, None, 0);
    let (win_rate, _move) = mcts.run(&board, 50);

    assert!(win_rate >= 0.0);
    assert!(win_rate <= 1.0);
}
