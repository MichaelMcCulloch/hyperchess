use crate::domain::board::cache::DirectionInfo;
use crate::domain::board::{BoardRepresentation, GenericBoard};
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{PieceType, Player};

pub fn is_square_attacked<R: BoardRepresentation>(
    board: &GenericBoard<R>,
    square: &Coordinate,
    by_player: Player,
) -> bool {
    let sq_idx = match board.coords_to_index(&square.values) {
        Some(idx) => idx,
        None => return false,
    };
    is_square_attacked_idx(board, sq_idx, by_player)
}

pub fn is_square_attacked_idx<R: BoardRepresentation>(
    board: &GenericBoard<R>,
    sq_idx: usize,
    by_player: Player,
) -> bool {
    let enemy_occupancy = match by_player {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    // Knight attacks via precomputed targets
    for &target_idx in &board.geo.cache.knight_targets[sq_idx] {
        if enemy_occupancy.get_bit(target_idx) && board.pieces.knights.get_bit(target_idx) {
            return true;
        }
    }

    // King attacks via precomputed targets
    for &target_idx in &board.geo.cache.king_targets[sq_idx] {
        if enemy_occupancy.get_bit(target_idx) && board.pieces.kings.get_bit(target_idx) {
            return true;
        }
    }

    // Slider attacks via index-based ray walking
    for dir_info in &board.geo.cache.rook_directions {
        if scan_ray_rook_queen(board, sq_idx, dir_info, by_player) {
            return true;
        }
    }

    for dir_info in &board.geo.cache.bishop_directions {
        if scan_ray_bishop_queen(board, sq_idx, dir_info, by_player) {
            return true;
        }
    }

    // Pawn attacks via precomputed targets
    let pawn_targets = match by_player {
        Player::White => &board.geo.cache.white_pawn_capture_targets[sq_idx],
        Player::Black => &board.geo.cache.black_pawn_capture_targets[sq_idx],
    };
    for &target_idx in pawn_targets {
        if enemy_occupancy.get_bit(target_idx) && board.pieces.pawns.get_bit(target_idx) {
            return true;
        }
    }

    false
}

/// Specialized rook+queen ray scan — avoids slice/match overhead on the hot path.
#[inline]
pub fn scan_ray_rook_queen<R: BoardRepresentation>(
    board: &GenericBoard<R>,
    origin_idx: usize,
    dir_info: &DirectionInfo,
    attacker: Player,
) -> bool {
    let stride = dir_info.stride;
    let mask = &board.geo.cache.validity_masks[dir_info.id * board.side() + 1];
    let enemy_occupancy = match attacker {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    let mut idx = origin_idx;
    loop {
        if !mask.get_bit(idx) {
            return false;
        }
        idx = (idx as isize + stride) as usize;
        if board.pieces.white_occupancy.get_bit(idx) || board.pieces.black_occupancy.get_bit(idx) {
            return enemy_occupancy.get_bit(idx)
                && (board.pieces.rooks.get_bit(idx) || board.pieces.queens.get_bit(idx));
        }
    }
}

/// Specialized bishop+queen ray scan — avoids slice/match overhead on the hot path.
#[inline]
pub fn scan_ray_bishop_queen<R: BoardRepresentation>(
    board: &GenericBoard<R>,
    origin_idx: usize,
    dir_info: &DirectionInfo,
    attacker: Player,
) -> bool {
    let stride = dir_info.stride;
    let mask = &board.geo.cache.validity_masks[dir_info.id * board.side() + 1];
    let enemy_occupancy = match attacker {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    let mut idx = origin_idx;
    loop {
        if !mask.get_bit(idx) {
            return false;
        }
        idx = (idx as isize + stride) as usize;
        if board.pieces.white_occupancy.get_bit(idx) || board.pieces.black_occupancy.get_bit(idx) {
            return enemy_occupancy.get_bit(idx)
                && (board.pieces.bishops.get_bit(idx) || board.pieces.queens.get_bit(idx));
        }
    }
}

/// Generic ray scan (kept for backward compatibility / non-hot paths).
#[inline]
pub fn scan_ray_for_threat_idx<R: BoardRepresentation>(
    board: &GenericBoard<R>,
    origin_idx: usize,
    dir_info: &DirectionInfo,
    attacker: Player,
    threat_types: &[PieceType],
) -> bool {
    let stride = dir_info.stride;
    let mask = &board.geo.cache.validity_masks[dir_info.id * board.side() + 1];
    let enemy_occupancy = match attacker {
        Player::White => &board.pieces.white_occupancy,
        Player::Black => &board.pieces.black_occupancy,
    };

    let mut idx = origin_idx;
    loop {
        if !mask.get_bit(idx) {
            return false;
        }
        idx = (idx as isize + stride) as usize;
        if board.pieces.white_occupancy.get_bit(idx) || board.pieces.black_occupancy.get_bit(idx) {
            if enemy_occupancy.get_bit(idx) {
                for &t in threat_types {
                    let found = match t {
                        PieceType::Rook => board.pieces.rooks.get_bit(idx),
                        PieceType::Bishop => board.pieces.bishops.get_bit(idx),
                        PieceType::Queen => board.pieces.queens.get_bit(idx),
                        _ => false,
                    };
                    if found {
                        return true;
                    }
                }
            }
            return false;
        }
    }
}

/// Kept for backward compatibility (used by entity.rs get_smallest_attacker).
pub fn scan_ray_for_threat<R: BoardRepresentation>(
    board: &GenericBoard<R>,
    origin_vals: &[u8],
    direction: &[isize],
    attacker: Player,
    threat_types: &[PieceType],
) -> bool {
    // Find the matching DirectionInfo to get stride + mask
    let origin_idx = match board.coords_to_index(origin_vals) {
        Some(idx) => idx,
        None => return false,
    };

    // Try rook directions first, then bishop directions
    for dir_info in board
        .geo
        .cache
        .rook_directions
        .iter()
        .chain(board.geo.cache.bishop_directions.iter())
    {
        if dir_info.offsets == direction {
            return scan_ray_for_threat_idx(board, origin_idx, dir_info, attacker, threat_types);
        }
    }

    false
}
