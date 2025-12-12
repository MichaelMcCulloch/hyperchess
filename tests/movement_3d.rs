use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use std::collections::HashSet;

fn coord3(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_bishop_moves_3d() {
    // 3D board, 4x4x4
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
    // Bishops in 3D: even number of non-zero displacements.
    // Dirs:
    // 1. (±1, ±1, 0)
    // 2. (±1, 0, ±1)
    // 3. (0, ±1, ±1)
    // Total dirs = 4 + 4 + 4 = 12 directions.

    // Let's check a few targets.
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 2, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(0, 0, 1)), "2D diagonal xy");
    assert!(dests.contains(&coord3(2, 1, 2)), "2D diagonal xz");
    assert!(dests.contains(&coord3(1, 2, 2)), "2D diagonal yz");

    // (2,2,2) would be (1+1, 1+1, 1+1) -> 3 non-zero displacements -> ODD -> Not a Bishop move in default "HyperChess" (usually).
    // Let's verify standard hyperchess rules for "Bishop".
    // mechanics.rs: `get_bishop_directions`: "count of non-zero elements is EVEN".
    // So (1,1,1) displacement is NOT allowed.
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
    // Rooks: 1 non-zero displacement.
    // directions: (±1, 0, 0), (0, ±1, 0), (0, 0, ±1) -> 6 dirs.

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    assert!(dests.contains(&coord3(2, 1, 1)));
    assert!(dests.contains(&coord3(1, 2, 1)));
    assert!(dests.contains(&coord3(1, 1, 2)));

    assert!(!dests.contains(&coord3(2, 2, 1))); // Diagonal
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

    // Knights: One axis ±2, one axis ±1.
    // From (0,0,0):
    // (2, 1, 0), (2, 0, 1)
    // (1, 2, 0), (0, 2, 1)
    // (1, 0, 2), (0, 1, 2)
    // Negatives are out of bounds.

    assert!(dests.contains(&coord3(2, 1, 0)));
    assert!(dests.contains(&coord3(0, 1, 2)));
    assert_eq!(dests.len(), 6);
}
