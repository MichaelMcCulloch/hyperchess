use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord_3d(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_promotion_conditions_3d_white() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    // Setup White Pawn at (6, 0, 7)
    // Moving to (7, 0, 7) should PROMOTE because:
    // Axis 0 (Rank) -> 7 (Max)
    // Axis 1 (File) -> 0 (Irrelevant for far-side check, but valid pos)
    // Axis 2 (Height) -> 7 (Max)

    // In the new rule: Promotion happens if ALL non-file axes are at limit.
    // White moves +1 on Axis 0.
    // Target is (7, 0, 7).
    // Axis 0 is 7 (Max). Axis 1 (File) ignored. Axis 2 is 7 (Max).
    // result: PROMOTE.

    let start_pos = coord_3d(6, 0, 7);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(7, 0, 7) && m.promotion == Some(PieceType::Queen));

    assert!(promo_move.is_some(), "Should promote at (7, 0, 7)");
}

#[test]
fn test_no_promotion_partial_far_side_white() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    // Setup White Pawn at (6, 0, 0)
    // Moves to (7, 0, 0).
    // Axis 0 -> 7 (Max) - Met.
    // Axis 2 -> 0 (Min) - Not Max.
    // Result: NO PROMOTION.

    let start_pos = coord_3d(6, 0, 0);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    // Check for plain move
    let plain_move = moves
        .iter()
        .find(|m| m.to == coord_3d(7, 0, 0) && m.promotion.is_none());
    assert!(plain_move.is_some(), "Should be a normal move");

    // Check for promotion move (should be absent)
    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(7, 0, 0) && m.promotion == Some(PieceType::Queen));
    assert!(
        promo_move.is_none(),
        "Should NOT promote at (7, 0, 0) if Z is not max"
    );
}

#[test]
fn test_promotion_conditions_3d_black() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    // Setup Black Pawn at (1, 0, 0)
    // Moves to (0, 0, 0).
    // Axis 0 -> 0 (Min).
    // Axis 2 -> 0 (Min).
    // Result: PROMOTE.

    let start_pos = coord_3d(1, 0, 0);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::Black);
    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(0, 0, 0) && m.promotion == Some(PieceType::Queen));

    assert!(promo_move.is_some(), "Black should promote at (0, 0, 0)");
}

#[test]
fn test_no_promotion_partial_black() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);

    // Black Pawn at (1, 0, 7). Moves to (0, 0, 7).
    // Axis 0 -> 0 (Min).
    // Axis 2 -> 7 (Max). Not 0.
    // Result: NO PROMOTION.

    let start_pos = coord_3d(1, 0, 7);
    board
        .set_piece(
            &start_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::Black);
    let promo_move = moves
        .iter()
        .find(|m| m.to == coord_3d(0, 0, 7) && m.promotion.is_some());

    assert!(
        promo_move.is_none(),
        "Black should NOT promote at (0, 0, 7)"
    );
}
