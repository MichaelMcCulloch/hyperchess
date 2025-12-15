use hyperchess::config::MinimaxConfig;
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Piece, PieceType, Player};
use hyperchess::domain::rules::Rules;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;

fn coord(x: usize, y: usize) -> Coordinate {
    Coordinate::new(vec![x, y])
}

#[test]
fn test_detect_checkmate_in_one() {
    let mut board = Board::new_empty(2, 4);

    board
        .set_piece(
            &coord(3, 3),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(2, 3),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(3, 2),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(2, 2),
            Piece {
                piece_type: PieceType::Pawn,
                owner: Player::Black,
            },
        )
        .unwrap();

    board = Board::new_empty(2, 4);
    board
        .set_piece(
            &coord(0, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(2, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();

    board
        .set_piece(
            &coord(0, 3),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::White,
            },
        )
        .unwrap();

    board = Board::new_empty(2, 4);
    board
        .set_piece(
            &coord(0, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(1, 2),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 3),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::White,
            },
        )
        .unwrap();

    let config = MinimaxConfig { depth: 2 };
    let mut bot = MinimaxBot::new(&config, 1000, 2, 4, 256);
    let mv = bot
        .get_move(&board, Player::White)
        .expect("Should return a move");

    assert!(
        mv.to == coord(0, 1) || mv.to == coord(2, 1),
        "Should find checkmate move (Queen to (0,1) or King to (2,1)), found {:?}",
        mv.to
    );
}

#[test]
fn test_verify_mate_validity() {
    let mut board = Board::new_empty(2, 4);
    board
        .set_piece(
            &coord(0, 0),
            Piece {
                piece_type: PieceType::King,
                owner: Player::Black,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(1, 2),
            Piece {
                piece_type: PieceType::King,
                owner: Player::White,
            },
        )
        .unwrap();
    board
        .set_piece(
            &coord(0, 3),
            Piece {
                piece_type: PieceType::Queen,
                owner: Player::White,
            },
        )
        .unwrap();

    let moves = Rules::generate_legal_moves(&mut board, Player::White);
    let mate_move = moves.iter().find(|m| m.to == coord(0, 1));
    assert!(mate_move.is_some(), "Move to (0,1) should be legal");

    board.apply_move(mate_move.unwrap()).unwrap();

    let black_moves = Rules::generate_legal_moves(&mut board, Player::Black);
    assert!(
        black_moves.is_empty(),
        "Black should have no moves after Checkmate"
    );

    let black_king = board.get_king_coordinate(Player::Black).unwrap();
    assert!(
        Rules::is_square_attacked(&board, &black_king, Player::White),
        "Black King should be in check"
    );
}

#[test]
fn test_avoid_immediate_mate() {}
