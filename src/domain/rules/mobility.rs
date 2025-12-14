use smallvec::SmallVec;

use crate::domain::board::{BitBoard, Board};
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{PieceType, Player};
use crate::domain::rules::apply_offset;
use crate::domain::rules::move_gen::{calculate_stride, kogge_stone_fill_inplace};

pub fn count_piece_mobility(board: &Board, index: usize, piece_type: PieceType) -> i32 {
    let coords = board.index_to_coords(index);
    let coord = Coordinate::new(coords.clone());
    let player = board
        .get_piece_at_index(index)
        .map(|p| p.owner)
        .unwrap_or(Player::White);

    let mut count = 0;

    match piece_type {
        PieceType::Pawn => {
            return 0;
        }
        PieceType::Knight => {
            return count_leaper_moves(board, &coord, player, &board.cache.knight_offsets);
        }
        PieceType::King => {
            return count_leaper_moves(board, &coord, player, &board.cache.king_offsets);
        }
        PieceType::Rook => {
            return count_slider_moves(board, &coord, player, &board.cache.rook_directions);
        }
        PieceType::Bishop => {
            return count_slider_moves(board, &coord, player, &board.cache.bishop_directions);
        }
        PieceType::Queen => {
            count += count_slider_moves(board, &coord, player, &board.cache.rook_directions);
            count += count_slider_moves(board, &coord, player, &board.cache.bishop_directions);
            return count;
        }
    }
}

fn count_leaper_moves(
    board: &Board,
    origin: &Coordinate,
    player: Player,
    offsets: &[Vec<isize>],
) -> i32 {
    let mut count = 0;
    let same_occupancy = match player {
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };
    for offset in offsets {
        if let Some(target) = apply_offset(&origin.values, offset, board.side) {
            if let Some(idx) = board.coords_to_index(&target) {
                if !same_occupancy.get_bit(idx) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn count_slider_moves(
    board: &Board,
    origin: &Coordinate,
    player: Player,
    directions: &[Vec<isize>],
) -> i32 {
    let origin_idx = board.coords_to_index(&origin.values).unwrap();
    let mut count = 0;
    let mut generator = board.white_occupancy.zero_like();
    generator.set_bit(origin_idx);

    let all_occupancy = &board.white_occupancy | &board.black_occupancy;
    let own_occupancy = match player {
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };

    let empty = match all_occupancy {
        BitBoard::Small(b) => {
            let mask = (1u32.checked_shl(board.total_cells as u32).unwrap_or(0)).wrapping_sub(1);
            BitBoard::Small((!b) & mask)
        }
        BitBoard::Medium(b) => {
            let mask = (1u128.checked_shl(board.total_cells as u32).unwrap_or(0)).wrapping_sub(1);
            BitBoard::Medium((!b) & mask)
        }
        BitBoard::Large { data } => {
            let mut new_data = SmallVec::with_capacity(data.len());
            let mut remaining = board.total_cells;
            for val in data {
                let limit = std::cmp::min(64, remaining);
                let mask = if limit == 64 {
                    !0u64
                } else {
                    (1u64 << limit) - 1
                };
                new_data.push((!val) & mask);
                remaining = remaining.saturating_sub(64);
            }
            BitBoard::Large { data: new_data }
        }
    };

    let mut g = generator.zero_like();
    let mut p = generator.zero_like();
    let mut shifted_g = generator.zero_like();
    let mut shifted_p = generator.zero_like();
    let mut temp = generator.zero_like();

    for dir in directions {
        let stride = calculate_stride(board, dir);
        if stride == 0 {
            continue;
        }

        g.copy_from(&generator);
        p.copy_from(&empty);

        kogge_stone_fill_inplace(
            &mut g,
            &mut p,
            stride,
            board,
            dir,
            &mut shifted_g,
            &mut shifted_p,
            &mut temp,
        );

        g &= &(!own_occupancy);
        count += g.count_ones() as i32;
        if g.get_bit(origin_idx) {
            count -= 1;
        }
    }
    count
}
