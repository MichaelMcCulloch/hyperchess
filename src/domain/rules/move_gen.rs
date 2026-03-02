use std::cell::UnsafeCell;

use crate::domain::board::cache::DirectionInfo;
use crate::domain::board::{BitBoardLarge, Board};
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType, Player};
use crate::domain::rules::MoveList;
use crate::domain::rules::attacks::is_square_attacked;

#[derive(Default)]
struct MoveGenBuffer {
    generator: BitBoardLarge,
    g: BitBoardLarge,
    p: BitBoardLarge,
    shifted_g: BitBoardLarge,
    shifted_p: BitBoardLarge,
    all_occupancy: BitBoardLarge,
    empty: BitBoardLarge,
}

thread_local! {
    static MOVE_GEN_BUFFER: UnsafeCell<MoveGenBuffer> = UnsafeCell::new(MoveGenBuffer::default());
}

pub fn generate_legal_moves(board: &mut Board, player: Player) -> MoveList {
    let mut moves = MoveList::new();
    let pseudo_legal = generate_pseudo_legal_moves(board, player);

    for mv in pseudo_legal {
        if !leaves_king_in_check(board, player, &mv) {
            moves.push(mv);
        }
    }

    generate_castling_moves(board, player, &mut moves);
    moves
}

pub fn generate_loud_moves(board: &mut Board, player: Player) -> MoveList {
    let mut moves = MoveList::new();
    let pseudo_legal = generate_pseudo_legal_moves(board, player);

    for mv in pseudo_legal {
        let is_loud = {
            let enemy_occupancy = match player {
                Player::White => &board.pieces.black_occupancy,
                Player::Black => &board.pieces.white_occupancy,
            };
            let to_idx = board.coords_to_index(&mv.to.values);
            let is_capture = if let Some(idx) = to_idx {
                enemy_occupancy.get_bit(idx)
            } else {
                false
            };
            let is_ep_capture = if let Some((ep_idx, _)) = board.state.en_passant_target {
                if let Some(idx) = to_idx {
                    idx == ep_idx
                        && board
                            .pieces
                            .pawns
                            .get_bit(board.coords_to_index(&mv.from.values).unwrap_or(usize::MAX))
                } else {
                    false
                }
            } else {
                false
            };
            let is_promotion = mv.promotion.is_some();
            is_capture || is_promotion || is_ep_capture
        };

        if is_loud && !leaves_king_in_check(board, player, &mv) {
            moves.push(mv);
        }
    }
    moves
}

pub fn leaves_king_in_check(board: &mut Board, player: Player, mv: &Move) -> bool {
    let info = match board.apply_move(mv) {
        Ok(i) => i,
        Err(_) => return true,
    };
    let in_check = if let Some(king_pos) = board.get_king_coordinate(player) {
        is_square_attacked(board, &king_pos, player.opponent())
    } else {
        false
    };
    board.unmake_move(mv, info);
    in_check
}

pub fn generate_pseudo_legal_moves(board: &Board, player: Player) -> MoveList {
    let mut moves = MoveList::new();
    let occupancy = match player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    // SAFETY: thread_local guarantees single-threaded access; no re-entrant calls.
    MOVE_GEN_BUFFER.with(|buffer_cell| {
        let buffer = unsafe { &mut *buffer_cell.get() };

        let template = &board.pieces.white_occupancy;
        buffer.generator.ensure_capacity_and_clear(template);
        buffer.g.ensure_capacity_and_clear(template);
        buffer.p.ensure_capacity_and_clear(template);
        buffer.shifted_g.ensure_capacity_and_clear(template);
        buffer.shifted_p.ensure_capacity_and_clear(template);
        buffer.all_occupancy.ensure_capacity_and_clear(template);
        buffer.empty.ensure_capacity_and_clear(template);

        // Compute all_occupancy and empty once for all pieces
        board
            .pieces
            .white_occupancy
            .or_into(&board.pieces.black_occupancy, &mut buffer.all_occupancy);
        // Compute empty = !all_occupancy (masked to valid cells)
        // Use manual loop to avoid borrow conflict
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

        let MoveGenBuffer {
            generator,
            g,
            p,
            shifted_g,
            shifted_p,
            all_occupancy,
            empty,
        } = buffer;

        for i in occupancy.iter_indices() {
            let coord = Coordinate::new(board.geo.cache.index_to_coords[i].clone());

            let piece_type = if board.pieces.pawns.get_bit(i) {
                PieceType::Pawn
            } else if board.pieces.knights.get_bit(i) {
                PieceType::Knight
            } else if board.pieces.bishops.get_bit(i) {
                PieceType::Bishop
            } else if board.pieces.rooks.get_bit(i) {
                PieceType::Rook
            } else if board.pieces.queens.get_bit(i) {
                PieceType::Queen
            } else if board.pieces.kings.get_bit(i) {
                PieceType::King
            } else {
                continue;
            };

            match piece_type {
                PieceType::Pawn => {
                    generate_pawn_moves(board, &coord, i, player, all_occupancy, &mut moves)
                }
                PieceType::Knight => generate_leaper_moves(
                    board,
                    i,
                    &coord,
                    player,
                    &board.geo.cache.knight_targets[i],
                    &mut moves,
                ),
                PieceType::King => generate_leaper_moves(
                    board,
                    i,
                    &coord,
                    player,
                    &board.geo.cache.king_targets[i],
                    &mut moves,
                ),
                PieceType::Rook => generate_slider_moves_bitwise(
                    board,
                    i,
                    &coord,
                    player,
                    &board.geo.cache.rook_directions,
                    &mut moves,
                    generator,
                    g,
                    p,
                    shifted_g,
                    shifted_p,
                    empty,
                ),
                PieceType::Bishop => generate_slider_moves_bitwise(
                    board,
                    i,
                    &coord,
                    player,
                    &board.geo.cache.bishop_directions,
                    &mut moves,
                    generator,
                    g,
                    p,
                    shifted_g,
                    shifted_p,
                    empty,
                ),
                PieceType::Queen => {
                    generate_slider_moves_bitwise(
                        board,
                        i,
                        &coord,
                        player,
                        &board.geo.cache.rook_directions,
                        &mut moves,
                        generator,
                        g,
                        p,
                        shifted_g,
                        shifted_p,
                        empty,
                    );
                    generate_slider_moves_bitwise(
                        board,
                        i,
                        &coord,
                        player,
                        &board.geo.cache.bishop_directions,
                        &mut moves,
                        generator,
                        g,
                        p,
                        shifted_g,
                        shifted_p,
                        empty,
                    );
                }
            }
        }
    });
    moves
}

#[allow(clippy::too_many_arguments)]
fn generate_slider_moves_bitwise(
    board: &Board,
    origin_idx: usize,
    origin_coord: &Coordinate,
    player: Player,
    directions: &[DirectionInfo],
    moves: &mut MoveList,

    generator: &mut BitBoardLarge,
    g: &mut BitBoardLarge,
    p: &mut BitBoardLarge,
    shifted_g: &mut BitBoardLarge,
    shifted_p: &mut BitBoardLarge,
    empty: &BitBoardLarge,
) {
    generator.ensure_capacity_and_clear(empty);
    generator.set_bit(origin_idx);

    let own_occupancy = match player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    for dir_info in directions {
        if dir_info.stride == 0 {
            continue;
        }

        g.copy_from(generator);
        p.copy_from(empty);

        kogge_stone_fill_inplace(g, p, dir_info.stride, board, dir_info, shifted_g, shifted_p);

        for to_idx in g.iter_indices() {
            if to_idx == origin_idx {
                continue;
            }
            if own_occupancy.get_bit(to_idx) {
                continue;
            }

            moves.push(Move {
                from: origin_coord.clone(),
                to: Coordinate::new(board.geo.cache.index_to_coords[to_idx].clone()),
                promotion: None,
            });
        }
    }
}

pub fn kogge_stone_fill_inplace(
    g: &mut BitBoardLarge,
    p: &mut BitBoardLarge,
    stride: isize,
    board: &Board,
    dir_info: &DirectionInfo,

    shifted_g: &mut BitBoardLarge,
    shifted_p: &mut BitBoardLarge,
) {
    let len = g.data.len();
    debug_assert_eq!(len, p.data.len());

    let mask_base_idx = dir_info.id * board.side();
    let abs_stride = stride.unsigned_abs();
    let shift_right = stride < 0;

    let mut shift_amt = 1;
    while shift_amt < board.side() {
        let mask = unsafe {
            board
                .geo
                .cache
                .validity_masks
                .get_unchecked(mask_base_idx + shift_amt)
        };

        let total_shift = abs_stride * shift_amt;
        let chunks_shift = total_shift / 64;
        let bits_shift = total_shift % 64;

        // Operate directly on raw data slices, bypassing range tracking.
        let g_data = g.data.as_mut_ptr();
        let p_data = p.data.as_mut_ptr();
        let sg_data = shifted_g.data.as_mut_ptr();
        let sp_data = shifted_p.data.as_mut_ptr();
        let m_data = mask.data.as_ptr();

        unsafe {
            if shift_right {
                raw_and_shr(g_data, m_data, sg_data, len, chunks_shift, bits_shift);
                raw_and_shr(p_data, m_data, sp_data, len, chunks_shift, bits_shift);
            } else {
                raw_and_shl(g_data, m_data, sg_data, len, chunks_shift, bits_shift);
                raw_and_shl(p_data, m_data, sp_data, len, chunks_shift, bits_shift);
            }

            // g |= shifted_g & p; p &= shifted_p (fused, with early-exit on dead p)
            let mut any_p = false;
            for i in 0..len {
                *g_data.add(i) |= *sg_data.add(i) & *p_data.add(i);
                let new_p = *p_data.add(i) & *sp_data.add(i);
                *p_data.add(i) = new_p;
                any_p |= new_p != 0;
            }

            if !any_p {
                // p is all-zero — no further propagation possible.
                break;
            }
        }

        shift_amt *= 2;
    }

    // Final mask and shift
    let mask = unsafe {
        board
            .geo
            .cache
            .validity_masks
            .get_unchecked(mask_base_idx + 1)
    };

    *g &= mask;
    if shift_right {
        *g >>= abs_stride;
    } else {
        *g <<= abs_stride;
    }

    // Recompute ranges since we bypassed range tracking
    g.recompute_range();
    p.recompute_range();
}

/// Raw `dst = (a & b) >> shift` on u64 slices. No bounds/range tracking.
#[inline(always)]
unsafe fn raw_and_shr(
    a: *const u64,
    b: *const u64,
    dst: *mut u64,
    len: usize,
    chunks_shift: usize,
    bits_shift: usize,
) {
    unsafe {
        if bits_shift == 0 {
            // Pure chunk shift
            for i in 0..len {
                let src = i + chunks_shift;
                *dst.add(i) = if src < len {
                    *a.add(src) & *b.add(src)
                } else {
                    0
                };
            }
        } else {
            let inv = 64 - bits_shift;
            for i in 0..len {
                let src = i + chunks_shift;
                let cur = if src < len {
                    *a.add(src) & *b.add(src)
                } else {
                    0
                };
                let next = if src + 1 < len {
                    *a.add(src + 1) & *b.add(src + 1)
                } else {
                    0
                };
                *dst.add(i) = (cur >> bits_shift) | (next << inv);
            }
        }
    }
}

/// Raw `dst = (a & b) << shift` on u64 slices. No bounds/range tracking.
#[inline(always)]
unsafe fn raw_and_shl(
    a: *const u64,
    b: *const u64,
    dst: *mut u64,
    len: usize,
    chunks_shift: usize,
    bits_shift: usize,
) {
    unsafe {
        if bits_shift == 0 {
            for i in (0..len).rev() {
                *dst.add(i) = if i >= chunks_shift {
                    *a.add(i - chunks_shift) & *b.add(i - chunks_shift)
                } else {
                    0
                };
            }
        } else {
            let inv = 64 - bits_shift;
            for i in (0..len).rev() {
                let cur = if i >= chunks_shift {
                    let src = i - chunks_shift;
                    *a.add(src) & *b.add(src)
                } else {
                    0
                };
                let prev = if i > chunks_shift {
                    let src = i - chunks_shift - 1;
                    *a.add(src) & *b.add(src)
                } else {
                    0
                };
                *dst.add(i) = (cur << bits_shift) | (prev >> inv);
            }
        }
    }
}

pub fn calculate_stride(board: &Board, dir: &[isize]) -> isize {
    let mut stride = 0;
    let mut multiplier = 1;
    for d_val in dir.iter().take(board.dimension()) {
        stride += d_val * multiplier as isize;
        multiplier *= board.side();
    }
    stride
}

fn generate_castling_moves(board: &Board, player: Player, moves: &mut MoveList) {
    if board.side() != 8 {
        return;
    }
    let (rights_mask, rank) = match player {
        Player::White => (0x3, 0),
        Player::Black => (0xC, board.side() - 1),
    };
    let my_rights = board.state.castling_rights & rights_mask;
    if my_rights == 0 {
        return;
    }

    let king_file = 4;
    let mut king_coords = vec![rank as u8; board.dimension()];
    king_coords[1] = king_file;
    let king_coord = Coordinate::new(king_coords.clone());
    if is_square_attacked(board, &king_coord, player.opponent()) {
        return;
    }

    let all_occupancy = &board.pieces.white_occupancy | &board.pieces.black_occupancy;
    let ks_mask = match player {
        Player::White => 0x1,
        Player::Black => 0x4,
    };
    if (my_rights & ks_mask) != 0 {
        let f_file = 5;
        let g_file = 6;
        let mut f_coords = king_coords.clone();
        f_coords[1] = f_file;
        let mut g_coords = king_coords.clone();
        g_coords[1] = g_file;
        let f_idx = board.coords_to_index(&f_coords);
        let g_idx = board.coords_to_index(&g_coords);
        let mut blocked = true;
        if let (Some(fi), Some(gi)) = (f_idx, g_idx)
            && !all_occupancy.get_bit(fi)
            && !all_occupancy.get_bit(gi)
        {
            blocked = false;
        }
        if !blocked
            && !is_square_attacked(board, &Coordinate::new(f_coords), player.opponent())
            && !is_square_attacked(board, &Coordinate::new(g_coords.clone()), player.opponent())
        {
            moves.push(Move {
                from: king_coord.clone(),
                to: Coordinate::new(g_coords),
                promotion: None,
            });
        }
    }
    let qs_mask = match player {
        Player::White => 0x2,
        Player::Black => 0x8,
    };
    if (my_rights & qs_mask) != 0 {
        let b_file = 1;
        let c_file = 2;
        let d_file = 3;
        let mut b_coords = king_coords.clone();
        b_coords[1] = b_file;
        let mut c_coords = king_coords.clone();
        c_coords[1] = c_file;
        let mut d_coords = king_coords.clone();
        d_coords[1] = d_file;
        let b_idx = board.coords_to_index(&b_coords);
        let c_idx = board.coords_to_index(&c_coords);
        let d_idx = board.coords_to_index(&d_coords);
        let mut blocked = true;
        if let (Some(bi), Some(ci), Some(di)) = (b_idx, c_idx, d_idx)
            && !all_occupancy.get_bit(bi)
            && !all_occupancy.get_bit(ci)
            && !all_occupancy.get_bit(di)
        {
            blocked = false;
        }
        if !blocked
            && !is_square_attacked(board, &Coordinate::new(d_coords), player.opponent())
            && !is_square_attacked(board, &Coordinate::new(c_coords.clone()), player.opponent())
        {
            moves.push(Move {
                from: king_coord.clone(),
                to: Coordinate::new(c_coords),
                promotion: None,
            });
        }
    }
}

fn generate_leaper_moves(
    board: &Board,
    _origin_idx: usize,
    origin: &Coordinate,
    player: Player,
    targets: &[usize],
    moves: &mut MoveList,
) {
    let same_occupancy = match player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };
    for &target_idx in targets {
        if !same_occupancy.get_bit(target_idx) {
            moves.push(Move {
                from: origin.clone(),
                to: Coordinate::new(board.geo.cache.index_to_coords[target_idx].clone()),
                promotion: None,
            });
        }
    }
}

fn generate_pawn_moves(
    board: &Board,
    origin: &Coordinate,
    origin_idx: usize,
    player: Player,
    all_occupancy: &BitBoardLarge,
    moves: &mut MoveList,
) {
    let side = board.side();
    let total_cells = board.total_cells();
    let enemy_occupancy = match player.opponent() {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    let forward_sign: isize = match player {
        Player::White => 1,
        Player::Black => -1,
    };

    // Precompute axis strides: stride[k] = side^k
    let mut axis_stride = 1usize;
    for movement_axis in 0..board.dimension() {
        let cur_stride = axis_stride;
        axis_stride *= side;

        if movement_axis == 1 {
            continue;
        }

        let coord_val = origin.values[movement_axis];
        let forward_target_coord = coord_val as isize + forward_sign;
        if forward_target_coord < 0 || forward_target_coord >= side as isize {
            continue;
        }

        let forward_idx = (origin_idx as isize + forward_sign * cur_stride as isize) as usize;
        if forward_idx >= total_cells || all_occupancy.get_bit(forward_idx) {
            continue;
        }

        add_pawn_move_idx(origin, forward_idx, board, player, moves);

        let is_start_rank = match player {
            Player::White => coord_val == 1,
            Player::Black => coord_val as usize == side - 2,
        };
        if is_start_rank {
            let double_idx =
                (origin_idx as isize + forward_sign * 2 * cur_stride as isize) as usize;
            if double_idx < total_cells && !all_occupancy.get_bit(double_idx) {
                add_pawn_move_idx(origin, double_idx, board, player, moves);
            }
        }

        // Captures: for each other axis, try ±1 on that axis combined with forward on movement_axis
        let mut cap_axis_stride = 1usize;
        for capture_axis in 0..board.dimension() {
            let cap_stride = cap_axis_stride;
            cap_axis_stride *= side;

            if capture_axis == movement_axis {
                continue;
            }
            let cap_coord = origin.values[capture_axis];
            for s in [-1isize, 1isize] {
                let cap_target_coord = cap_coord as isize + s;
                if cap_target_coord < 0 || cap_target_coord >= side as isize {
                    continue;
                }
                let target_idx = (origin_idx as isize
                    + forward_sign * cur_stride as isize
                    + s * cap_stride as isize) as usize;
                if target_idx >= total_cells {
                    continue;
                }
                if enemy_occupancy.get_bit(target_idx) {
                    add_pawn_move_idx(origin, target_idx, board, player, moves);
                } else if let Some((ep_target, _)) = board.state.en_passant_target
                    && target_idx == ep_target
                {
                    moves.push(Move {
                        from: origin.clone(),
                        to: Coordinate::new(board.geo.cache.index_to_coords[target_idx].clone()),
                        promotion: None,
                    });
                }
            }
        }
    }
}

#[inline]
fn add_pawn_move_idx(
    from: &Coordinate,
    to_idx: usize,
    board: &Board,
    player: Player,
    moves: &mut MoveList,
) {
    let to_vals = &board.geo.cache.index_to_coords[to_idx];
    let side = board.side();
    let is_promotion = (0..to_vals.len()).all(|i| {
        if i == 1 {
            true
        } else {
            match player {
                Player::White => to_vals[i] as usize == side - 1,
                Player::Black => to_vals[i] == 0,
            }
        }
    });
    let to = Coordinate::new(to_vals.clone());
    if is_promotion {
        for t in [
            PieceType::Queen,
            PieceType::Rook,
            PieceType::Bishop,
            PieceType::Knight,
        ] {
            moves.push(Move {
                from: from.clone(),
                to: to.clone(),
                promotion: Some(t),
            });
        }
    } else {
        moves.push(Move {
            from: from.clone(),
            to,
            promotion: None,
        });
    }
}
