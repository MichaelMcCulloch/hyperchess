use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
// use std::collections::HashSet;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_en_passant() {
    // 1. Setup Board
    let mut board = Board::new_empty(2, 8);

    // Low-level setup: White Pawn at (1, 4), moves to (3, 4) (Double Push)
    // Actually, Black Pawn should be the one capturing? Or White?
    // Let's test White Capturing.
    // White Pawn at (4, 4). Black Pawn moves (6, 5) -> (4, 5).
    // White captures (4, 4) -> (5, 5). En Passant target was (5, 5).

    // Setup White Pawn at 4,4 (Rank 4, File E)
    board
        .set_piece(
            &coord(4, 4),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();

    // Setup Black Pawn at 6,5 (Rank 6, File F) -- Start pos
    board
        .set_piece(
            &coord(6, 5),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    // 2. Execute Black Double Push
    let move_black = hyperchess::domain::models::Move {
        from: coord(6, 5),
        to: coord(4, 5),
        promotion: None,
    };
    board.apply_move(&move_black).unwrap();

    // 3. Verify En Passant Target
    // Rank 5, File 5 -> (5, 5) (Target)
    // Rank 4, File 5 -> (4, 5) (Victim)
    let ep_target_idx = board.coords_to_index(&[5, 5]).unwrap();
    let ep_victim_idx = board.coords_to_index(&[4, 5]).unwrap();
    assert_eq!(
        board.en_passant_target,
        Some((ep_target_idx, ep_victim_idx)),
        "EP Target/Victim tuple should be set"
    );

    // 4. Generate White Moves
    let moves = Rules::generate_legal_moves(&board, Player::White);
    let ep_move = moves.iter().find(|m| m.to == coord(5, 5));

    assert!(
        ep_move.is_some(),
        "En Passant capture move should be generated"
    );

    // 5. Execute En Passant
    board.apply_move(ep_move.unwrap()).unwrap();

    // 6. Verify Result
    // White Pawn at (5, 5)
    let p = board.get_piece(&coord(5, 5));
    assert!(p.is_some());
    assert_eq!(p.unwrap().owner, Player::White);

    // Black Pawn at (4, 5) should be gone
    let captured = board.get_piece(&coord(4, 5));
    assert!(captured.is_none(), "Captured pawn should be removed");

    // EP target should be cleared
    assert_eq!(board.en_passant_target, None);
}

#[test]
fn test_castling_kingside_white() {
    let mut board = Board::new_empty(2, 8);
    board.castling_rights = 0xF; // All rights

    // White King at E1 (0, 4)
    board
        .set_piece(
            &coord(0, 4),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    // White Rook at H1 (0, 7)
    board
        .set_piece(
            &coord(0, 7),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    // Generate moves
    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Expect Castling move to G1 (0, 6) from King (0, 4)
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_some(),
        "White Kingside Castling should be available"
    );

    // Execute
    board.apply_move(castle_move.unwrap()).unwrap();

    // Verify King at G1
    let k = board.get_piece(&coord(0, 6));
    assert!(k.is_some());
    assert_eq!(k.unwrap().piece_type, PieceType::King);

    // Verify Rook at F1 (0, 5)
    let r = board.get_piece(&coord(0, 5));
    assert!(r.is_some());
    assert_eq!(r.unwrap().piece_type, PieceType::Rook);

    // Verify Rights lost (White rights 0 & 1 cleared -> 0xC remaining (Black rights))
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
    // Blocker at F1 (0, 5)
    board
        .set_piece(
            &coord(0, 5),
            Piece {
                piece_type: PieceType::Bishop,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
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

    // Black Rook attacking F1 (0, 5)
    // Place Black Rook at F8 (7, 5)
    board
        .set_piece(
            &coord(7, 5),
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);
    let castle_move = moves
        .iter()
        .find(|m| m.from == coord(0, 4) && m.to == coord(0, 6));
    assert!(
        castle_move.is_none(),
        "Castling through check should be illegal"
    );
}
