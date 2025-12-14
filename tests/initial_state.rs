use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{PieceType, Player};

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_standard_2d_chess_setup() {
    let board = Board::new(2, 8);

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

    assert!(
        is_piece_at(&board, &coord(0, 4), PieceType::King, Player::White),
        "White King at (0,4)"
    );

    assert!(
        is_piece_at(&board, &coord(0, 3), PieceType::Queen, Player::White),
        "White Queen at (0,3)"
    );

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

    assert!(board.get_piece(&coord(3, 3)).is_none());
    assert!(board.get_piece(&coord(4, 4)).is_none());
}

#[test]
fn test_3d_setup() {
    let board = Board::new(3, 4);

    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![0, 2, 0]),
            PieceType::King,
            Player::White
        ),
        "White King at (0, 2, 0)"
    );

    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![3, 2, 3]),
            PieceType::King,
            Player::Black
        ),
        "Black King at (3, 2, 3)"
    );

    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![1, 0, 1]),
            PieceType::Pawn,
            Player::White
        ),
        "White Pawn at (1, 0, 1)"
    );

    assert!(
        is_piece_at(
            &board,
            &Coordinate::new(vec![2, 0, 2]),
            PieceType::Pawn,
            Player::Black
        ),
        "Black Pawn at (2, 0, 2)"
    );

    assert!(
        board.get_piece(&Coordinate::new(vec![0, 2, 1])).is_none(),
        "Should be empty at x=0, z=1"
    );
}

fn is_piece_at(board: &Board, c: &Coordinate, t: PieceType, p: Player) -> bool {
    if let Some(piece) = board.get_piece(c) {
        piece.piece_type == t && piece.owner == p
    } else {
        false
    }
}
