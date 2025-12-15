use crate::domain::board::board_representation::BoardRepresentation;
use smallvec::{SmallVec, smallvec};
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not, Shl, ShlAssign, Shr, ShrAssign};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BitBoard {
    Small(u32),
    Medium(u128),
    Large { data: SmallVec<[u64; 8]> },
}

impl Default for BitBoard {
    fn default() -> Self {
        BitBoard::Small(0)
    }
}

impl BoardRepresentation for BitBoard {
    type Iter<'a> = BitIterator<'a>;

    fn new_empty(dimension: usize, side: usize) -> Self {
        BitBoard::new_empty(dimension, side)
    }

    fn set_bit(&mut self, index: usize) {
        self.set_bit(index)
    }

    fn clear_bit(&mut self, index: usize) {
        self.clear_bit(index)
    }

    fn get_bit(&self, index: usize) -> bool {
        self.get_bit(index)
    }

    fn count_ones(&self) -> u32 {
        self.count_ones()
    }

    fn iter_indices(&self) -> Self::Iter<'_> {
        self.iter_indices()
    }

    fn copy_from(&mut self, other: &Self) {
        self.copy_from(other)
    }

    fn zero_like(&self) -> Self {
        self.zero_like()
    }

    fn ensure_capacity_and_clear(&mut self, template: &Self) {
        self.ensure_capacity_and_clear(template)
    }
}

impl BitBoard {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        if total_cells <= 32 {
            BitBoard::Small(0)
        } else if total_cells <= 128 {
            BitBoard::Medium(0)
        } else {
            let len = (total_cells + 63) / 64;
            BitBoard::Large {
                data: smallvec![0u64; len],
            }
        }
    }

    pub fn set_bit(&mut self, index: usize) {
        match self {
            BitBoard::Small(b) => *b |= 1 << index,
            BitBoard::Medium(b) => {
                *b |= 1 << index;
            }
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

    pub fn or_with(mut self, other: &Self) -> Self {
        self |= other;
        self
    }

    pub fn iter_indices(&self) -> BitIterator<'_> {
        BitIterator::new(self)
    }

    pub fn copy_from(&mut self, other: &Self) {
        match (self, other) {
            (BitBoard::Small(a), BitBoard::Small(b)) => *a = *b,
            (BitBoard::Medium(a), BitBoard::Medium(b)) => *a = *b,
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                if a.len() != b.len() {
                    a.resize(b.len(), 0);
                }
                a.copy_from_slice(b);
            }

            (this, that) => *this = that.clone(),
        }
    }

    pub fn zero_like(&self) -> Self {
        match self {
            BitBoard::Small(_) => BitBoard::Small(0),
            BitBoard::Medium(_) => BitBoard::Medium(0),
            BitBoard::Large { data } => BitBoard::Large {
                data: smallvec![0u64; data.len()],
            },
        }
    }

    pub fn ensure_capacity_and_clear(&mut self, template: &Self) {
        match (self, template) {
            (BitBoard::Small(a), BitBoard::Small(_)) => *a = 0,
            (BitBoard::Medium(a), BitBoard::Medium(_)) => *a = 0,
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                if a.len() != b.len() {
                    a.resize(b.len(), 0);
                }
                for x in a.iter_mut() {
                    *x = 0;
                }
            }
            (this, that) => *this = that.zero_like(),
        }
    }
}

impl BitAndAssign<&BitBoard> for BitBoard {
    fn bitand_assign(&mut self, rhs: &BitBoard) {
        match (&mut *self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => {
                *a &= b;
            }
            (BitBoard::Medium(a), BitBoard::Medium(b)) => {
                *a &= b;
            }
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::min(a.len(), b.len());
                for (l, r) in a.iter_mut().zip(b.iter()).take(len) {
                    *l &= *r;
                }

                if a.len() > len {
                    for l in a.iter_mut().skip(len) {
                        *l = 0;
                    }
                }
            }
            _ => {
                *self = &*self & rhs;
            }
        }
    }
}

impl BitOrAssign<&BitBoard> for BitBoard {
    fn bitor_assign(&mut self, rhs: &BitBoard) {
        match (&mut *self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => {
                *a |= b;
            }
            (BitBoard::Medium(a), BitBoard::Medium(b)) => {
                *a |= b;
            }
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::min(a.len(), b.len());
                for (l, r) in a.iter_mut().zip(b.iter()).take(len) {
                    *l |= *r;
                }

                if b.len() > a.len() {
                    a.extend_from_slice(&b[len..]);
                }
            }
            _ => {
                *self = &*self | rhs;
            }
        }
    }
}

impl ShlAssign<usize> for BitBoard {
    fn shl_assign(&mut self, shift: usize) {
        if shift == 0 {
            return;
        }
        match self {
            BitBoard::Small(b) => *b = b.wrapping_shl(shift as u32),
            BitBoard::Medium(b) => *b = b.wrapping_shl(shift as u32),
            BitBoard::Large { data } => {
                let chunks_shift = shift / 64;
                let bits_shift = shift % 64;

                if chunks_shift > 0 {
                    if chunks_shift >= data.len() {
                        for x in data.iter_mut() {
                            *x = 0;
                        }
                    } else {
                        for i in (chunks_shift..data.len()).rev() {
                            data[i] = data[i - chunks_shift];
                        }

                        for i in 0..chunks_shift {
                            data[i] = 0;
                        }
                    }
                }

                if bits_shift > 0 {
                    let inv_shift = 64 - bits_shift;
                    for i in (0..data.len()).rev() {
                        let prev = if i > 0 { data[i - 1] } else { 0 };
                        data[i] = (data[i] << bits_shift) | (prev >> inv_shift);
                    }
                }
            }
        }
    }
}

impl ShrAssign<usize> for BitBoard {
    fn shr_assign(&mut self, shift: usize) {
        if shift == 0 {
            return;
        }
        match self {
            BitBoard::Small(b) => *b = b.wrapping_shr(shift as u32),
            BitBoard::Medium(b) => *b = b.wrapping_shr(shift as u32),
            BitBoard::Large { data } => {
                let chunks_shift = shift / 64;
                let bits_shift = shift % 64;

                if chunks_shift > 0 {
                    if chunks_shift >= data.len() {
                        for x in data.iter_mut() {
                            *x = 0;
                        }
                    } else {
                        for i in 0..(data.len() - chunks_shift) {
                            data[i] = data[i + chunks_shift];
                        }

                        for i in (data.len() - chunks_shift)..data.len() {
                            data[i] = 0;
                        }
                    }
                }

                if bits_shift > 0 {
                    let inv_shift = 64 - bits_shift;
                    for i in 0..data.len() {
                        let next = if i + 1 < data.len() { data[i + 1] } else { 0 };
                        data[i] = (data[i] >> bits_shift) | (next << inv_shift);
                    }
                }
            }
        }
    }
}

impl<'a, 'b> BitAnd<&'b BitBoard> for &'a BitBoard {
    type Output = BitBoard;

    fn bitand(self, rhs: &'b BitBoard) -> BitBoard {
        match (self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => BitBoard::Small(a & b),
            (BitBoard::Medium(a), BitBoard::Medium(b)) => BitBoard::Medium(a & b),
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::max(a.len(), b.len());
                let mut new_data = SmallVec::with_capacity(len);

                for i in 0..len {
                    let val_a = a.get(i).copied().unwrap_or(0);
                    let val_b = b.get(i).copied().unwrap_or(0);
                    new_data.push(val_a & val_b);
                }
                BitBoard::Large { data: new_data }
            }
            _ => panic!("Mismatched BitBoard types in BitAnd"),
        }
    }
}

impl<'a, 'b> BitOr<&'b BitBoard> for &'a BitBoard {
    type Output = BitBoard;

    fn bitor(self, rhs: &'b BitBoard) -> BitBoard {
        match (self, rhs) {
            (BitBoard::Small(a), BitBoard::Small(b)) => BitBoard::Small(a | b),
            (BitBoard::Medium(a), BitBoard::Medium(b)) => BitBoard::Medium(a | b),
            (BitBoard::Large { data: a }, BitBoard::Large { data: b }) => {
                let len = std::cmp::max(a.len(), b.len());

                let mut new_data = SmallVec::with_capacity(len);
                for i in 0..len {
                    let val_a = a.get(i).copied().unwrap_or(0);
                    let val_b = b.get(i).copied().unwrap_or(0);
                    new_data.push(val_a | val_b);
                }
                BitBoard::Large { data: new_data }
            }
            _ => panic!("Mismatched BitBoard types in BitOr"),
        }
    }
}

impl<'a> Not for &'a BitBoard {
    type Output = BitBoard;

    fn not(self) -> BitBoard {
        match self {
            BitBoard::Small(a) => BitBoard::Small(!a),
            BitBoard::Medium(a) => BitBoard::Medium(!a),
            BitBoard::Large { data } => {
                let mut new_data = SmallVec::with_capacity(data.len());
                for x in data {
                    new_data.push(!x);
                }
                BitBoard::Large { data: new_data }
            }
        }
    }
}

impl<'a> Shl<usize> for &'a BitBoard {
    type Output = BitBoard;
    fn shl(self, shift: usize) -> BitBoard {
        let mut res = self.clone();
        res <<= shift;
        res
    }
}

impl<'a> Shr<usize> for &'a BitBoard {
    type Output = BitBoard;
    fn shr(self, shift: usize) -> BitBoard {
        let mut res = self.clone();
        res >>= shift;
        res
    }
}

impl BitAnd for BitBoard {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        &self & &rhs
    }
}
impl BitOr for BitBoard {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        &self | &rhs
    }
}
impl Not for BitBoard {
    type Output = Self;
    fn not(self) -> Self {
        !&self
    }
}
impl Shl<usize> for BitBoard {
    type Output = Self;
    fn shl(self, rhs: usize) -> Self {
        &self << rhs
    }
}
impl Shr<usize> for BitBoard {
    type Output = Self;
    fn shr(self, rhs: usize) -> Self {
        &self >> rhs
    }
}

pub struct BitIterator<'a> {
    board: &'a BitBoard,
    current_chunk_idx: usize,
    current_chunk: u64,
}

impl<'a> BitIterator<'a> {
    pub fn new(board: &'a BitBoard) -> Self {
        let (first_chunk, start_idx) = match board {
            BitBoard::Small(b) => (*b as u64, 0),
            BitBoard::Medium(b) => (*b as u64, 0),
            BitBoard::Large { data } => {
                if data.is_empty() {
                    (0, 0)
                } else {
                    (data[0], 0)
                }
            }
        };

        Self {
            board,
            current_chunk_idx: start_idx,
            current_chunk: first_chunk,
        }
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_chunk != 0 {
                let trailing = self.current_chunk.trailing_zeros();

                self.current_chunk &= !(1 << trailing);

                let index = if let BitBoard::Medium(_) = self.board {
                    self.current_chunk_idx * 64 + trailing as usize
                } else if let BitBoard::Large { .. } = self.board {
                    self.current_chunk_idx * 64 + trailing as usize
                } else {
                    trailing as usize
                };

                return Some(index);
            }

            match self.board {
                BitBoard::Small(_) => return None,
                BitBoard::Medium(b) => {
                    if self.current_chunk_idx == 0 {
                        self.current_chunk_idx = 1;
                        self.current_chunk = (b >> 64) as u64;
                    } else {
                        return None;
                    }
                }
                BitBoard::Large { data } => {
                    self.current_chunk_idx += 1;
                    if self.current_chunk_idx < data.len() {
                        self.current_chunk = data[self.current_chunk_idx];
                    } else {
                        return None;
                    }
                }
            }
        }
    }
}

impl<'a> BitAnd<&'a BitBoard> for BitBoard {
    type Output = BitBoard;
    fn bitand(self, rhs: &'a BitBoard) -> BitBoard {
        &self & rhs
    }
}

impl<'a> BitOr<&'a BitBoard> for BitBoard {
    type Output = BitBoard;
    fn bitor(self, rhs: &'a BitBoard) -> BitBoard {
        &self | rhs
    }
}
