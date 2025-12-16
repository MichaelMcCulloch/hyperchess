use crate::domain::board::board_representation::BoardRepresentation;
use smallvec::{SmallVec, smallvec};
use std::fmt::{self, Debug};
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not, Shl, ShlAssign, Shr, ShrAssign};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BitBoardLarge {
    pub data: SmallVec<[u64; 8]>,
}

impl Default for BitBoardLarge {
    fn default() -> Self {
        BitBoardLarge { data: smallvec![0] }
    }
}

impl Debug for BitBoardLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BitBoardLarge({:?})", self.data)
    }
}

impl BitBoardLarge {
    pub fn new(len: usize) -> Self {
        BitBoardLarge {
            data: smallvec![0; len],
        }
    }

    pub fn new_empty(dimension: usize, side: usize) -> Self {
        <Self as BoardRepresentation>::new_empty(dimension, side)
    }

    pub fn set_bit(&mut self, index: usize) {
        <Self as BoardRepresentation>::set_bit(self, index)
    }

    pub fn clear_bit(&mut self, index: usize) {
        <Self as BoardRepresentation>::clear_bit(self, index)
    }

    pub fn get_bit(&self, index: usize) -> bool {
        <Self as BoardRepresentation>::get_bit(self, index)
    }

    pub fn count_ones(&self) -> u32 {
        <Self as BoardRepresentation>::count_ones(self)
    }

    pub fn iter_indices(&self) -> BitIteratorLarge<'_> {
        let (first_chunk, start_idx) = if self.data.is_empty() {
            (0, 0)
        } else {
            (self.data[0], 0)
        };
        BitIteratorLarge {
            board: self,
            current_chunk_idx: start_idx,
            current_chunk: first_chunk,
        }
    }

    pub fn copy_from(&mut self, other: &Self) {
        <Self as BoardRepresentation>::copy_from(self, other)
    }

    pub fn zero_like(&self) -> Self {
        <Self as BoardRepresentation>::zero_like(self)
    }

    pub fn ensure_capacity_and_clear(&mut self, template: &Self) {
        <Self as BoardRepresentation>::ensure_capacity_and_clear(self, template)
    }
}

impl<'a> BitAnd<&'a BitBoardLarge> for BitBoardLarge {
    type Output = BitBoardLarge;
    fn bitand(self, rhs: &'a BitBoardLarge) -> BitBoardLarge {
        &self & rhs
    }
}

impl<'a> BitOr<&'a BitBoardLarge> for BitBoardLarge {
    type Output = BitBoardLarge;
    fn bitor(self, rhs: &'a BitBoardLarge) -> BitBoardLarge {
        &self | rhs
    }
}

impl BoardRepresentation for BitBoardLarge {
    type Iter<'a> = BitIteratorLarge<'a>;

    fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let len = (total_cells + 63) / 64;
        BitBoardLarge {
            data: smallvec![0u64; len],
        }
    }

    fn set_bit(&mut self, index: usize) {
        let vec_idx = index / 64;
        if vec_idx < self.data.len() {
            self.data[vec_idx] |= 1 << (index % 64);
        }
    }

    fn clear_bit(&mut self, index: usize) {
        let vec_idx = index / 64;
        if vec_idx < self.data.len() {
            self.data[vec_idx] &= !(1 << (index % 64));
        }
    }

    fn get_bit(&self, index: usize) -> bool {
        let vec_idx = index / 64;
        if let Some(chunk) = self.data.get(vec_idx) {
            (chunk & (1 << (index % 64))) != 0
        } else {
            false
        }
    }

    fn count_ones(&self) -> u32 {
        self.data.iter().map(|c| c.count_ones()).sum()
    }

    fn iter_indices(&self) -> Self::Iter<'_> {
        let (first_chunk, start_idx) = if self.data.is_empty() {
            (0, 0)
        } else {
            (self.data[0], 0)
        };
        BitIteratorLarge {
            board: self,
            current_chunk_idx: start_idx,
            current_chunk: first_chunk,
        }
    }

    fn copy_from(&mut self, other: &Self) {
        if self.data.len() != other.data.len() {
            self.data.resize(other.data.len(), 0);
        }
        self.data.copy_from_slice(&other.data);
    }

    fn zero_like(&self) -> Self {
        BitBoardLarge {
            data: smallvec![0u64; self.data.len()],
        }
    }

    fn ensure_capacity_and_clear(&mut self, template: &Self) {
        if self.data.len() != template.data.len() {
            self.data.resize(template.data.len(), 0);
        }
        for x in self.data.iter_mut() {
            *x = 0;
        }
    }
}

impl BitAndAssign<&BitBoardLarge> for BitBoardLarge {
    fn bitand_assign(&mut self, rhs: &BitBoardLarge) {
        let len = std::cmp::min(self.data.len(), rhs.data.len());
        for (l, r) in self.data.iter_mut().zip(rhs.data.iter()).take(len) {
            *l &= *r;
        }
        if self.data.len() > len {
            for l in self.data.iter_mut().skip(len) {
                *l = 0;
            }
        }
    }
}

impl BitOrAssign<&BitBoardLarge> for BitBoardLarge {
    fn bitor_assign(&mut self, rhs: &BitBoardLarge) {
        let len = std::cmp::min(self.data.len(), rhs.data.len());
        for (l, r) in self.data.iter_mut().zip(rhs.data.iter()).take(len) {
            *l |= *r;
        }
        if rhs.data.len() > self.data.len() {
            self.data.extend_from_slice(&rhs.data[len..]);
        }
    }
}

impl ShlAssign<usize> for BitBoardLarge {
    fn shl_assign(&mut self, shift: usize) {
        if shift == 0 {
            return;
        }
        let chunks_shift = shift / 64;
        let bits_shift = shift % 64;

        if chunks_shift > 0 {
            if chunks_shift >= self.data.len() {
                for x in self.data.iter_mut() {
                    *x = 0;
                }
            } else {
                for i in (chunks_shift..self.data.len()).rev() {
                    self.data[i] = self.data[i - chunks_shift];
                }
                for i in 0..chunks_shift {
                    self.data[i] = 0;
                }
            }
        }

        if bits_shift > 0 {
            let inv_shift = 64 - bits_shift;
            for i in (0..self.data.len()).rev() {
                let prev = if i > 0 { self.data[i - 1] } else { 0 };
                self.data[i] = (self.data[i] << bits_shift) | (prev >> inv_shift);
            }
        }
    }
}

impl ShrAssign<usize> for BitBoardLarge {
    fn shr_assign(&mut self, shift: usize) {
        if shift == 0 {
            return;
        }
        let chunks_shift = shift / 64;
        let bits_shift = shift % 64;

        if chunks_shift > 0 {
            if chunks_shift >= self.data.len() {
                for x in self.data.iter_mut() {
                    *x = 0;
                }
            } else {
                for i in 0..(self.data.len() - chunks_shift) {
                    self.data[i] = self.data[i + chunks_shift];
                }
                for i in (self.data.len() - chunks_shift)..self.data.len() {
                    self.data[i] = 0;
                }
            }
        }

        if bits_shift > 0 {
            let inv_shift = 64 - bits_shift;
            for i in 0..self.data.len() {
                let next = if i + 1 < self.data.len() {
                    self.data[i + 1]
                } else {
                    0
                };
                self.data[i] = (self.data[i] >> bits_shift) | (next << inv_shift);
            }
        }
    }
}

impl<'a, 'b> BitAnd<&'b BitBoardLarge> for &'a BitBoardLarge {
    type Output = BitBoardLarge;

    fn bitand(self, rhs: &'b BitBoardLarge) -> BitBoardLarge {
        let len = std::cmp::max(self.data.len(), rhs.data.len());
        let mut new_data = SmallVec::with_capacity(len);
        for i in 0..len {
            let val_a = self.data.get(i).copied().unwrap_or(0);
            let val_b = rhs.data.get(i).copied().unwrap_or(0);
            new_data.push(val_a & val_b);
        }
        BitBoardLarge { data: new_data }
    }
}

impl<'a, 'b> BitOr<&'b BitBoardLarge> for &'a BitBoardLarge {
    type Output = BitBoardLarge;

    fn bitor(self, rhs: &'b BitBoardLarge) -> BitBoardLarge {
        let len = std::cmp::max(self.data.len(), rhs.data.len());
        let mut new_data = SmallVec::with_capacity(len);
        for i in 0..len {
            let val_a = self.data.get(i).copied().unwrap_or(0);
            let val_b = rhs.data.get(i).copied().unwrap_or(0);
            new_data.push(val_a | val_b);
        }
        BitBoardLarge { data: new_data }
    }
}

impl<'a> Not for &'a BitBoardLarge {
    type Output = BitBoardLarge;

    fn not(self) -> BitBoardLarge {
        let mut new_data = SmallVec::with_capacity(self.data.len());
        for x in &self.data {
            new_data.push(!x);
        }
        BitBoardLarge { data: new_data }
    }
}

impl<'a> Shl<usize> for &'a BitBoardLarge {
    type Output = BitBoardLarge;
    fn shl(self, shift: usize) -> BitBoardLarge {
        let mut res = self.clone();
        res <<= shift;
        res
    }
}

impl<'a> Shr<usize> for &'a BitBoardLarge {
    type Output = BitBoardLarge;
    fn shr(self, shift: usize) -> BitBoardLarge {
        let mut res = self.clone();
        res >>= shift;
        res
    }
}

impl BitAnd for BitBoardLarge {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        &self & &rhs
    }
}
impl BitOr for BitBoardLarge {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        &self | &rhs
    }
}
impl Not for BitBoardLarge {
    type Output = Self;
    fn not(self) -> Self {
        !&self
    }
}
impl Shl<usize> for BitBoardLarge {
    type Output = Self;
    fn shl(self, rhs: usize) -> Self {
        &self << rhs
    }
}
impl Shr<usize> for BitBoardLarge {
    type Output = Self;
    fn shr(self, rhs: usize) -> Self {
        &self >> rhs
    }
}

pub struct BitIteratorLarge<'a> {
    board: &'a BitBoardLarge,
    current_chunk_idx: usize,
    current_chunk: u64,
}

impl<'a> Iterator for BitIteratorLarge<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_chunk != 0 {
                let trailing = self.current_chunk.trailing_zeros();
                self.current_chunk &= !(1 << trailing);
                let index = self.current_chunk_idx * 64 + trailing as usize;
                return Some(index);
            }

            self.current_chunk_idx += 1;
            if self.current_chunk_idx < self.board.data.len() {
                self.current_chunk = self.board.data[self.current_chunk_idx];
            } else {
                return None;
            }
        }
    }
}
