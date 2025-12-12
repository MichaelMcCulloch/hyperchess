use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_en_passant() {
    let mut board = Board::new_empty(2, 8);

    board
        .set_piece(
            &coord(4, 4),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(6, 5),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let move_black = hyperchess::domain::models::Move {
        from: coord(6, 5),
        to: coord(4, 5),
        promotion: None,
    };
    board.apply_move(&move_black).unwrap();

    let ep_target_idx = board.coords_to_index(&[5, 5]).unwrap();
    let ep_victim_idx = board.coords_to_index(&[4, 5]).unwrap();
    assert_eq!(
        board.en_passant_target,
        Some((ep_target_idx, ep_victim_idx)),
        "EP Target/Victim tuple should be set"
    );

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let ep_move = moves.iter().find(|m| m.to == coord(5, 5));

    assert!(
        ep_move.is_some(),
        "En Passant capture move should be generated"
    );

    board.apply_move(ep_move.unwrap()).unwrap();

    let p = board.get_piece(&coord(5, 5));
    assert!(p.is_some());
    assert_eq!(p.unwrap().owner, Player::White);

    let captured = board.get_piece(&coord(4, 5));
    assert!(captured.is_none(), "Captured pawn should be removed");

    assert_eq!(board.en_passant_target, None);
}

#[test]
fn test_castling_kingside_white() {
    let mut board = Board::new_empty(2, 8);
    board.castling_rights = 0xF;

    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(0, 7),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_some(),
        "White Kingside Castling should be available"
    );

    board.apply_move(castle_move.unwrap()).unwrap();

    let k = board.get_piece(&coord(0, 6));
    assert!(k.is_some());
    assert_eq!(k.unwrap().piece_type, PieceType::King);

    let r = board.get_piece(&coord(0, 5));
    assert!(r.is_some());
    assert_eq!(r.unwrap().piece_type, PieceType::Rook);

    assert_eq!(board.castling_rights & 0x3, 0);
}

#[test]
fn test_castling_blocked() {
    let mut board = Board::new_empty(2, 8);
    board.castling_rights = 0xF;

    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 7),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(0, 5),
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(castle_move.is_none(), "Castling should be blocked");
}

#[test]
fn test_castling_through_check() {
    let mut board = Board::new_empty(2, 8);
    board.castling_rights = 0xF;

    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 7),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(7, 5),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_none(),
        "Castling through check should be illegal"
    );
}
