use hyperchess::domain::models::{BoardState, Player};
use hyperchess::infrastructure::mechanics::MoveGenerator;
use hyperchess::infrastructure::persistence::{BitBoardState, index_to_coords};

#[test]
fn test_initial_board_setup_and_pawn_move() {
    let dim = 2; // Simple 2D chess
    let side = 8;
    let mut board = BitBoardState::new_empty(dim, side);

    // We need to populate the board first. BitBoardState::new returns EMPTY board now (based on persistence.rs change).
    // So we must manually setup pieces or assume GameService setup.
    // Wait, MinimaxBot expects pieces.
    // Let's manually place a White Pawn at index 8 (Row 1, Col 0) and check moves.

    use hyperchess::domain::coordinate::Coordinate;
    use hyperchess::domain::models::{Piece, PieceType};

    let pawn = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    let start_idx = 8; // (0, 1) in 8x8
    let start_coord = Coordinate::new(index_to_coords(start_idx, dim, side));

    board.set_piece(&start_coord, pawn).unwrap();

    // Add Kings (Required for move legality check)
    let w_king = Piece {
        piece_type: PieceType::King,
        owner: Player::White,
    };
    let b_king = Piece {
        piece_type: PieceType::King,
        owner: Player::Black,
    };

    // Place Kings far away
    let w_king_coord = Coordinate::new(vec![0, 0]);
    let b_king_coord = Coordinate::new(vec![7, 7]);

    board.set_piece(&w_king_coord, w_king).unwrap();
    board.set_piece(&b_king_coord, b_king).unwrap();

    // Generate moves for White
    let moves = MoveGenerator::generate_legal_moves(&board, Player::White);

    assert!(!moves.is_empty(), "Should generate moves");

    // Find pawn move
    let pawn_move = moves
        .iter()
        .find(|m| m.from == start_coord)
        .expect("Should find pawn move")
        .clone();

    println!("Applying move: {:?}", pawn_move);

    board.apply_move(&pawn_move).unwrap();

    assert!(
        board.get_piece(&start_coord).is_none(),
        "Start square should be empty"
    );
    assert!(
        board.get_piece(&pawn_move.to).is_some(),
        "End square should be occupied"
    );
}
