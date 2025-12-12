use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord_3d(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_super_pawn_z_axis_movement() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    // Setup White Pawn at (0, 0, 1)
    let start_pos = coord_3d(0, 0, 1);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Axis 0 (X/Rank): Allowed (+1)
    // Axis 1 (Y/File): Forbidden (Lateral)
    // Axis 2 (Z/Height): Allowed (+1)

    let move_z = moves.iter().find(|m| m.to == coord_3d(0, 0, 2));
    let move_x = moves.iter().find(|m| m.to == coord_3d(1, 0, 1));
    let move_y = moves.iter().find(|m| m.to == coord_3d(0, 1, 1));

    assert!(move_z.is_some(), "Should allow Z-axis push");
    assert!(move_x.is_some(), "Should allow X-axis push");

    // UPDATED ASSERTION: Lateral push (Y-axis) should now be forbidden
    assert!(
        move_y.is_none(),
        "Should NOT allow Y-axis push (Lateral Forbidden)"
    );
}

#[test]
fn test_super_pawn_capture_multidimensional() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    // White Pawn at (1, 1, 1)
    let p1 = coord_3d(1, 1, 1);
    board
        .set_piece(
            &p1,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    // Black Pawn at (2, 2, 1)
    // Capture via: Move Axis 0 (+1) to X=2, Capture Axis 1 (+1) to Y=2.
    // Result: (2, 2, 1). This is valid.

    let target = coord_3d(2, 2, 1);
    board
        .set_piece(
            &target,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    let capture = moves.iter().find(|m| m.to == target);

    assert!(
        capture.is_some(),
        "Should capture diagonally across dimensions"
    );
}
