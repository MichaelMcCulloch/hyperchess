use std::cell::RefCell;

use crate::domain::board::BitBoardLarge;
use crate::domain::board::Board;
use crate::domain::board::cache::DirectionInfo;
use crate::domain::models::{PieceType, Player};
use crate::domain::rules::move_gen::kogge_stone_fill_inplace;

#[derive(Default)]
struct MobilityBuffer {
    generator: BitBoardLarge,
    g: BitBoardLarge,
    p: BitBoardLarge,
    shifted_g: BitBoardLarge,
    shifted_p: BitBoardLarge,
    all_occupancy: BitBoardLarge,
    empty: BitBoardLarge,
}

thread_local! {
    static MOBILITY_BUFFER: RefCell<MobilityBuffer> = RefCell::new(MobilityBuffer::default());
}

pub fn count_piece_mobility(board: &Board, index: usize, piece_type: PieceType) -> i32 {
    let player = board
        .get_piece_at_index(index)
        .map(|p| p.owner)
        .unwrap_or(Player::White);

    match piece_type {
        PieceType::Pawn | PieceType::King => 0,
        PieceType::Knight => {
            count_leaper_moves_idx(board, index, player, &board.geo.cache.knight_targets[index])
        }
        PieceType::Rook | PieceType::Bishop | PieceType::Queen => {
            count_slider_mobility(board, index, player, piece_type)
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

/// Single thread_local access for all slider directions of one piece.
fn count_slider_mobility(
    board: &Board,
    origin_idx: usize,
    player: Player,
    piece_type: PieceType,
) -> i32 {
    let own_occupancy = match player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    MOBILITY_BUFFER.with(|buffer_ref| {
        let mut buffer = buffer_ref.borrow_mut();

        let template = &board.pieces.white_occupancy;
        buffer.generator.ensure_capacity_and_clear(template);
        buffer.g.ensure_capacity_and_clear(template);
        buffer.p.ensure_capacity_and_clear(template);
        buffer.shifted_g.ensure_capacity_and_clear(template);
        buffer.shifted_p.ensure_capacity_and_clear(template);
        buffer.all_occupancy.ensure_capacity_and_clear(template);
        buffer.empty.ensure_capacity_and_clear(template);

        // Compute all_occupancy and empty once
        board
            .pieces
            .white_occupancy
            .or_into(&board.pieces.black_occupancy, &mut buffer.all_occupancy);
        {
            let total_cells = board.total_cells();
            let len = buffer.all_occupancy.data.len();
            let mut remaining = total_cells;
            for i in 0..len {
                let limit = std::cmp::min(64, remaining);
                let mask = if limit == 64 {
                    !0u64
                } else {
                    (1u64 << limit) - 1
                };
                buffer.empty.data[i] = (!buffer.all_occupancy.data[i]) & mask;
                remaining = remaining.saturating_sub(64);
            }
        }
        buffer.all_occupancy.recompute_range();
        buffer.empty.recompute_range();

        buffer.generator.set_bit(origin_idx);

        let MobilityBuffer {
            generator,
            g,
            p,
            shifted_g,
            shifted_p,
            all_occupancy: _,
            empty,
        } = &mut *buffer;

        let mut count = 0;

        // Collect direction sets to iterate based on piece type
        let dir_sets: &[&[DirectionInfo]] = match piece_type {
            PieceType::Rook => &[&board.geo.cache.rook_directions],
            PieceType::Bishop => &[&board.geo.cache.bishop_directions],
            PieceType::Queen => &[
                &board.geo.cache.rook_directions,
                &board.geo.cache.bishop_directions,
            ],
            _ => &[],
        };

        for directions in dir_sets {
            for dir_info in *directions {
                if dir_info.stride == 0 {
                    continue;
                }

                g.copy_from(generator);
                p.copy_from(empty);

                kogge_stone_fill_inplace(
                    g,
                    p,
                    dir_info.stride,
                    board,
                    dir_info,
                    shifted_g,
                    shifted_p,
                );

                g.andnot_assign(own_occupancy);
                count += g.count_ones() as i32;
                if g.get_bit(origin_idx) {
                    count -= 1;
                }
            }
        }
        count
    })
}
