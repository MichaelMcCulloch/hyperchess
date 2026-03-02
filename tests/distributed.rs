//! Tests for distributed infrastructure: serde round-trips, deterministic Zobrist keys,
//! search_subset correctness, partition_moves, and RedisSession logic.

use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::game::Game;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use hyperchess::domain::zobrist::ZobristKeys;
use hyperchess::infrastructure::ai::MinimaxBot;
use std::time::Duration;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x as u8, y as u8])
}

// ────────────────────────────────────────────────
// Board serde round-trip
// ────────────────────────────────────────────────

#[test]
fn test_board_bincode_roundtrip_empty() {
    let board = Board::new_empty(2, 8);
    let bytes = bincode::serialize(&board).expect("serialize");
    let restored: Board = bincode::deserialize(&bytes).expect("deserialize");

    assert_eq!(restored.dimension(), board.dimension());
    assert_eq!(restored.side(), board.side());
    assert_eq!(restored.total_cells(), board.total_cells());
    assert_eq!(restored.state.hash, board.state.hash);
}

#[test]
fn test_board_bincode_roundtrip_standard() {
    let board = Board::new(2, 8);
    let bytes = bincode::serialize(&board).expect("serialize");
    let restored: Board = bincode::deserialize(&bytes).expect("deserialize");

    assert_eq!(restored.dimension(), 2);
    assert_eq!(restored.side(), 8);
    assert_eq!(restored.state.hash, board.state.hash);
    assert_eq!(restored.state.castling_rights, board.state.castling_rights);

    // Verify pieces survived the round-trip
    let piece_at_e1 = restored.get_piece(&coord(0, 4));
    assert!(piece_at_e1.is_some());
    let king = piece_at_e1.unwrap();
    assert_eq!(king.piece_type, PieceType::King);
    assert_eq!(king.owner, Player::White);
}

#[test]
fn test_board_bincode_roundtrip_3d() {
    let board = Board::new_empty(3, 4);
    let bytes = bincode::serialize(&board).expect("serialize");
    let restored: Board = bincode::deserialize(&bytes).expect("deserialize");

    assert_eq!(restored.dimension(), 3);
    assert_eq!(restored.side(), 4);
    assert_eq!(restored.total_cells(), 64);
}

#[test]
fn test_board_bincode_roundtrip_with_fen() {
    // Sicilian defense position
    let board = Board::from_fen("rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2")
        .expect("valid FEN");
    let bytes = bincode::serialize(&board).expect("serialize");
    let restored: Board = bincode::deserialize(&bytes).expect("deserialize");

    assert_eq!(restored.state.hash, board.state.hash);
    assert_eq!(
        restored.state.en_passant_target,
        board.state.en_passant_target
    );
}

// ────────────────────────────────────────────────
// Game serde round-trip
// ────────────────────────────────────────────────

#[test]
fn test_game_bincode_roundtrip() {
    let board = Board::new(2, 8);
    let game = Game::new(board);
    let bytes = bincode::serialize(&game).expect("serialize");
    let restored: Game = bincode::deserialize(&bytes).expect("deserialize");

    assert_eq!(restored.current_turn(), Player::White);
}

#[test]
fn test_game_bincode_roundtrip_after_moves() {
    let board = Board::new(2, 8);
    let mut game = Game::new(board);
    game.start();

    // Play e2-e4
    let e2e4 = hyperchess::domain::models::Move {
        from: coord(1, 4),
        to: coord(3, 4),
        promotion: None,
    };
    let _ = game.play_turn(e2e4);

    let bytes = bincode::serialize(&game).expect("serialize");
    let restored: Game = bincode::deserialize(&bytes).expect("deserialize");

    assert_eq!(restored.current_turn(), Player::Black);
}

// ────────────────────────────────────────────────
// ZobristKeys determinism
// ────────────────────────────────────────────────

#[test]
fn test_zobrist_deterministic_same_total_cells() {
    let keys_a = ZobristKeys::new(64);
    let keys_b = ZobristKeys::new(64);

    assert_eq!(keys_a.piece_keys, keys_b.piece_keys);
    assert_eq!(keys_a.black_to_move, keys_b.black_to_move);
    assert_eq!(keys_a.en_passant_keys, keys_b.en_passant_keys);
    assert_eq!(keys_a.castling_keys, keys_b.castling_keys);
}

#[test]
fn test_zobrist_different_for_different_sizes() {
    let keys_64 = ZobristKeys::new(64);
    let keys_16 = ZobristKeys::new(16);

    // Different total_cells must produce different keys
    assert_ne!(keys_64.black_to_move, keys_16.black_to_move);
}

#[test]
fn test_zobrist_determinism_across_threads() {
    let handles: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let keys = ZobristKeys::new(64);
                (keys.piece_keys, keys.black_to_move)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All threads must produce identical keys
    for r in &results[1..] {
        assert_eq!(r.0, results[0].0);
        assert_eq!(r.1, results[0].1);
    }
}

// ────────────────────────────────────────────────
// Board hash consistency after serde
// ────────────────────────────────────────────────

#[test]
fn test_board_hash_matches_after_serde() {
    let board = Board::new(2, 8);
    let original_hash = board.state.hash;

    let bytes = bincode::serialize(&board).expect("serialize");
    let restored: Board = bincode::deserialize(&bytes).expect("deserialize");

    // The deserialized board reconstructs ZobristKeys from the same seed,
    // so its hash (stored in state) should match.
    assert_eq!(restored.state.hash, original_hash);
}

// ────────────────────────────────────────────────
// MinimaxBot::new_from_params
// ────────────────────────────────────────────────

#[test]
fn test_minimax_new_from_params() {
    let bot = MinimaxBot::new_from_params(6, Duration::from_secs(60), 512, 2);
    // Bot should be constructable without panicking
    // We can't easily inspect private fields, but this verifies construction works
    drop(bot);
}

#[test]
fn test_minimax_new_from_params_min_threads() {
    // 0 threads should be clamped to 1
    let bot = MinimaxBot::new_from_params(4, Duration::from_secs(30), 256, 0);
    drop(bot);
}

// ────────────────────────────────────────────────
// MinimaxBot::search_subset
// ────────────────────────────────────────────────

#[test]
fn test_search_subset_returns_from_given_moves() {
    let board = Board::new(2, 8);
    let moves = Rules::generate_legal_moves(&mut board.clone(), Player::White);

    // Give it only a subset of moves (first 5)
    let subset: Vec<_> = moves.iter().take(5).cloned().collect();
    let subset_clone = subset.clone();

    let mut bot = MinimaxBot::new_from_params(2, Duration::from_secs(5), 256, 1);
    let (best_move, _score, nodes, _completed) = bot.search_subset(&board, Player::White, subset);

    // The returned move must be one of the subset we provided
    assert!(
        subset_clone.contains(&best_move),
        "search_subset returned a move not in the given subset"
    );
    assert!(nodes > 0, "search should have explored at least some nodes");
}

#[test]
fn test_search_subset_single_move() {
    let board = Board::new(2, 8);
    let moves = Rules::generate_legal_moves(&mut board.clone(), Player::White);
    let single = vec![moves[0].clone()];
    let expected = moves[0].clone();

    let mut bot = MinimaxBot::new_from_params(2, Duration::from_secs(5), 256, 1);
    let (best_move, _score, _nodes, _completed) = bot.search_subset(&board, Player::White, single);

    // With only one move available, it must return that move
    assert_eq!(best_move, expected);
}

#[test]
fn test_search_subset_finds_capture() {
    // Set up a position where one move is clearly best: capturing a free queen
    let mut board = Board::new_empty(2, 8);

    // White king on e1, white rook on a1
    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 0),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    // Black king on e8, black queen hanging on a8
    board
        .set_piece(
            &coord(7, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(7, 0),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::Black,
            },
        )
        .unwrap();

    let all_moves = Rules::generate_legal_moves(&mut board.clone(), Player::White);
    let all_moves_vec: Vec<_> = all_moves.to_vec();

    let mut bot = MinimaxBot::new_from_params(3, Duration::from_secs(5), 256, 1);
    let (best_move, score, _nodes, _completed) =
        bot.search_subset(&board, Player::White, all_moves_vec);

    // Should find the rook captures queen (Rxa8)
    assert_eq!(
        best_move.to,
        coord(7, 0),
        "Should capture the hanging queen"
    );
    assert!(score > 0, "Capturing a queen should yield positive score");
}

// ────────────────────────────────────────────────
// Move serde round-trip (used for gRPC transport)
// ────────────────────────────────────────────────

#[test]
fn test_move_bincode_roundtrip() {
    let mv = hyperchess::domain::models::Move {
        from: coord(1, 4),
        to: coord(3, 4),
        promotion: None,
    };
    let bytes = bincode::serialize(&mv).expect("serialize");
    let restored: hyperchess::domain::models::Move =
        bincode::deserialize(&bytes).expect("deserialize");
    assert_eq!(restored.from, mv.from);
    assert_eq!(restored.to, mv.to);
    assert_eq!(restored.promotion, mv.promotion);
}

#[test]
fn test_move_vec_bincode_roundtrip() {
    let board = Board::new(2, 8);
    let moves = Rules::generate_legal_moves(&mut board.clone(), Player::White);
    let moves_vec: Vec<_> = moves.to_vec();

    let bytes = bincode::serialize(&moves_vec).expect("serialize");
    let restored: Vec<hyperchess::domain::models::Move> =
        bincode::deserialize(&bytes).expect("deserialize");

    assert_eq!(restored.len(), moves_vec.len());
    for (a, b) in restored.iter().zip(moves_vec.iter()) {
        assert_eq!(a.from, b.from);
        assert_eq!(a.to, b.to);
    }
}
