use hyperchess::domain::board::Board;
use hyperchess::domain::models::Player;
use hyperchess::domain::rules::Rules;

#[test]
fn test_initial_board_setup_and_pawn_move() {
    let dim = 2;
    let side = 8;
    let mut board = Board::new_empty(dim, side);

    use hyperchess::domain::coordinate::Coordinate;
    use hyperchess::domain::models::{Piece, PieceType};

    let pawn = Piece {
        piece_type: PieceType::Pawn,
        owner: Player::White,
    };
    let start_idx = 8;
    let start_coord = Coordinate::new(board.index_to_coords(start_idx));

    board.set_piece(&start_coord, pawn).unwrap();

    let w_king = Piece {
        piece_type: PieceType::King,
        owner: Player::White,
    };
    let b_king = Piece {
        piece_type: PieceType::King,
        owner: Player::Black,
    };

    let w_king_coord = Coordinate::new(vec![0, 0]);
    let b_king_coord = Coordinate::new(vec![7, 7]);

    board.set_piece(&w_king_coord, w_king).unwrap();
    board.set_piece(&b_king_coord, b_king).unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);

    assert!(!moves.is_empty(), "Should generate moves");

    let pawn_move = moves
        .iter()
        .find(|m| m.from == start_coord)
        .expect("Should find pawn move")
        .clone();

    println!("Applying move: {:?}", pawn_move);

    board.apply_move(&pawn_move).unwrap();

    assert!(
        board.get_piece(&start_coord).is_none(),
        "Start square should be empty"
    );
    assert!(
        board.get_piece(&pawn_move.to).is_some(),
        "End square should be occupied"
    );
}
