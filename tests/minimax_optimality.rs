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
