use crate::domain::coordinate::Coordinate;
use crate::domain::models::{BoardState, Player};
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::fmt;
use std::sync::Arc;

// Removed Copy: Vec<u64> cannot be Copy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BitBoard {
    Small(u32),
    Medium(u128),
    Large { data: Vec<u64> },
}

#[derive(Clone, Debug)]
pub enum WinningMasks {
    Small {
        masks: Vec<u32>,
        map_flat: Vec<usize>,
        map_offsets: Vec<(u32, u32)>, // (start, count)
        cell_mask_lookup: Vec<Vec<u32>>,
    },
    Medium {
        masks: Vec<u128>,
        map_flat: Vec<usize>,
        map_offsets: Vec<(u32, u32)>,
        cell_mask_lookup: Vec<Vec<u128>>,
    },
    Large {
        masks: Vec<Vec<u64>>,
        map_flat: Vec<usize>,
        map_offsets: Vec<(u32, u32)>,
    },
}

#[derive(Clone, Debug)]
pub struct BitBoardState {
    pub dimension: usize,
    pub side: usize,
    pub total_cells: usize,
    pub p1: BitBoard,
    pub p2: BitBoard,
    pub winning_masks: Arc<WinningMasks>,
}

impl BitBoardState {
    pub fn get_cell_index(&self, index: usize) -> Option<Player> {
        if self.p1.get_bit(index) {
            Some(Player::X)
        } else if self.p2.get_bit(index) {
            Some(Player::O)
        } else {
            None
        }
    }

    pub fn set_cell_index(&mut self, index: usize, player: Player) -> Result<(), String> {
        if index >= self.total_cells {
            return Err("Index out of bounds".to_string());
        }
        if self.p1.get_bit(index) || self.p2.get_bit(index) {
            return Err("Cell already occupied".to_string());
        }

        match player {
            Player::X => self.p1.set_bit(index),
            Player::O => self.p2.set_bit(index),
        }
        Ok(())
    }

    pub fn clear_cell_index(&mut self, index: usize) {
        self.p1.clear_bit(index);
        self.p2.clear_bit(index);
    }
}

impl BoardState for BitBoardState {
    fn new(dimension: usize) -> Self {
        let side: usize = 3;
        let total_cells = side.pow(dimension as u32);

        let p1 = BitBoard::new_empty(dimension, side);
        let p2 = BitBoard::new_empty(dimension, side);

        let winning_masks = Arc::new(generate_winning_masks(dimension, side));

        BitBoardState {
            dimension,
            side,
            total_cells,
            p1,
            p2,
            winning_masks,
        }
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn side(&self) -> usize {
        self.side
    }

    fn total_cells(&self) -> usize {
        self.total_cells
    }

    fn get_cell(&self, coord: &Coordinate) -> Option<Player> {
        let index = coords_to_index(&coord.values, self.side)?;
        self.get_cell_index(index)
    }

    fn set_cell(&mut self, coord: &Coordinate, player: Player) -> Result<(), String> {
        let index = coords_to_index(&coord.values, self.side)
            .ok_or_else(|| "Invalid coordinate".to_string())?;
        self.set_cell_index(index, player)
    }

    fn clear_cell(&mut self, coord: &Coordinate) {
        if let Some(index) = coords_to_index(&coord.values, self.side) {
            self.clear_cell_index(index);
        }
    }

    fn check_win(&self) -> Option<Player> {
        if self.p1.check_win(&self.winning_masks) {
            return Some(Player::X);
        }
        if self.p2.check_win(&self.winning_masks) {
            return Some(Player::O);
        }
        None
    }

    fn is_full(&self) -> bool {
        // Since BitBoard is no longer Copy, we must use reference or clone appropriately.
        // or_with now takes &self and other by value or ref?
        // Let's refactor or_with to take reference to avoid clone.
        let combined = self.p1.or_with(&self.p2);
        combined.is_full(self.total_cells)
    }
}

impl fmt::Display for BitBoardState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", crate::infrastructure::display::render_board(self))
    }
}

// --- BitBoard Implementation ---

impl BitBoard {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        if total_cells <= 32 {
            BitBoard::Small(0)
        } else if total_cells <= 128 {
            BitBoard::Medium(0)
        } else {
            let len = (total_cells + 63) / 64;
            // Removed panic, allocated vector based on required length
            BitBoard::Large { data: vec![0; len] }
        }
    }

    pub fn set_bit(&mut self, index: usize) {
        match self {
            BitBoard::Small(b) => *b |= 1 << index,
            BitBoard::Medium(b) => *b |= 1 << index,
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if vec_idx < data.len() {
                    data[vec_idx] |= 1 << (index % 64);
                }
            }
        }
    }

    pub fn clear_bit(&mut self, index: usize) {
        match self {
            BitBoard::Small(b) => *b &= !(1 << index),
            BitBoard::Medium(b) => *b &= !(1 << index),
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if vec_idx < data.len() {
                    data[vec_idx] &= !(1 << (index % 64));
                }
            }
        }
    }

    pub fn get_bit(&self, index: usize) -> bool {
        match self {
            BitBoard::Small(b) => (*b & (1 << index)) != 0,
            BitBoard::Medium(b) => (*b & (1 << index)) != 0,
            BitBoard::Large { data } => {
                let vec_idx = index / 64;
                if let Some(chunk) = data.get(vec_idx) {
                    (chunk & (1 << (index % 64))) != 0
                } else {
                    false
                }
            }
        }
    }

    pub fn count_ones(&self) -> u32 {
        match self {
            BitBoard::Small(b) => b.count_ones(),
            BitBoard::Medium(b) => b.count_ones(),
            BitBoard::Large { data } => data.iter().map(|c| c.count_ones()).sum(),
        }
    }

    pub fn or_with(&self, other: &BitBoard) -> BitBoard {
        match (self, other) {
            (BitBoard::Small(a), BitBoard::Small(b)) => BitBoard::Small(a | b),
            (BitBoard::Medium(a), BitBoard::Medium(b)) => BitBoard::Medium(a | b),
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = a.len().max(b.len());
                let mut new_data = vec![0; len];
                for i in 0..len {
                    let v1 = if i < a.len() { a[i] } else { 0 };
                    let v2 = if i < b.len() { b[i] } else { 0 };
                    new_data[i] = v1 | v2;
                }
                BitBoard::Large { data: new_data }
            }
            // Fallback for mismatched types (should not happen in valid state)
            _ => self.clone(),
        }
    }

    pub fn is_full(&self, total_cells: usize) -> bool {
        self.count_ones() as usize >= total_cells
    }

    pub fn check_win(&self, winning_masks: &WinningMasks) -> bool {
        match (self, winning_masks) {
            (BitBoard::Small(board), WinningMasks::Small { masks, .. }) => unsafe {
                check_win_u32_opt(*board, masks)
            },
            (BitBoard::Medium(board), WinningMasks::Medium { masks, .. }) => unsafe {
                check_win_u128_opt(*board, masks)
            },
            (BitBoard::Large { data: board }, WinningMasks::Large { masks, .. }) => {
                masks.iter().any(|mask_chunks| {
                    if board.len() < mask_chunks.len() {
                        return false;
                    }
                    // Zip iteration is safer and cleaner
                    mask_chunks
                        .iter()
                        .zip(board.iter())
                        .all(|(m, b)| (b & m) == *m)
                })
            }
            _ => false,
        }
    }

    // Unused in current AI but kept for API consistency
    pub fn check_win_at(&self, winning_masks: &WinningMasks, index: usize) -> bool {
        match (self, winning_masks) {
            (
                BitBoard::Small(board),
                WinningMasks::Small {
                    cell_mask_lookup, ..
                },
            ) => {
                if let Some(masks_for_cell) = cell_mask_lookup.get(index) {
                    for &m in masks_for_cell {
                        if (board & m) == m {
                            return true;
                        }
                    }
                }
                false
            }
            (
                BitBoard::Medium(board),
                WinningMasks::Medium {
                    cell_mask_lookup, ..
                },
            ) => {
                if let Some(masks_for_cell) = cell_mask_lookup.get(index) {
                    for &m in masks_for_cell {
                        if (board & m) == m {
                            return true;
                        }
                    }
                }
                false
            }
            (
                BitBoard::Large { data: board },
                WinningMasks::Large {
                    masks,
                    map_flat,
                    map_offsets,
                },
            ) => {
                if index < map_offsets.len() {
                    let (start, count) = map_offsets[index];
                    let range = start as usize..(start + count) as usize;
                    for &i in &map_flat[range] {
                        let mask_chunks = &masks[i];
                        let mut match_all = true;
                        for (k, m) in mask_chunks.iter().enumerate() {
                            if let Some(b) = board.get(k) {
                                if (b & *m) != *m {
                                    match_all = false;
                                    break;
                                }
                            } else {
                                // Board smaller than mask (shouldn't happen)
                                match_all = false;
                                break;
                            }
                        }
                        if match_all {
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
}

// --- Mask Generation Logic ---

fn generate_winning_masks(dimension: usize, side: usize) -> WinningMasks {
    let lines_indices = generate_winning_lines_indices(dimension, side);
    let total_cells = side.pow(dimension as u32);

    let mut map_flat = Vec::new();
    let mut map_offsets = Vec::with_capacity(total_cells);

    // Build temporary map to group lines by cell
    let mut temp_map: Vec<Vec<usize>> = vec![vec![]; total_cells];
    for (line_idx, line) in lines_indices.iter().enumerate() {
        for &cell_idx in line {
            temp_map[cell_idx].push(line_idx);
        }
    }

    // Flatten
    for indices in temp_map {
        let start = map_flat.len() as u32;
        let count = indices.len() as u32;
        map_flat.extend(indices);
        map_offsets.push((start, count));
    }

    if total_cells <= 32 {
        let mut masks = Vec::new();
        let mut cell_mask_lookup = vec![Vec::new(); total_cells];
        for line in lines_indices {
            let mut mask: u32 = 0;
            for &idx in &line {
                mask |= 1 << idx;
            }
            masks.push(mask);
            for idx in line {
                cell_mask_lookup[idx].push(mask);
            }
        }
        WinningMasks::Small {
            masks,
            map_flat,
            map_offsets,
            cell_mask_lookup,
        }
    } else if total_cells <= 128 {
        let mut masks = Vec::new();
        let mut cell_mask_lookup = vec![Vec::new(); total_cells];
        for line in lines_indices {
            let mut mask: u128 = 0;
            for &idx in &line {
                mask |= 1 << idx;
            }
            masks.push(mask);
            for idx in line {
                cell_mask_lookup[idx].push(mask);
            }
        }
        WinningMasks::Medium {
            masks,
            map_flat,
            map_offsets,
            cell_mask_lookup,
        }
    } else {
        let mut masks = Vec::new();
        let num_u64s = (total_cells + 63) / 64;
        for line in lines_indices {
            let mut mask_chunks = vec![0u64; num_u64s];
            for idx in line {
                let vec_idx = idx / 64;
                mask_chunks[vec_idx] |= 1 << (idx % 64);
            }
            masks.push(mask_chunks);
        }
        WinningMasks::Large {
            masks,
            map_flat,
            map_offsets,
        }
    }
}

// ... [Existing helper functions: generate_winning_lines_indices, get_canonical_directions, etc.] ...
fn generate_winning_lines_indices(dimension: usize, side: usize) -> Vec<Vec<usize>> {
    let mut lines = Vec::new();
    let all_directions = get_canonical_directions(dimension);

    for dir in all_directions {
        let valid_starts = get_valid_starts(dimension, side, &dir);
        for start in valid_starts {
            let mut line = Vec::new();
            let mut current = start.clone();
            let mut valid = true;

            for _ in 0..side {
                if let Some(idx) = coords_to_index(&current, side) {
                    line.push(idx);

                    for (i, d) in dir.iter().enumerate() {
                        let next_val = current[i] as isize + d;
                        current[i] = next_val as usize;
                    }
                } else {
                    valid = false;
                    break;
                }
            }

            if valid && line.len() == side {
                lines.push(line);
            }
        }
    }
    lines
}

fn get_canonical_directions(dimension: usize) -> Vec<Vec<isize>> {
    let mut dirs = Vec::new();
    let num_dirs = 3_usize.pow(dimension as u32);
    for i in 0..num_dirs {
        let mut dir = Vec::new();
        let mut temp = i;
        let mut has_nonzero = false;
        let mut first_nonzero_is_positive = false;
        for _ in 0..dimension {
            let digit = temp % 3;
            temp /= 3;
            let val = match digit {
                0 => 0,
                1 => 1,
                2 => -1,
                _ => unreachable!(),
            };
            dir.push(val);
        }
        for &val in &dir {
            if val != 0 {
                has_nonzero = true;
                if val > 0 {
                    first_nonzero_is_positive = true;
                }
                break;
            }
        }
        if has_nonzero && first_nonzero_is_positive {
            dirs.push(dir);
        }
    }
    dirs
}

fn get_valid_starts(dimension: usize, side: usize, dir: &[isize]) -> Vec<Vec<usize>> {
    let num_cells = side.pow(dimension as u32);
    let mut starts = Vec::new();
    for i in 0..num_cells {
        let coords = index_to_coords(i, dimension, side);
        let end_coords = coords.clone();
        let mut possible = true;
        for (c_idx, &d) in dir.iter().enumerate() {
            let start_val = end_coords[c_idx] as isize;
            let end_val = start_val + d * (side as isize - 1);
            if end_val < 0 || end_val >= side as isize {
                possible = false;
                break;
            }
        }
        if possible {
            starts.push(coords);
        }
    }
    starts
}

pub fn index_to_coords(index: usize, dimension: usize, side: usize) -> Vec<usize> {
    let mut coords = vec![0; dimension];
    let mut temp = index;
    for i in 0..dimension {
        coords[i] = temp % side;
        temp /= side;
    }
    coords
}

pub fn coords_to_index(coords: &[usize], side: usize) -> Option<usize> {
    let mut index = 0;
    let mut multiplier = 1;
    for &c in coords {
        if c >= side {
            return None;
        }
        index += c * multiplier;
        multiplier *= side;
    }
    Some(index)
}

// ... [Include existing intrinsic optimized check_win implementations here] ...
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u32_opt(board: u32, masks: &[u32]) -> bool {
    let board_vec = unsafe { _mm256_set1_epi32(board as i32) };
    let chunks = masks.chunks_exact(8);
    let remainder = chunks.remainder();
    for chunk in chunks {
        unsafe {
            let mask_vec = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            let and_res = _mm256_and_si256(board_vec, mask_vec);
            let cmp = _mm256_cmpeq_epi32(and_res, mask_vec);
            if _mm256_movemask_epi8(cmp) != 0 {
                return true;
            }
        }
    }
    for &m in remainder {
        if (board & m) == m {
            return true;
        }
    }
    false
}

// Fallbacks for non-AVX... (omitted for brevity, keep existing implementations)
#[cfg(not(target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u32_opt(board: u32, masks: &[u32]) -> bool {
    masks.iter().any(|&m| (board & m) == m)
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u128_opt(board: u128, masks: &[u128]) -> bool {
    let board_low = board as u64;
    let board_high = (board >> 64) as u64;
    let board_vec = unsafe {
        _mm256_set_epi64x(
            board_high as i64,
            board_low as i64,
            board_high as i64,
            board_low as i64,
        )
    };
    let chunks = masks.chunks_exact(2);
    let remainder = chunks.remainder();
    for chunk in chunks {
        unsafe {
            let mask_vec = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            let and_res = _mm256_and_si256(board_vec, mask_vec);
            let cmp = _mm256_cmpeq_epi64(and_res, mask_vec);
            let mask_bits = _mm256_movemask_epi8(cmp);
            if (mask_bits & 0xFFFF) == 0xFFFF {
                return true;
            }
            if (mask_bits as u32 & 0xFFFF0000) == 0xFFFF0000 {
                return true;
            }
        }
    }
    for &m in remainder {
        if (board & m) == m {
            return true;
        }
    }
    false
}

#[cfg(not(target_feature = "avx2"))]
#[inline]
unsafe fn check_win_u128_opt(board: u128, masks: &[u128]) -> bool {
    masks.iter().any(|&m| (board & m) == m)
}
