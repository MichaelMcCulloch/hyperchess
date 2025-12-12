use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Board, BoardState, PieceType, Player};
use hyperchess::infrastructure::persistence::BitBoardState;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_standard_2d_chess_setup() {
    let board = Board::<BitBoardState>::new(2, 8);

    // Check Corner Rooks
    assert!(
        is_piece_at(&board, &coord(0, 0), PieceType::Rook, Player::White),
        "White Rook at (0,0)"
    );
    assert!(
        is_piece_at(&board, &coord(0, 7), PieceType::Rook, Player::White),
        "White Rook at (0,7)"
    );
    assert!(
        is_piece_at(&board, &coord(7, 0), PieceType::Rook, Player::Black),
        "Black Rook at (7,0)"
    );

    // Check King/Queen
    // White King at (0, 4)
    assert!(
        is_piece_at(&board, &coord(0, 4), PieceType::King, Player::White),
        "White King at (0,4)"
    );
    // White Queen at (0, 3)
    assert!(
        is_piece_at(&board, &coord(0, 3), PieceType::Queen, Player::White),
        "White Queen at (0,3)"
    );

    // Check Pawns
    for i in 0..8 {
        assert!(
            is_piece_at(&board, &coord(1, i), PieceType::Pawn, Player::White),
            "White Pawn at (1, {})",
            i
        );
        assert!(
            is_piece_at(&board, &coord(6, i), PieceType::Pawn, Player::Black),
            "Black Pawn at (6, {})",
            i
        );
    }

    // Check Empty Middle
    assert!(board.get_piece(&coord(3, 3)).is_none());
    assert!(board.get_piece(&coord(4, 4)).is_none());
}

#[test]
fn test_3d_setup() {
    // 3D 4x4x4
    let board = Board::<BitBoardState>::new(3, 4);

    // New Setup Logic:
    // White: x=0 (Rank), pieces at z=0.
    // Black: x=3 (Rank), pieces at z=3 (side-1).
    // King position is determined by y (file) index.
    // y = side / 2 = 2.
    // So White King at (0, 2, 0).
    // Black King at (3, 2, 3).

    // White King
    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![0, 2, 0]),
            PieceType::King,
            Player::White
        ),
        "White King at (0, 2, 0)"
    );

    // Black King
    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![3, 2, 3]),
            PieceType::King,
            Player::Black
        ),
        "Black King at (3, 2, 3)"
    );

    // Verify EMPTY elsewhere (e.g. z=1)
    assert!(
        board.get_piece(&Coordinate::new(vec![0, 2, 1])).is_none(),
        "Should be empty at z=1"
    );

    // Count total pieces?
    // Side=4.
    // White: 4 Pawns + 4 Pieces = 8.
    // Black: 4 Pawns + 4 Pieces = 8.
    // Total 16.
    // Implementation details: Board stores pieces in bitboards.
    // Check occupancy count if possible, or just trust specific checks.
}

fn is_piece_at<S: BoardState>(board: &Board<S>, c: &Coordinate, t: PieceType, p: Player) -> bool {
    if let Some(piece) = board.get_piece(c) {
        piece.piece_type == t && piece.owner == p
    } else {
        false
    }
}
