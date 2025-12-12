use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord_2d(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

fn coord_3d(x: usize, y: usize, z: usize) -> Coordinate {
    Coordinate::new(vec![x, y, z])
}

#[test]
fn test_castling_standard_8x8() {
    let side = 8;
    let dim = 2;
    let mut board = Board::new_empty(dim, side);
    board.castling_rights = 0xF;

    let king_pos = coord_2d(0, 4);
    let rook_pos = coord_2d(0, 7);

    board
        .set_piece(
            &king_pos,
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &rook_pos,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let castling_target = coord_2d(0, 6);
    let castle_move = moves
        .iter()
        .find(|m| m.to == castling_target && m.from == king_pos);

    assert!(castle_move.is_some(), "Should allow castling on 8x8 board");

    board.apply_move(castle_move.unwrap()).unwrap();

    assert!(board.get_piece(&castling_target).is_some());

    let rook_coord = coord_2d(0, 5);
    let rook_piece = board.get_piece(&rook_coord);
    assert!(rook_piece.is_some(), "Rook should be at F1 (0,5)");
    assert_eq!(rook_piece.unwrap().piece_type, PieceType::Rook);
}

#[test]
fn test_castling_3d_blocked() {
    let side = 8;
    let dim = 3;
    let mut board = Board::new_empty(dim, side);
    board.castling_rights = 0xF;

    let king_pos = coord_3d(0, 4, 0);

    board
        .set_piece(
            &king_pos,
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord_3d(0, 7, 0),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord_3d(0, 5, 0),
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let castling_target = coord_3d(0, 6, 0);
    let castle_move = moves
        .iter()
        .find(|m| m.to == castling_target && m.from == king_pos);

    if castle_move.is_some() {
        eprintln!("Castle move found: {:?}", castle_move.unwrap());
        eprintln!("All moves: {:?}", moves);
    }

    assert!(
        castle_move.is_none(),
        "Castling should be blocked on 3D board path"
    );
}
