#[cfg(test)]
mod tests {
    use hyperchess::domain::board::{BitBoard, Board};
    use hyperchess::domain::coordinate::Coordinate;
    use hyperchess::domain::models::{Piece, PieceType, Player};
    use hyperchess::domain::rules::Rules;

    fn coord(x: usize, y: usize) -> Coordinate {
        Coordinate::new(vec![x, y])
    }

    #[test]
    fn debug_validity_mask_2x4() {
        let mut board = Board::new_empty(2, 4);

        // Setup from failing test case
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
        println!("Generated {} moves:", moves.len());
        let mut found_mate = false;
        let mut found_02 = false;

        for mv in &moves {
            println!("Move: {:?} -> {:?}", mv.from, mv.to);
            if mv.to == coord(0, 1) {
                found_mate = true;
            }
            if mv.to == coord(0, 2) {
                found_02 = true;
            }
        }

        // Check validity mask for (0, -1) step 1 just in case
        let dir = vec![0, -1];
        if let Some(mask) = board.cache.validity_masks.get(&(dir.clone(), 1)) {
            if let BitBoard::Small(bits) = mask {
                println!("Mask (0,-1) step 1: {:016b}", bits);
            }
        }

        assert!(found_02, "Should find move to (0,2)");
        assert!(found_mate, "Should find move to (0,1)");

        // Simulate Move (0,3) -> (0,1)
        println!("--- Simulating Move Queen to (0,1) ---");
        // Apply move manually
        let move_mate = moves.iter().find(|m| m.to == coord(0, 1)).unwrap();
        board.apply_move(move_mate).unwrap();

        // Check if Black has moves
        let black_moves = Rules::generate_legal_moves(&mut board, Player::Black);
        println!("Black moves count: {}", black_moves.len());
        for bm in &black_moves {
            println!("Black Move: {:?} -> {:?}", bm.from, bm.to);
        }

        let black_king_pos = board.get_king_coordinate(Player::Black).unwrap();
        let in_check = Rules::is_square_attacked(&board, &black_king_pos, Player::White);
        println!("Black King at {:?} In Check: {}", black_king_pos, in_check);

        assert!(black_moves.is_empty(), "Black should be checkmated");
        assert!(in_check, "Black should be in check");
    }
}
