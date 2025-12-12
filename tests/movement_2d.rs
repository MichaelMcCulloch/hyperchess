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
    // Remove all pieces for clean slate testing?
    // `new` creates empty board? User prompt said "The board is empty at the beginning of the game".
    // Let's verify that. If so, we just place what we need.

    // Low-level setup: White Pawn at (1, 1) (Rank 1 is usually pawn start in 0-indexed terms? in standard chess: rank 1 (0-7 indexing) is White Pawns)
    // Coords: vec![rank, file] or vec![file, rank]?
    // mechanics.rs: `forward_dir` for White is +1 on axis 0.
    // So axis 0 is "Rank" (Forward/Backward). Axis 1 is "File" (Sideways).
    // White moves +1 on Axis 0.
    // Start Rank for White is typically index 1.
    // Start Rank for Black is typically index 6.

    let pawn_pos = coord(1, 3); // Rank 1, File 3
    let p = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    board.set_piece(&pawn_pos, p).unwrap();

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Expect: Single push to (2, 3), Double push to (3, 3).
    // No captures available.

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    // Expect:
    // Axis 0 (Rank): Single push (2, 3), Double push (3, 3) (Since From(1) == Standard Start(1)? check rules.rs double push logic).
    // Axis 1 (File): Single push (1, 4). (From(3) != Standard Start(1) for Axis 1?).
    // Wait, rules.rs uses `to_vals[movement_axis] == ranking offset`.
    // Wait, double push condition in rules.rs: `if origin.values[movement_axis] == start_rank`.
    // start_rank = 1 (White).
    // Axis 0: origin[0] = 1. Matches. -> Double Push allowed.
    // Axis 1: origin[1] = 3. 3 != 1. -> No Double Push.
    // So: (2, 3), (3, 3), (1, 4). Total 3.

    assert!(
        dests.contains(&coord(2, 3)),
        "Should have single push on rank"
    );
    assert!(
        dests.contains(&coord(3, 3)),
        "Should have double push on rank"
    );
    assert!(
        dests.contains(&coord(1, 4)),
        "Should have single push on file (Super Pawn)"
    );
    assert_eq!(dests.len(), 3, "Should have 3 moves (2 Rank + 1 File)");
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

    let moves = Rules::generate_legal_moves(&board, Player::White);

    // Pawn cannot move forward on Axis 0 (blocked).
    // BUT Super Pawn can move "forward" on Axis 1 (File) -> (1, 5).
    // Expect 1 move.

    // Debug print
    if moves.len() != 1 {
        let dests: Vec<_> = moves.iter().map(|m| m.to.values.clone()).collect();
        eprintln!("Blocked Pawn test moves: {:?}", dests);
    }

    assert_eq!(
        moves.len(),
        1,
        "Super Pawn blocked on rank should still move on file"
    );
    assert!(
        moves.iter().any(|m| m.to == coord(1, 5)),
        "Should allow file push"
    );
}

#[test]
fn test_pawn_capture() {
    let mut board = Board::new_empty(2, 8);
    let pawn_pos = coord(3, 3); // Not start rank
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

    let moves = Rules::generate_legal_moves(&board, Player::White);

    let dests: HashSet<Coordinate> = moves.iter().map(|m| m.to.clone()).collect();

    // Captures available:
    // Axis 0 Move (+1) -> Captures on Axis 1 (+/-1).
    //   -> Move to (4, 3) -- Non-capture.
    //   -> Capture Logic: From (3,3) + Forward(0) + Side(1).
    //      Forward(0): +1 -> 4. Side(1): 3 +/- 1 -> 2, 4.
    //      Targets: (4, 2), (4, 4).
    //      (4, 4) has enemy. Capture! (4, 2) is empty (or off board).
    // Axis 1 Move (+1) -> Captures on Axis 0 (+/-1).
    //   -> Move to (3, 4) -- Non-capture. (Is (3,4) empty? No checked in test setup, assumed empty).
    //   -> Capture Logic: From (3,3) + Forward(1) + Side(0).
    //      Forward(1): +1 -> 4. Side(0): 3 +/- 1 -> 2, 4.
    //      Targets: (2, 4), (4, 4).
    //      (4, 4) has enemy. Capture! (Duplicate target, distinct move path? Rules generate by target).
    //      "moves.push(...)". If logic generates duplicates, HashSet filters them?
    //      Or List? `dests` is HashSet. So (4,4) is 1 destination.

    // Moves:
    // 1. (4, 3) [Axis 0 Push]
    // 2. (3, 4) [Axis 1 Push]
    // 3. (4, 4) [Capture]

    // Total 3 unique destinations?
    // Wait, (4, 4) is captured via Axis 0 AND Axis 1?
    // Yes.

    assert!(dests.contains(&coord(4, 3)), "Single push rank");
    assert!(dests.contains(&coord(3, 4)), "Single push file");
    assert!(dests.contains(&coord(4, 4)), "Capture intersection");

    assert_eq!(dests.len(), 3, "Should have 3 moves (2 pushes + 1 capture)");
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

    let moves = Rules::generate_legal_moves(&board, Player::White);

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

    let moves = Rules::generate_legal_moves(&board, Player::White);
    let rook_moves: Vec<_> = moves.into_iter().filter(|m| m.from == pos).collect();
    let dests: HashSet<Coordinate> = rook_moves.iter().map(|m| m.to.clone()).collect();

    // Axis 0 (Vertical/Rank): (0..8) except 4 -> 7 squares.
    // Axis 1 (Horizontal/File): 4 is blocked at 6. Can go 0,1,2,3,5. (Blocked at 6 means cannot go to 6 or 7).
    // Total: 7 + 5 = 12 moves?

    // Explicit checks:
    // Vertical: (0,4), (1,4), (2,4), (3,4), (5,4), (6,4), (7,4) -> 7 moves
    // Horizontal: (4,0), (4,1), (4,2), (4,3), (4,5) -> 5 moves

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

    let moves = Rules::generate_legal_moves(&board, Player::White);
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

    let moves = Rules::generate_legal_moves(&board, Player::White);
    // 8 neighbors
    assert_eq!(moves.len(), 8);
}
