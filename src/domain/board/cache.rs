use smallvec::SmallVec;

use crate::domain::board::bitboard::BitBoard;
use crate::domain::board::board_representation::BoardRepresentation;
use crate::domain::models::Player;

#[derive(Debug, Clone)]
pub struct DirectionInfo {
    pub id: usize,
    pub offsets: Vec<isize>,
    pub stride: isize,
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

    /// Precomputed leaper targets: for each cell index, the list of valid target indices.
    pub knight_targets: Vec<SmallVec<[usize; 16]>>,
    pub king_targets: Vec<SmallVec<[usize; 16]>>,
    pub white_pawn_capture_targets: Vec<SmallVec<[usize; 16]>>,
    pub black_pawn_capture_targets: Vec<SmallVec<[usize; 16]>>,

    /// Precomputed distance-from-center for PST evaluation.
    /// `center_dist[cell_index]` = sum of |coord[i] - center| truncated to i32.
    pub center_dist: Vec<i32>,
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

                let mut stride: isize = 0;
                let mut multiplier: usize = 1;
                for d in 0..dimension {
                    stride += dir_vec[d] * multiplier as isize;
                    multiplier *= side;
                }

                infos.push(DirectionInfo {
                    id: dir_id,
                    offsets: dir_vec,
                    stride,
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

        let precompute_targets = |offsets: &[Vec<isize>]| -> Vec<SmallVec<[usize; 16]>> {
            let mut targets: Vec<SmallVec<[usize; 16]>> = Vec::with_capacity(total_cells);
            for i in 0..total_cells {
                let coords = &index_to_coords[i];
                let mut cell_targets = SmallVec::new();
                for offset in offsets {
                    let mut valid = true;
                    let mut target_idx: usize = 0;
                    let mut multiplier: usize = 1;
                    for d in 0..dimension {
                        let val = coords[d] as isize + offset[d];
                        if val < 0 || val >= side as isize {
                            valid = false;
                            break;
                        }
                        target_idx += val as usize * multiplier;
                        multiplier *= side;
                    }
                    if valid {
                        cell_targets.push(target_idx);
                    }
                }
                targets.push(cell_targets);
            }
            targets
        };

        let knight_targets = precompute_targets(&knight_offsets);
        let king_targets = precompute_targets(&king_offsets);
        let white_pawn_capture_targets = precompute_targets(&white_pawn_capture_offsets);
        let black_pawn_capture_targets = precompute_targets(&black_pawn_capture_offsets);

        // Precompute center distance for PST
        let center = (side as f32 - 1.0) / 2.0;
        let center_dist: Vec<i32> = (0..total_cells)
            .map(|i| {
                let coords = &index_to_coords[i];
                let dist: f32 = coords.iter().map(|&c| (c as f32 - center).abs()).sum();
                dist as i32
            })
            .collect();

        Self {
            index_to_coords,
            validity_masks,
            knight_offsets,
            king_offsets,
            rook_directions: rook_infos,
            bishop_directions: bishop_infos,
            white_pawn_capture_offsets,
            black_pawn_capture_offsets,
            knight_targets,
            king_targets,
            white_pawn_capture_targets,
            black_pawn_capture_targets,
            center_dist,
        }
    }
}
