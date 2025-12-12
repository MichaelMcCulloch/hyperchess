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

    let plain_move = moves
        .iter()
        .find(|m| m.to == coord_3d(7, 0, 0) && m.promotion.is_none());
    assert!(plain_move.is_some(), "Should be a normal move");

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
