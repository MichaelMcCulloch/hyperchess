use smallvec::SmallVec;

use crate::domain::board::Board;
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{PieceType, Player};
use crate::domain::rules::apply_offset;

pub fn is_square_attacked(board: &Board, square: &Coordinate, by_player: Player) -> bool {
    let enemy_occupancy = match by_player {
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };

    for offset in &board.cache.knight_offsets {
        if let Some(target_coord) = apply_offset(&square.values, offset, board.side) {
            if let Some(target_idx) = board.coords_to_index(&target_coord) {
                if enemy_occupancy.get_bit(target_idx) && board.knights.get_bit(target_idx) {
                    return true;
                }
            }
        }
    }

    for offset in &board.cache.king_offsets {
        if let Some(target_coord) = apply_offset(&square.values, offset, board.side) {
            if let Some(target_idx) = board.coords_to_index(&target_coord) {
                if enemy_occupancy.get_bit(target_idx) && board.kings.get_bit(target_idx) {
                    return true;
                }
            }
        }
    }

    for dir in &board.cache.rook_directions {
        if scan_ray_for_threat(
            board,
            &square.values,
            dir,
            by_player,
            &[PieceType::Rook, PieceType::Queen],
        ) {
            return true;
        }
    }

    for dir in &board.cache.bishop_directions {
        if scan_ray_for_threat(
            board,
            &square.values,
            dir,
            by_player,
            &[PieceType::Bishop, PieceType::Queen],
        ) {
            return true;
        }
    }

    let pawn_attack_offsets = match by_player {
        Player::White => &board.cache.white_pawn_capture_offsets,
        Player::Black => &board.cache.black_pawn_capture_offsets,
    };

    for offset in pawn_attack_offsets {
        if let Some(target_coord) = apply_offset(&square.values, offset, board.side) {
            if let Some(target_idx) = board.coords_to_index(&target_coord) {
                if enemy_occupancy.get_bit(target_idx) && board.pawns.get_bit(target_idx) {
                    return true;
                }
            }
        }
    }

    false
}

pub fn scan_ray_for_threat(
    board: &Board,
    origin_vals: &[usize],
    direction: &[isize],
    attacker: Player,
    threat_types: &[PieceType],
) -> bool {
    let mut current: SmallVec<[usize; 4]> = SmallVec::from_slice(origin_vals);
    let enemy_occupancy = match attacker {
        Player::White => &board.white_occupancy,
        Player::Black => &board.black_occupancy,
    };

    let all_occupancy = &board.white_occupancy | &board.black_occupancy;

    loop {
        if let Some(next) = apply_offset(&current, direction, board.side) {
            if let Some(idx) = board.coords_to_index(&next) {
                if all_occupancy.get_bit(idx) {
                    if enemy_occupancy.get_bit(idx) {
                        for &t in threat_types {
                            let match_found = match t {
                                PieceType::Rook => board.rooks.get_bit(idx),
                                PieceType::Bishop => board.bishops.get_bit(idx),
                                PieceType::Queen => board.queens.get_bit(idx),
                                _ => false,
                            };
                            if match_found {
                                return true;
                            }
                        }
                        return false;
                    } else {
                        return false;
                    }
                }
                current = next;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    false
}
