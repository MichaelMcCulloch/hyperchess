use smallvec::SmallVec;
use std::cell::RefCell;

use crate::domain::board::cache::DirectionInfo;
use crate::domain::board::{BitBoardLarge, Board};
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType, Player};
use crate::domain::rules::attacks::is_square_attacked;
use crate::domain::rules::{MoveList, apply_offset};

struct MoveGenBuffer {
    generator: BitBoardLarge,
    g: BitBoardLarge,
    p: BitBoardLarge,
    shifted_g: BitBoardLarge,
    shifted_p: BitBoardLarge,
    temp: BitBoardLarge,
}

impl Default for MoveGenBuffer {
    fn default() -> Self {
        Self {
            generator: BitBoardLarge::default(),
            g: BitBoardLarge::default(),
            p: BitBoardLarge::default(),
            shifted_g: BitBoardLarge::default(),
            shifted_p: BitBoardLarge::default(),
            temp: BitBoardLarge::default(),
        }
    }
}

thread_local! {
    static MOVE_GEN_BUFFER: RefCell<MoveGenBuffer> = RefCell::new(MoveGenBuffer::default());
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
                Player::White => &board.black_occupancy,
                Player::Black => &board.white_occupancy,
            };
            let to_idx = board.coords_to_index(&mv.to.values);
            let is_capture = if let Some(idx) = to_idx {
                enemy_occupancy.get_bit(idx)
            } else {
                false
            };
            let is_ep_capture = if let Some((ep_idx, _)) = board.en_passant_target {
                if let Some(idx) = to_idx {
                    idx == ep_idx
                        && board
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

        if is_loud {
            if !leaves_king_in_check(board, player, &mv) {
                moves.push(mv);
            }
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
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };

    MOVE_GEN_BUFFER.with(|buffer_ref| {
        let mut buffer = buffer_ref.borrow_mut();

        buffer
            .generator
            .ensure_capacity_and_clear(&board.white_occupancy);
        buffer.g.ensure_capacity_and_clear(&board.white_occupancy);
        buffer.p.ensure_capacity_and_clear(&board.white_occupancy);
        buffer
            .shifted_g
            .ensure_capacity_and_clear(&board.white_occupancy);
        buffer
            .shifted_p
            .ensure_capacity_and_clear(&board.white_occupancy);
        buffer
            .temp
            .ensure_capacity_and_clear(&board.white_occupancy);

        let MoveGenBuffer {
            generator,
            g,
            p,
            shifted_g,
            shifted_p,
            temp,
        } = &mut *buffer;

        for i in occupancy.iter_indices() {
            let coord_vals = board.index_to_coords(i);
            let coord = Coordinate::new(coord_vals.clone());

            let piece_type = if board.pawns.get_bit(i) {
                PieceType::Pawn
            } else if board.knights.get_bit(i) {
                PieceType::Knight
            } else if board.bishops.get_bit(i) {
                PieceType::Bishop
            } else if board.rooks.get_bit(i) {
                PieceType::Rook
            } else if board.queens.get_bit(i) {
                PieceType::Queen
            } else if board.kings.get_bit(i) {
                PieceType::King
            } else {
                continue;
            };

            match piece_type {
                PieceType::Pawn => generate_pawn_moves(board, &coord, player, &mut moves),
                PieceType::Knight => generate_leaper_moves(
                    board,
                    &coord,
                    player,
                    &board.cache.knight_offsets,
                    &mut moves,
                ),
                PieceType::King => generate_leaper_moves(
                    board,
                    &coord,
                    player,
                    &board.cache.king_offsets,
                    &mut moves,
                ),
                PieceType::Rook => generate_slider_moves_bitwise(
                    board,
                    i,
                    &coord,
                    player,
                    &board.cache.rook_directions,
                    &mut moves,
                    generator,
                    g,
                    p,
                    shifted_g,
                    shifted_p,
                    temp,
                ),
                PieceType::Bishop => generate_slider_moves_bitwise(
                    board,
                    i,
                    &coord,
                    player,
                    &board.cache.bishop_directions,
                    &mut moves,
                    generator,
                    g,
                    p,
                    shifted_g,
                    shifted_p,
                    temp,
                ),
                PieceType::Queen => {
                    generate_slider_moves_bitwise(
                        board,
                        i,
                        &coord,
                        player,
                        &board.cache.rook_directions,
                        &mut moves,
                        generator,
                        g,
                        p,
                        shifted_g,
                        shifted_p,
                        temp,
                    );
                    generate_slider_moves_bitwise(
                        board,
                        i,
                        &coord,
                        player,
                        &board.cache.bishop_directions,
                        &mut moves,
                        generator,
                        g,
                        p,
                        shifted_g,
                        shifted_p,
                        temp,
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
    temp: &mut BitBoardLarge,
) {
    for x in generator.data.iter_mut() {
        *x = 0;
    }
    generator.set_bit(origin_idx);

    let all_occupancy = &board.white_occupancy | &board.black_occupancy;

    let mut empty_data = SmallVec::with_capacity(all_occupancy.data.len());
    let mut remaining = board.total_cells;
    for val in &all_occupancy.data {
        let limit = std::cmp::min(64, remaining);
        let mask = if limit == 64 {
            !0u64
        } else {
            (1u64 << limit) - 1
        };
        empty_data.push((!val) & mask);
        remaining = remaining.saturating_sub(64);
    }
    let empty = BitBoardLarge { data: empty_data };

    let own_occupancy = match player {
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };

    for dir_info in directions {
        let stride = calculate_stride(board, &dir_info.offsets);
        if stride == 0 {
            continue;
        }

        g.copy_from(generator);
        p.copy_from(&empty);

        kogge_stone_fill_inplace(g, p, stride, board, dir_info, shifted_g, shifted_p, temp);

        for to_idx in g.iter_indices() {
            if to_idx == origin_idx {
                continue;
            }
            if own_occupancy.get_bit(to_idx) {
                continue;
            }

            let to_coords = board.index_to_coords(to_idx);
            moves.push(Move {
                from: origin_coord.clone(),
                to: Coordinate::new(to_coords),
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
    temp: &mut BitBoardLarge,
) {
    let mut shift_amt = 1;

    let mask_base_idx = dir_info.id * board.side;

    while shift_amt < board.side {
        let mask = unsafe {
            board
                .cache
                .validity_masks
                .get_unchecked(mask_base_idx + shift_amt)
        };

        shifted_g.copy_from(g);
        *shifted_g &= mask;
        if stride > 0 {
            *shifted_g <<= stride.abs() as usize * shift_amt;
        } else {
            *shifted_g >>= stride.abs() as usize * shift_amt;
        }

        shifted_p.copy_from(p);
        *shifted_p &= mask;
        if stride > 0 {
            *shifted_p <<= stride.abs() as usize * shift_amt;
        } else {
            *shifted_p >>= stride.abs() as usize * shift_amt;
        }

        temp.copy_from(shifted_g);
        *temp &= p;
        *g |= temp;

        *p &= shifted_p;

        shift_amt *= 2;
    }

    let mask = unsafe { board.cache.validity_masks.get_unchecked(mask_base_idx + 1) };

    *g &= mask;
    if stride > 0 {
        *g <<= stride.abs() as usize;
    } else {
        *g >>= stride.abs() as usize;
    }
}

pub fn calculate_stride(board: &Board, dir: &[isize]) -> isize {
    let mut stride = 0;
    let mut multiplier = 1;
    for i in 0..board.dimension {
        stride += dir[i] * multiplier as isize;
        multiplier *= board.side;
    }
    stride
}

fn generate_castling_moves(board: &Board, player: Player, moves: &mut MoveList) {
    if board.side != 8 {
        return;
    }
    let (rights_mask, rank) = match player {
        Player::White => (0x3, 0),
        Player::Black => (0xC, board.side - 1),
    };
    let my_rights = board.castling_rights & rights_mask;
    if my_rights == 0 {
        return;
    }

    let king_file = 4;
    let mut king_coords = vec![rank as u8; board.dimension];
    king_coords[1] = king_file;
    let king_coord = Coordinate::new(king_coords.clone());
    if is_square_attacked(board, &king_coord, player.opponent()) {
        return;
    }

    let all_occupancy = &board.white_occupancy | &board.black_occupancy;
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
        if let (Some(fi), Some(gi)) = (f_idx, g_idx) {
            if !all_occupancy.get_bit(fi) && !all_occupancy.get_bit(gi) {
                blocked = false;
            }
        }
        if !blocked {
            if !is_square_attacked(board, &Coordinate::new(f_coords), player.opponent())
                && !is_square_attacked(board, &Coordinate::new(g_coords.clone()), player.opponent())
            {
                moves.push(Move {
                    from: king_coord.clone(),
                    to: Coordinate::new(g_coords),
                    promotion: None,
                });
            }
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
        if let (Some(bi), Some(ci), Some(di)) = (b_idx, c_idx, d_idx) {
            if !all_occupancy.get_bit(bi)
                && !all_occupancy.get_bit(ci)
                && !all_occupancy.get_bit(di)
            {
                blocked = false;
            }
        }
        if !blocked {
            if !is_square_attacked(board, &Coordinate::new(d_coords), player.opponent())
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
}

fn generate_leaper_moves(
    board: &Board,
    origin: &Coordinate,
    player: Player,
    offsets: &[Vec<isize>],
    moves: &mut MoveList,
) {
    let same_occupancy = match player {
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };
    for offset in offsets {
        if let Some(target_coords) = apply_offset(&origin.values, offset, board.side) {
            if let Some(target_idx) = board.coords_to_index(&target_coords) {
                if !same_occupancy.get_bit(target_idx) {
                    moves.push(Move {
                        from: origin.clone(),
                        to: Coordinate::new(target_coords),
                        promotion: None,
                    });
                }
            }
        }
    }
}

fn generate_pawn_moves(board: &Board, origin: &Coordinate, player: Player, moves: &mut MoveList) {
    let all_occupancy = &board.white_occupancy | &board.black_occupancy;
    let enemy_occupancy = match player.opponent() {
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };
    for movement_axis in 0..board.dimension {
        if movement_axis == 1 {
            continue;
        }
        let forward_dir = match player {
            Player::White => 1,
            Player::Black => -1,
        };
        let mut forward_step = vec![0; board.dimension];
        forward_step[movement_axis] = forward_dir;
        if let Some(target) = apply_offset(&origin.values, &forward_step, board.side) {
            if let Some(idx) = board.coords_to_index(&target) {
                if !all_occupancy.get_bit(idx) {
                    add_pawn_move(origin, &target, board.side, player, moves);
                    let is_start_rank = match player {
                        Player::White => origin.values[movement_axis] == 1,
                        Player::Black => origin.values[movement_axis] as usize == board.side - 2,
                    };
                    if is_start_rank {
                        if let Some(target2) = apply_offset(&target, &forward_step, board.side) {
                            if let Some(idx2) = board.coords_to_index(&target2) {
                                if !all_occupancy.get_bit(idx2) {
                                    add_pawn_move(origin, &target2, board.side, player, moves);
                                }
                            }
                        }
                    }
                }
            }
        }
        for capture_axis in 0..board.dimension {
            if capture_axis == movement_axis {
                continue;
            }
            for s in [-1, 1] {
                let mut cap_step = forward_step.clone();
                cap_step[capture_axis] = s;
                if let Some(target) = apply_offset(&origin.values, &cap_step, board.side) {
                    if let Some(idx) = board.coords_to_index(&target) {
                        if enemy_occupancy.get_bit(idx) {
                            add_pawn_move(origin, &target, board.side, player, moves);
                        } else if let Some((ep_target, _)) = board.en_passant_target {
                            if idx == ep_target {
                                moves.push(Move {
                                    from: origin.clone(),
                                    to: Coordinate::new(target),
                                    promotion: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn add_pawn_move(
    from: &Coordinate,
    to_vals: &[u8],
    side: usize,
    player: Player,
    moves: &mut MoveList,
) {
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
    let to = Coordinate::new(to_vals.to_vec());
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
