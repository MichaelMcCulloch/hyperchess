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

    // Setup White Pawn at (0, 0, 1) - Rank 1 on Z-axis?
    // Note: Standard chess setup for White is at Z=0?
    // Let's assume an arbitrary pawn placement.
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

    // Expect Move to (0, 0, 2) (Z-push)
    // AND moves along other axes if "forward" is defined per axis.
    // My implementation: `movement_axis` iterates 0..dim.
    // For Axis 0 (X? Rank?): Forward is +1.
    // For Axis 1 (Y? File?): Forward is +1.
    // For Axis 2 (Z?): Forward is +1.

    // So Pawn at (0,0,1) can move:
    // Axis 0: None? (0+1=1). (1, 0, 1).
    // Axis 1: (0, 1, 1).
    // Axis 2: (0, 0, 2).

    let move_z = moves.iter().find(|m| m.to == coord_3d(0, 0, 2));
    let move_x = moves.iter().find(|m| m.to == coord_3d(1, 0, 1));
    let move_y = moves.iter().find(|m| m.to == coord_3d(0, 1, 1));

    assert!(move_z.is_some(), "Should allow Z-axis push");
    assert!(move_x.is_some(), "Should allow X-axis push (Super Pawn)");
    assert!(move_y.is_some(), "Should allow Y-axis push (Super Pawn)");
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

    // Black Pawn at (2, 2, 1) -> Capture via Axis 0 (X) + Axis 1 (Y) diagonal?
    // Rules: "Combine forward on movement_axis with +/- 1 on capture_axis"
    // Valid captures from (1,1,1):
    // Move Axis 0 (+1 -> 2,1,1). Capture Axis 1 (+/-1). Targets: (2, 2, 1) and (2, 0, 1).
    // Move Axis 0 (+1 -> 2,1,1). Capture Axis 2 (+/-1). Targets: (2, 1, 2) and (2, 1, 0).
    // ... plus other movement axes.

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
