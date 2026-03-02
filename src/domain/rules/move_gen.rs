use crate::domain::board::cache::DirectionInfo;
use crate::domain::board::{BitBoardLarge, Board};
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType, Player};
use crate::domain::rules::MoveList;
use crate::domain::rules::attacks::is_square_attacked;

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

    let all_occupancy = &board.pieces.all_occupancy;

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
            PieceType::Rook => generate_slider_moves_scalar(
                board,
                i,
                &coord,
                player,
                &board.geo.cache.rook_directions,
                &mut moves,
            ),
            PieceType::Bishop => generate_slider_moves_scalar(
                board,
                i,
                &coord,
                player,
                &board.geo.cache.bishop_directions,
                &mut moves,
            ),
            PieceType::Queen => {
                generate_slider_moves_scalar(
                    board,
                    i,
                    &coord,
                    player,
                    &board.geo.cache.rook_directions,
                    &mut moves,
                );
                generate_slider_moves_scalar(
                    board,
                    i,
                    &coord,
                    player,
                    &board.geo.cache.bishop_directions,
                    &mut moves,
                );
            }
        }
    }
    moves
}

/// Scalar ray walker for slider move generation.
/// Walks one step at a time along the ray using stride arithmetic.
/// O(side) per direction vs O(len × log(side)) for Kogge-Stone.
#[inline]
fn generate_slider_moves_scalar(
    board: &Board,
    origin_idx: usize,
    origin_coord: &Coordinate,
    player: Player,
    directions: &[DirectionInfo],
    moves: &mut MoveList,
) {
    let own_occupancy = match player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    for dir_info in directions {
        let stride = dir_info.stride;
        if stride == 0 {
            continue;
        }
        // validity_masks[dir_id * side + 1] = cells that can step 1 in this direction
        let mask = &board.geo.cache.validity_masks[dir_info.id * board.side() + 1];

        let mut idx = origin_idx;
        loop {
            // Can this cell step one more in this direction?
            if !mask.get_bit(idx) {
                break;
            }
            idx = (idx as isize + stride) as usize;

            // Hit own piece — blocked, stop
            if own_occupancy.get_bit(idx) {
                break;
            }

            moves.push(Move {
                from: origin_coord.clone(),
                to: Coordinate::new(board.geo.cache.index_to_coords[idx].clone()),
                promotion: None,
            });

            // Hit any piece (must be enemy since we checked own above) — capture, stop
            if board.pieces.all_occupancy.get_bit(idx) {
                break;
            }
        }
    }
}

/// Scalar ray walker for slider mobility counting.
/// Returns the number of squares reachable from origin_idx along given directions.
#[inline]
pub fn count_slider_mobility_scalar(
    board: &Board,
    origin_idx: usize,
    player: Player,
    directions: &[DirectionInfo],
) -> i32 {
    let own_occupancy = match player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    let mut count = 0;
    for dir_info in directions {
        let stride = dir_info.stride;
        if stride == 0 {
            continue;
        }
        let mask = &board.geo.cache.validity_masks[dir_info.id * board.side() + 1];

        let mut idx = origin_idx;
        loop {
            if !mask.get_bit(idx) {
                break;
            }
            idx = (idx as isize + stride) as usize;

            if own_occupancy.get_bit(idx) {
                break;
            }

            count += 1;

            if board.pieces.all_occupancy.get_bit(idx) {
                break;
            }
        }
    }
    count
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
