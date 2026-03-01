//! Regression test for a blunder found in a game vs Stockfish 20.
//!
//! After move 29 (e4xf5, Rc5-e5+) the engine played Ke3-d4?? walking into
//! a forced mate: Qg2-e4+ Kd4-c3 Rf8-c8+ Qb3-c4(forced) Qe4xc4#.
//! The engine must choose Ke3-f4 or Ke3-d3 to avoid the forced mate.

use hyperchess::config::AppConfig;
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Move, Player};
use hyperchess::domain::rules::Rules;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;

/// Parse a UCI move string (e.g. "e2e4") into an internal Move.
fn uci(s: &str) -> Move {
    let bytes = s.as_bytes();
    let from_file = bytes[0] - b'a';
    let from_rank = bytes[1] - b'1';
    let to_file = bytes[2] - b'a';
    let to_rank = bytes[3] - b'1';
    Move {
        from: Coordinate::new(vec![from_rank, from_file]),
        to: Coordinate::new(vec![to_rank, to_file]),
        promotion: None,
    }
}

/// Replay a sequence of UCI moves on the board.
fn replay(board: &mut Board, moves: &[&str]) {
    for (i, m) in moves.iter().enumerate() {
        let mv = uci(m);
        board.apply_move(&mv).unwrap_or_else(|e| {
            panic!("Failed to apply move {} at ply {}: {:?}", m, i, e);
        });
    }
}

/// After move 29, white is in check from Re5 and has 3 king moves: Kf4, Kd3, Kd4.
/// Kd4 leads to forced mate in 5 plies. The engine must not choose it.
#[test]
fn test_no_blunder_allows_forced_mate() {
    let moves = [
        "e2e4", "e7e5", "g1f3", "b8c6", "b1c3", "g8f6", "f1b5", "c6d4", "b5a4", "c7c6", "d2d3",
        "a7a5", "f3e5", "d7d6", "e5f3", "d4f3", "d1f3", "c8g4", "f3e3", "b7b5", "c3b5", "c6b5",
        "a4b5", "g4d7", "a2a4", "f8e7", "e3d4", "e8g8", "c1d2", "f6e8", "b5d7", "d8d7", "f2f4",
        "e8f6", "c2c4", "d6d5", "c4d5", "a8c8", "d4b6", "e7c5", "b6a5", "f6d5", "d3d4", "d5f4",
        "d4c5", "c8c5", "a5b6", "f4g2", "e1e2", "d7g4", "e2f2", "g2h4", "b6b3", "g4g2", "f2e3",
        "h4f5", // move 29
        "e4f5", "c5e5",
    ];

    let mut board = Board::new(2, 8);
    replay(&mut board, &moves);

    // White to move, king on e3, in check from Re5
    let white_king = board.get_king_coordinate(Player::White).unwrap();
    assert_eq!(
        white_king,
        Coordinate::new(vec![2, 4]),
        "White king should be on e3"
    );
    assert!(
        Rules::is_square_attacked(&board, &white_king, Player::Black),
        "White should be in check"
    );

    // White has 3 legal moves: Kf4, Kd3, Kd4
    let legal = Rules::generate_legal_moves(&mut board.clone(), Player::White);
    assert_eq!(
        legal.len(),
        3,
        "White should have exactly 3 legal king moves"
    );

    // The engine with depth >= 5 should see the forced mate after Kd4
    // and avoid it. Depth 6 gives a safety margin.
    let mut config = AppConfig::default();
    config.minimax.depth = 6;
    config.compute.minutes = 1;
    config.compute.concurrency = 1;
    config.compute.memory = 64;

    let mut bot = MinimaxBot::new(&config, 2, 8);
    let chosen = bot
        .get_move(&board, Player::White)
        .expect("White should have legal moves");

    // The engine must NOT play Ke3-d4 which leads to forced mate
    let blunder = uci("e3d4");
    assert_ne!(
        (chosen.from.clone(), chosen.to.clone()),
        (blunder.from, blunder.to),
        "Engine must not play Ke3-d4 which leads to forced mate \
         (Qg2-e4+ Kd4-c3 Rf8-c8+ Qb3-c4 Qe4xc4#)"
    );
}
