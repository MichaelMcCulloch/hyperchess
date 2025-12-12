use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

#[test]
fn test_5d_bishop_movement() {
    let dimension = 5;
    let side = 3;
    let mut board = Board::new_empty(dimension, side);

    let center = Coordinate::new(vec![1, 1, 1, 1, 1]);
    board
        .set_piece(
            &center,
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    for m in moves {
        let diff = diff_coords(&center, &m.to);
        let non_zeros = diff.iter().filter(|&&d| d != 0).count();
        assert!(non_zeros > 0, "Must move");
        assert_eq!(
            non_zeros % 2,
            0,
            "Bishop 5D move must have even number of coordinate changes. Found move to {:?} with {} changes",
            m.to,
            non_zeros
        );
    }
}

#[test]
fn test_5d_rook_movement() {
    let dimension = 5;
    let side = 3;
    let mut board = Board::new_empty(dimension, side);

    let center = Coordinate::new(vec![1, 1, 1, 1, 1]);
    board
        .set_piece(
            &center,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    for m in moves {
        let diff = diff_coords(&center, &m.to);
        let non_zeros = diff.iter().filter(|&&d| d != 0).count();
        assert_eq!(
            non_zeros, 1,
            "Rook 5D move must allow movement on exactly one axis"
        );
    }
}

#[test]
fn test_5d_knight_movement() {
    let dimension = 5;
    let side = 5;
    let mut board = Board::new_empty(dimension, side);

    let center = Coordinate::new(vec![2, 2, 2, 2, 2]);
    board
        .set_piece(
            &center,
            Piece {
                piece_type: PieceType::Knight,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    for m in moves {
        let diff = diff_coords(&center, &m.to);
        let non_zeros = diff.iter().filter(|&&d| d != 0).count();
        assert_eq!(non_zeros, 2, "Knight 5D move changes exactly 2 coords");

        let abs_sum: usize = diff.iter().map(|&d| d.abs() as usize).sum();
        assert_eq!(
            abs_sum, 3,
            "Knight move is +/-2 and +/-1 => sum of abs diffs is 3"
        );
    }
}

fn diff_coords(c1: &Coordinate, c2: &Coordinate) -> Vec<isize> {
    c1.values
        .iter()
        .zip(c2.values.iter())
        .map(|(a, b)| *a as isize - *b as isize)
        .collect()
}
