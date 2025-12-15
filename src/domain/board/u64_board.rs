use crate::domain::board::board_representation::BoardRepresentation;
use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not, Shl, ShlAssign, Shr, ShrAssign};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct BitBoard64(pub u64);

impl fmt::Binary for BitBoard64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Binary::fmt(&self.0, f)
    }
}

impl BoardRepresentation for BitBoard64 {
    type Iter<'a> = BitIterator64;

    fn new_empty(_dimension: usize, _side: usize) -> Self {
        BitBoard64(0)
    }

    fn set_bit(&mut self, index: usize) {
        self.0 |= 1u64 << index;
    }

    fn clear_bit(&mut self, index: usize) {
        self.0 &= !(1u64 << index);
    }

    fn get_bit(&self, index: usize) -> bool {
        (self.0 & (1u64 << index)) != 0
    }

    fn count_ones(&self) -> u32 {
        self.0.count_ones()
    }

    fn iter_indices(&self) -> Self::Iter<'_> {
        BitIterator64 { current: self.0 }
    }

    fn copy_from(&mut self, other: &Self) {
        self.0 = other.0;
    }

    fn zero_like(&self) -> Self {
        BitBoard64(0)
    }

    fn ensure_capacity_and_clear(&mut self, _template: &Self) {
        self.0 = 0;
    }
}

impl BitAnd for BitBoard64 {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        BitBoard64(self.0 & rhs.0)
    }
}

impl BitOr for BitBoard64 {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        BitBoard64(self.0 | rhs.0)
    }
}

impl Not for BitBoard64 {
    type Output = Self;
    fn not(self) -> Self::Output {
        BitBoard64(!self.0)
    }
}

impl BitAndAssign for BitBoard64 {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitOrAssign for BitBoard64 {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl Shl<usize> for BitBoard64 {
    type Output = Self;
    fn shl(self, rhs: usize) -> Self::Output {
        BitBoard64(self.0 << rhs)
    }
}

impl Shr<usize> for BitBoard64 {
    type Output = Self;
    fn shr(self, rhs: usize) -> Self::Output {
        BitBoard64(self.0 >> rhs)
    }
}

impl ShlAssign<usize> for BitBoard64 {
    fn shl_assign(&mut self, rhs: usize) {
        self.0 <<= rhs;
    }
}

impl ShrAssign<usize> for BitBoard64 {
    fn shr_assign(&mut self, rhs: usize) {
        self.0 >>= rhs;
    }
}

impl<'a> BitAnd<&'a BitBoard64> for BitBoard64 {
    type Output = BitBoard64;
    fn bitand(self, rhs: &'a BitBoard64) -> BitBoard64 {
        BitBoard64(self.0 & rhs.0)
    }
}

impl<'a> BitOr<&'a BitBoard64> for BitBoard64 {
    type Output = BitBoard64;
    fn bitor(self, rhs: &'a BitBoard64) -> BitBoard64 {
        BitBoard64(self.0 | rhs.0)
    }
}

impl<'a> BitAndAssign<&'a BitBoard64> for BitBoard64 {
    fn bitand_assign(&mut self, rhs: &'a BitBoard64) {
        self.0 &= rhs.0;
    }
}

impl<'a> BitOrAssign<&'a BitBoard64> for BitBoard64 {
    fn bitor_assign(&mut self, rhs: &'a BitBoard64) {
        self.0 |= rhs.0;
    }
}

pub struct BitIterator64 {
    current: u64,
}

impl Iterator for BitIterator64 {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == 0 {
            return None;
        }
        let trailing = self.current.trailing_zeros();
        self.current &= !(1u64 << trailing);
        Some(trailing as usize)
    }
}
