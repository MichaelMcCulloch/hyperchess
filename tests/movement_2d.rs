use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use std::collections::HashSet;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_pawn_moves_white_start() {
    let mut board = Board::new_empty(2, 8);

    // Setup: White Pawn at Rank 1, File 3
    let pawn_pos = coord(1, 3);
    let p = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    board.set_piece(&pawn_pos, p).unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    // Expect:
    // Axis 0 (Rank): Single push (2, 3), Double push (3, 3).
    // Axis 1 (File): Forbidden by new rules.

    assert!(
        dests.contains(&coord(2, 3)),
        "Should have single push on rank"
    );
    assert!(
        dests.contains(&coord(3, 3)),
        "Should have double push on rank"
    );
    assert!(
        !dests.contains(&coord(1, 4)),
        "Should NOT have single push on file (Lateral forbidden)"
    );
    assert_eq!(dests.len(), 2, "Should have 2 moves (2 Rank pushes)");
}

#[test]
fn test_pawn_blocked() {
    let mut board = Board::new_empty(2, 8);
    let pawn_pos = coord(1, 4);
    let blocker = coord(2, 4);

    board
        .set_piece(
            &pawn_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &blocker,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::Black,
            },
        )
        .unwrap(); // Enemy blocks

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    // Pawn cannot move forward on Axis 0 (blocked).
    // Axis 1 (File) is forbidden for pushes.
    // Expect 0 moves.

    assert_eq!(
        moves.len(),
        0,
        "Pawn blocked on rank and forbidden on file should have no moves"
    );
}

#[test]
fn test_pawn_capture() {
    let mut board = Board::new_empty(2, 8);
    let pawn_pos = coord(3, 3);
    let enemy_pos = coord(4, 4); // Diagonally forward right

    board
        .set_piece(
            &pawn_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &enemy_pos,
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    // Moves:
    // 1. (4, 3) [Axis 0 Push] - Allowed.
    // 2. (3, 4) [Axis 1 Push] - Forbidden.
    // 3. (4, 4) [Capture] - Allowed (Axis 0 Move + Axis 1 Offset).

    assert!(dests.contains(&coord(4, 3)), "Single push rank");
    assert!(!dests.contains(&coord(3, 4)), "Single push file forbidden");
    assert!(dests.contains(&coord(4, 4)), "Capture intersection");

    assert_eq!(dests.len(), 2, "Should have 2 moves (1 push + 1 capture)");
}

#[test]
fn test_knight_moves_center() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(4, 4);
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

    // 8 possible moves in 2D
    assert_eq!(moves.len(), 8);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();
    // +/- 2 on one axis, +/- 1 on other
    assert!(dests.contains(&coord(6, 5)));
    assert!(dests.contains(&coord(6, 3)));
    assert!(dests.contains(&coord(2, 5)));
    assert!(dests.contains(&coord(2, 3)));
    assert!(dests.contains(&coord(5, 6)));
    assert!(dests.contains(&coord(3, 6)));
    assert!(dests.contains(&coord(5, 2)));
    assert!(dests.contains(&coord(3, 2)));
}

#[test]
fn test_rook_moves() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(4, 4);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::Rook,
                owner: Player::White,
            },
        )
        .unwrap();

    // Add a blocker
    board
        .set_piece(
            &coord(4, 6),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::White,
            },
        )
        .unwrap(); // Clean block

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let rook_moves: Vec<_> = moves.into_iter().filter(|m| m.from == pos).collect();
    let dests: HashSet<Coordinate> = rook_moves.iter().map(|m| m.to.clone()).collect();

    // Axis 0 (Vertical/Rank): (0..8) except 4 -> 7 squares.
    // Axis 1 (Horizontal/File): 4 is blocked at 6. Can go 0,1,2,3,5.
    // Total: 7 + 5 = 12 moves

    assert_eq!(rook_moves.len(), 12);
    assert!(!dests.contains(&coord(4, 6))); // Blocked
    assert!(!dests.contains(&coord(4, 7))); // Behind blocker
}

#[test]
fn test_bishop_moves() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(0, 0); // Corner
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
    // Main diagonal only: (1,1) .. (7,7) -> 7 moves
    assert_eq!(moves.len(), 7);
}

#[test]
fn test_king_moves() {
    let mut board = Board::new_empty(2, 8);
    let pos = coord(1, 1);
    board
        .set_piece(
            &pos,
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    // 8 neighbors
    assert_eq!(moves.len(), 8);
}
