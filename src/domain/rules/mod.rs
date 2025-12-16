pub mod attacks;
pub mod calculators;
pub mod mobility;
pub mod move_gen;

use smallvec::SmallVec;

use crate::domain::board::{Board, BoardRepresentation, GenericBoard};
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType, Player};
pub type MoveList = SmallVec<[Move; 64]>;

pub struct Rules;

impl Rules {
    pub fn is_square_attacked<R: BoardRepresentation>(
        board: &GenericBoard<R>,
        square: &Coordinate,
        by_player: Player,
    ) -> bool {
        attacks::is_square_attacked(board, square, by_player)
    }

    pub fn scan_ray_for_threat<R: BoardRepresentation>(
        board: &GenericBoard<R>,
        origin_vals: &[u8],
        direction: &[isize],
        attacker: Player,
        threat_types: &[PieceType],
    ) -> bool {
        attacks::scan_ray_for_threat(board, origin_vals, direction, attacker, threat_types)
    }

    pub fn get_rook_directions_calc(dimension: usize) -> Vec<Vec<isize>> {
        calculators::get_rook_directions_calc(dimension)
    }
    pub fn get_bishop_directions_calc(dimension: usize) -> Vec<Vec<isize>> {
        calculators::get_bishop_directions_calc(dimension)
    }
    pub fn get_knight_offsets_calc(dimension: usize) -> Vec<Vec<isize>> {
        calculators::get_knight_offsets_calc(dimension)
    }
    pub fn get_king_offsets_calc(dimension: usize) -> Vec<Vec<isize>> {
        calculators::get_king_offsets_calc(dimension)
    }
    pub fn get_pawn_capture_offsets_calc(dimension: usize, attacker: Player) -> Vec<Vec<isize>> {
        calculators::get_pawn_capture_offsets_calc(dimension, attacker)
    }

    pub fn count_piece_mobility(board: &Board, index: usize, piece_type: PieceType) -> i32 {
        mobility::count_piece_mobility(board, index, piece_type)
    }

    pub fn generate_legal_moves(board: &mut Board, player: Player) -> MoveList {
        move_gen::generate_legal_moves(board, player)
    }

    pub fn generate_loud_moves(board: &mut Board, player: Player) -> MoveList {
        move_gen::generate_loud_moves(board, player)
    }

    pub fn leaves_king_in_check(board: &mut Board, player: Player, mv: &Move) -> bool {
        move_gen::leaves_king_in_check(board, player, mv)
    }

    pub fn apply_offset(coords: &[u8], offset: &[isize], side: usize) -> Option<SmallVec<[u8; 8]>> {
        apply_offset(coords, offset, side)
    }
}

pub fn apply_offset(coords: &[u8], offset: &[isize], side: usize) -> Option<SmallVec<[u8; 8]>> {
    let mut new_coords = SmallVec::with_capacity(coords.len());
    for (c, &o) in coords.iter().zip(offset.iter()) {
        let val = *c as isize + o;
        if val < 0 || val >= side as isize {
            return None;
        }
        new_coords.push(val as u8);
    }
    Some(new_coords)
}
