use smallvec::SmallVec;

use crate::domain::board::bitboard::BitBoard;
use crate::domain::board::board_representation::BoardRepresentation;
use crate::domain::models::Player;

#[derive(Debug, Clone)]
pub struct DirectionInfo {
    pub id: usize,
    pub offsets: Vec<isize>,
}

#[derive(Debug)]
pub struct GenericBoardCache<R: BoardRepresentation> {
    pub index_to_coords: Vec<SmallVec<[u8; 8]>>,

    pub validity_masks: Vec<R>,

    pub knight_offsets: Vec<Vec<isize>>,
    pub king_offsets: Vec<Vec<isize>>,

    pub rook_directions: Vec<DirectionInfo>,
    pub bishop_directions: Vec<DirectionInfo>,

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
                coords[d] = (temp % side) as u8;
                temp /= side;
            }
            index_to_coords.push(coords);
        }

        let raw_rook = crate::domain::rules::Rules::get_rook_directions_calc(dimension);
        let raw_bishop = crate::domain::rules::Rules::get_bishop_directions_calc(dimension);

        let mut validity_masks = Vec::new();

        let mut current_id = 0;

        let mut process_dirs = |raw_dirs: Vec<Vec<isize>>| -> Vec<DirectionInfo> {
            let mut infos = Vec::new();
            for dir_vec in raw_dirs {
                let dir_id = current_id;
                current_id += 1;

                for step in 0..side {
                    let mut mask_bb = R::new_empty(dimension, side);

                    if step > 0 {
                        for i in 0..total_cells {
                            let coords = &index_to_coords[i];
                            let mut valid = true;
                            for (c, &d) in coords.iter().zip(dir_vec.iter()) {
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
                    }
                    validity_masks.push(mask_bb);
                }

                infos.push(DirectionInfo {
                    id: dir_id,
                    offsets: dir_vec,
                });
            }
            infos
        };

        let rook_infos = process_dirs(raw_rook);
        let bishop_infos = process_dirs(raw_bishop);

        let knight_offsets = crate::domain::rules::Rules::get_knight_offsets_calc(dimension);
        let king_offsets = crate::domain::rules::Rules::get_king_offsets_calc(dimension);
        let white_pawn_capture_offsets =
            crate::domain::rules::Rules::get_pawn_capture_offsets_calc(dimension, Player::White);
        let black_pawn_capture_offsets =
            crate::domain::rules::Rules::get_pawn_capture_offsets_calc(dimension, Player::Black);

        Self {
            index_to_coords,
            validity_masks,
            knight_offsets,
            king_offsets,
            rook_directions: rook_infos,
            bishop_directions: bishop_infos,
            white_pawn_capture_offsets,
            black_pawn_capture_offsets,
        }
    }
}
