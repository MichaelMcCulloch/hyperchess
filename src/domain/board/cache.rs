use smallvec::SmallVec;
use std::collections::HashMap;

use crate::domain::board::bitboard::BitBoard;
use crate::domain::board::board_representation::BoardRepresentation;
use crate::domain::models::Player;

#[derive(Debug)]
pub struct GenericBoardCache<R: BoardRepresentation> {
    pub index_to_coords: Vec<SmallVec<[usize; 4]>>,
    pub validity_masks: HashMap<(Vec<isize>, usize), R>,

    pub knight_offsets: Vec<Vec<isize>>,
    pub king_offsets: Vec<Vec<isize>>,
    pub rook_directions: Vec<Vec<isize>>,
    pub bishop_directions: Vec<Vec<isize>>,

    pub white_pawn_capture_offsets: Vec<Vec<isize>>,
    pub black_pawn_capture_offsets: Vec<Vec<isize>>,
}

pub type BoardCache = GenericBoardCache<BitBoard>;

impl<R: BoardRepresentation> GenericBoardCache<R> {
    pub fn new(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let mut index_to_coords = Vec::with_capacity(total_cells);

        for i in 0..total_cells {
            let mut coords = SmallVec::with_capacity(dimension);
            coords.resize(dimension, 0);
            let mut temp = i;
            for d in 0..dimension {
                coords[d] = temp % side;
                temp /= side;
            }
            index_to_coords.push(coords);
        }

        let mut validity_masks = HashMap::new();

        let rook_directions = crate::domain::rules::Rules::get_rook_directions_calc(dimension);
        let bishop_directions = crate::domain::rules::Rules::get_bishop_directions_calc(dimension);
        let knight_offsets = crate::domain::rules::Rules::get_knight_offsets_calc(dimension);
        let king_offsets = crate::domain::rules::Rules::get_king_offsets_calc(dimension);

        let white_pawn_capture_offsets =
            crate::domain::rules::Rules::get_pawn_capture_offsets_calc(dimension, Player::White);
        let black_pawn_capture_offsets =
            crate::domain::rules::Rules::get_pawn_capture_offsets_calc(dimension, Player::Black);

        let all_dirs = rook_directions.iter().chain(bishop_directions.iter());

        for dir in all_dirs {
            let mut step = 1;
            while step < side {
                let mut mask_bb = R::new_empty(dimension, side);

                for i in 0..total_cells {
                    let coords = &index_to_coords[i];
                    let mut valid = true;
                    for (c, &d) in coords.iter().zip(dir.iter()) {
                        let res = *c as isize + (d * step as isize);
                        if res < 0 || res >= side as isize {
                            valid = false;
                            break;
                        }
                    }
                    if valid {
                        mask_bb.set_bit(i);
                    }
                }

                validity_masks.insert((dir.clone(), step), mask_bb);
                step *= 2;
            }
        }

        Self {
            index_to_coords,
            validity_masks,
            knight_offsets,
            king_offsets,
            rook_directions,
            bishop_directions,
            white_pawn_capture_offsets,
            black_pawn_capture_offsets,
        }
    }
}
