use crate::domain::board::Board;
use crate::domain::models::{PieceType, Player};
use crate::domain::rules::move_gen::count_slider_mobility_scalar;

pub fn count_piece_mobility(board: &Board, index: usize, piece_type: PieceType) -> i32 {
    count_piece_mobility_for(board, index, piece_type, None)
}

pub fn count_piece_mobility_for(
    board: &Board,
    index: usize,
    piece_type: PieceType,
    known_player: Option<Player>,
) -> i32 {
    let player = known_player.unwrap_or_else(|| {
        board
            .get_piece_at_index(index)
            .map(|p| p.owner)
            .unwrap_or(Player::White)
    });

    match piece_type {
        PieceType::Pawn | PieceType::King => 0,
        PieceType::Knight => {
            count_leaper_moves_idx(board, index, player, &board.geo.cache.knight_targets[index])
        }
        PieceType::Rook => {
            count_slider_mobility_scalar(board, index, player, &board.geo.cache.rook_directions)
        }
        PieceType::Bishop => {
            count_slider_mobility_scalar(board, index, player, &board.geo.cache.bishop_directions)
        }
        PieceType::Queen => {
            count_slider_mobility_scalar(board, index, player, &board.geo.cache.rook_directions)
                + count_slider_mobility_scalar(
                    board,
                    index,
                    player,
                    &board.geo.cache.bishop_directions,
                )
        }
    }
}

#[inline]
fn count_leaper_moves_idx(
    board: &Board,
    _origin_idx: usize,
    player: Player,
    targets: &[usize],
) -> i32 {
    let same_occupancy = match player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };
    let mut count = 0;
    for &target_idx in targets {
        if !same_occupancy.get_bit(target_idx) {
            count += 1;
        }
    }
    count
}
