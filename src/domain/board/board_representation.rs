use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not, Shl, ShlAssign, Shr, ShrAssign};

pub trait BoardRepresentation:
    Clone
    + Debug
    + PartialEq
    + Eq
    + Hash
    + Default
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + Not<Output = Self>
    + for<'a> BitAnd<&'a Self, Output = Self>
    + for<'a> BitOr<&'a Self, Output = Self>
    + for<'a> BitAndAssign<&'a Self>
    + for<'a> BitOrAssign<&'a Self>
    + Shl<usize, Output = Self>
    + Shr<usize, Output = Self>
    + ShlAssign<usize>
    + ShrAssign<usize>
    + Send
    + Sync
{
    type Iter<'a>: Iterator<Item = usize>
    where
        Self: 'a;

    fn new_empty(dimension: usize, side: usize) -> Self;

    fn set_bit(&mut self, index: usize);
    fn clear_bit(&mut self, index: usize);
    fn get_bit(&self, index: usize) -> bool;

    fn count_ones(&self) -> u32;
    fn iter_indices(&self) -> Self::Iter<'_>;

    fn copy_from(&mut self, other: &Self);
    fn zero_like(&self) -> Self;
    fn ensure_capacity_and_clear(&mut self, template: &Self);
}
