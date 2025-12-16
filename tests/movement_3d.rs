use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use std::collections::HashSet;

fn coord3(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x as u8, y as u8, z as u8])
}

#[test]
fn test_bishop_moves_3d() {
    let mut board = Board::new_empty(3, 4);
    let pos = coord3(1, 1, 1);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 2, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(0, 0, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(2, 1, 2)), "2D diagonal xz");
    assert!(dests.contains(&coord3(1, 2, 2)), "2D diagonal yz");

    assert!(
        !dests.contains(&coord3(2, 2, 2)),
        "3D space diagonal forbidden for Bishop"
    );
}

#[test]
fn test_rook_moves_3d() {
    let mut board = Board::new_empty(3, 4);
    let pos = coord3(1, 1, 1);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 1, 1)));
    assert!(dests.contains(&coord3(1, 2, 1)));
    assert!(dests.contains(&coord3(1, 1, 2)));

    assert!(!dests.contains(&coord3(2, 2, 1)));
}

#[test]
fn test_knight_moves_3d() {
    let mut board = Board::new_empty(3, 4);
    let pos = coord3(0, 0, 0);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Knight,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 1, 0)));
    assert!(dests.contains(&coord3(0, 1, 2)));
    assert_eq!(dests.len(), 6);
}
