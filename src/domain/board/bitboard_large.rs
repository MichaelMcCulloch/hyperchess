use crate::domain::board::board_representation::BoardRepresentation;
use smallvec::{SmallVec, smallvec};
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not, Shl, ShlAssign, Shr, ShrAssign};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

// ─── SIMD helpers (AVX2 on x86_64, scalar fallback) ──────────────────

/// AND `count` u64 words: dst[i] &= src[i].
#[inline(always)]
unsafe fn simd_and_assign(dst: *mut u64, src: *const u64, count: usize) {
    unsafe {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        {
            let mut i = 0;
            while i + 4 <= count {
                let a = _mm256_loadu_si256((dst as *const u8).add(i * 8) as *const __m256i);
                let b = _mm256_loadu_si256((src as *const u8).add(i * 8) as *const __m256i);
                _mm256_storeu_si256(
                    (dst as *mut u8).add(i * 8) as *mut __m256i,
                    _mm256_and_si256(a, b),
                );
                i += 4;
            }
            while i < count {
                *dst.add(i) &= *src.add(i);
                i += 1;
            }
        }
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        {
            for i in 0..count {
                *dst.add(i) &= *src.add(i);
            }
        }
    }
}

/// OR `count` u64 words: dst[i] |= src[i].
#[inline(always)]
unsafe fn simd_or_assign(dst: *mut u64, src: *const u64, count: usize) {
    unsafe {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        {
            let mut i = 0;
            while i + 4 <= count {
                let a = _mm256_loadu_si256((dst as *const u8).add(i * 8) as *const __m256i);
                let b = _mm256_loadu_si256((src as *const u8).add(i * 8) as *const __m256i);
                _mm256_storeu_si256(
                    (dst as *mut u8).add(i * 8) as *mut __m256i,
                    _mm256_or_si256(a, b),
                );
                i += 4;
            }
            while i < count {
                *dst.add(i) |= *src.add(i);
                i += 1;
            }
        }
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        {
            for i in 0..count {
                *dst.add(i) |= *src.add(i);
            }
        }
    }
}

/// ANDNOT `count` u64 words: dst[i] &= !src[i].
#[inline(always)]
unsafe fn simd_andnot_assign(dst: *mut u64, src: *const u64, count: usize) {
    unsafe {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        {
            let mut i = 0;
            while i + 4 <= count {
                let a = _mm256_loadu_si256((dst as *const u8).add(i * 8) as *const __m256i);
                let b = _mm256_loadu_si256((src as *const u8).add(i * 8) as *const __m256i);
                // andnot(b, a) = !b & a — we want dst & !src
                _mm256_storeu_si256(
                    (dst as *mut u8).add(i * 8) as *mut __m256i,
                    _mm256_andnot_si256(b, a),
                );
                i += 4;
            }
            while i < count {
                *dst.add(i) &= !*src.add(i);
                i += 1;
            }
        }
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        {
            for i in 0..count {
                *dst.add(i) &= !*src.add(i);
            }
        }
    }
}

/// Zero `count` u64 words.
#[inline(always)]
unsafe fn simd_zero(dst: *mut u64, count: usize) {
    unsafe {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        {
            let mut i = 0;
            let zero = _mm256_setzero_si256();
            while i + 4 <= count {
                _mm256_storeu_si256((dst as *mut u8).add(i * 8) as *mut __m256i, zero);
                i += 4;
            }
            while i < count {
                *dst.add(i) = 0;
                i += 1;
            }
        }
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        {
            for i in 0..count {
                *dst.add(i) = 0;
            }
        }
    }
}

/// OR two source slices into dst: dst[i] = a[i] | b[i].
#[inline(always)]
unsafe fn simd_or_into(dst: *mut u64, a: *const u64, b: *const u64, count: usize) {
    unsafe {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        {
            let mut i = 0;
            while i + 4 <= count {
                let va = _mm256_loadu_si256((a as *const u8).add(i * 8) as *const __m256i);
                let vb = _mm256_loadu_si256((b as *const u8).add(i * 8) as *const __m256i);
                _mm256_storeu_si256(
                    (dst as *mut u8).add(i * 8) as *mut __m256i,
                    _mm256_or_si256(va, vb),
                );
                i += 4;
            }
            while i < count {
                *dst.add(i) = *a.add(i) | *b.add(i);
                i += 1;
            }
        }
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        {
            for i in 0..count {
                *dst.add(i) = *a.add(i) | *b.add(i);
            }
        }
    }
}

/// Bitboard for N-dimensional boards.
/// Tracks `lo`/`hi` (inclusive range of potentially non-zero u64 words)
/// to avoid scanning the full data array on shifts and other operations.
/// Invariant: all words outside `lo..=hi` are zero. `lo > hi` means all-zero.
#[derive(Clone)]
pub struct BitBoardLarge {
    pub data: SmallVec<[u64; 8]>,
    /// Lowest index that may be non-zero (inclusive).
    lo: u16,
    /// Highest index that may be non-zero (inclusive). If lo > hi, board is all-zero.
    hi: u16,
}

impl PartialEq for BitBoardLarge {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}
impl Eq for BitBoardLarge {}

impl Hash for BitBoardLarge {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
}

impl Default for BitBoardLarge {
    fn default() -> Self {
        BitBoardLarge {
            data: smallvec![0],
            lo: 1,
            hi: 0,
        }
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
            lo: 1,
            hi: 0,
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
        if self.lo > self.hi {
            return BitIteratorLarge {
                board: self,
                current_chunk_idx: self.data.len(),
                current_chunk: 0,
            };
        }
        BitIteratorLarge {
            board: self,
            current_chunk_idx: self.lo as usize,
            current_chunk: self.data[self.lo as usize],
        }
    }

    pub fn copy_from(&mut self, other: &Self) {
        <Self as BoardRepresentation>::copy_from(self, other)
    }

    /// Lowest u64 word index that may be non-zero (inclusive).
    #[inline(always)]
    pub fn lo(&self) -> usize {
        self.lo as usize
    }

    /// Highest u64 word index that may be non-zero (inclusive).
    #[inline(always)]
    pub fn hi(&self) -> usize {
        self.hi as usize
    }

    /// Whether the tracked range is empty (all-zero board).
    #[inline(always)]
    pub fn is_range_empty(&self) -> bool {
        self.lo > self.hi
    }

    /// Set the lo/hi range directly (must uphold invariant: words outside are zero).
    #[inline(always)]
    pub fn set_range(&mut self, lo: usize, hi: usize) {
        self.lo = lo as u16;
        self.hi = hi as u16;
    }

    pub fn zero_like(&self) -> Self {
        <Self as BoardRepresentation>::zero_like(self)
    }

    pub fn ensure_capacity_and_clear(&mut self, template: &Self) {
        <Self as BoardRepresentation>::ensure_capacity_and_clear(self, template)
    }

    /// Recompute lo/hi from scratch. Use sparingly (after ops that may shrink range).
    #[inline]
    pub fn recompute_range(&mut self) {
        let len = self.data.len();
        let mut new_lo = len;
        let mut new_hi = 0;
        for (i, &v) in self.data.iter().enumerate() {
            if v != 0 {
                if i < new_lo {
                    new_lo = i;
                }
                new_hi = i;
            }
        }
        if new_lo > new_hi {
            // all zero
            self.lo = 1;
            self.hi = 0;
        } else {
            self.lo = new_lo as u16;
            self.hi = new_hi as u16;
        }
    }

    /// Check if the board is all-zero based on tracked range.
    #[inline]
    pub fn is_empty_board(&self) -> bool {
        self.lo > self.hi
    }

    /// Fused `self &= !rhs` without allocating the NOT intermediate.
    #[inline]
    pub fn andnot_assign(&mut self, rhs: &Self) {
        if self.lo > self.hi {
            return;
        }
        let lo = self.lo as usize;
        let hi = std::cmp::min(self.hi as usize, rhs.data.len().saturating_sub(1));
        if lo > hi {
            return;
        }
        let ptr = self.data.as_mut_ptr();
        let rptr = rhs.data.as_ptr();
        unsafe {
            simd_andnot_assign(ptr.add(lo), rptr.add(lo), hi - lo + 1);
        }
        // Range may have shrunk but we keep conservative bounds
    }

    /// Compute `self | other` into a pre-existing destination buffer.
    /// `dest` must already have correct capacity.
    #[inline]
    pub fn or_into(&self, other: &Self, dest: &mut Self) {
        debug_assert!(dest.data.len() >= std::cmp::max(self.data.len(), other.data.len()));

        let self_empty = self.lo > self.hi;
        let other_empty = other.lo > other.hi;

        if self_empty && other_empty {
            dest.lo = 1;
            dest.hi = 0;
            return;
        }

        // Compute union range
        let (dlo, dhi) = if self_empty {
            (other.lo as usize, other.hi as usize)
        } else if other_empty {
            (self.lo as usize, self.hi as usize)
        } else {
            (
                std::cmp::min(self.lo as usize, other.lo as usize),
                std::cmp::max(self.hi as usize, other.hi as usize),
            )
        };

        let dptr = dest.data.as_mut_ptr();
        let alen = self.data.len();
        let blen = other.data.len();

        // Both slices cover the union range — use SIMD OR
        if dhi < alen && dhi < blen {
            let aptr = self.data.as_ptr();
            let bptr = other.data.as_ptr();
            unsafe {
                simd_or_into(dptr.add(dlo), aptr.add(dlo), bptr.add(dlo), dhi - dlo + 1);
            }
        } else {
            // Fallback: one is shorter
            let aptr = self.data.as_ptr();
            let bptr = other.data.as_ptr();
            for i in dlo..=dhi {
                unsafe {
                    let a = if i < alen { *aptr.add(i) } else { 0 };
                    let b = if i < blen { *bptr.add(i) } else { 0 };
                    *dptr.add(i) = a | b;
                }
            }
        }

        dest.lo = dlo as u16;
        dest.hi = dhi as u16;
    }

    /// Compute `!self & mask` into self (masked NOT for valid board cells).
    #[inline]
    pub fn invert_masked(&mut self, total_cells: usize) {
        let mut remaining = total_cells;
        for val in self.data.iter_mut() {
            let limit = std::cmp::min(64, remaining);
            let mask = if limit == 64 {
                !0u64
            } else {
                (1u64 << limit) - 1
            };
            *val = (!*val) & mask;
            remaining = remaining.saturating_sub(64);
        }
        self.recompute_range();
    }

    /// Fused `self = a & b` — copies AND result into self. Avoids separate copy + AND.
    #[inline]
    pub fn copy_and(&mut self, a: &Self, b: &Self) {
        // Compute intersection range of a and b
        if a.lo > a.hi || b.lo > b.hi {
            // Result is all-zero — clear old active range
            if self.lo <= self.hi {
                let slo = self.lo as usize;
                let shi = self.hi as usize;
                unsafe {
                    simd_zero(self.data.as_mut_ptr().add(slo), shi - slo + 1);
                }
            }
            self.lo = 1;
            self.hi = 0;
            return;
        }
        let lo = std::cmp::max(a.lo as usize, b.lo as usize);
        let hi = std::cmp::min(a.hi as usize, b.hi as usize);
        if lo > hi {
            if self.lo <= self.hi {
                let slo = self.lo as usize;
                let shi = self.hi as usize;
                unsafe {
                    simd_zero(self.data.as_mut_ptr().add(slo), shi - slo + 1);
                }
            }
            self.lo = 1;
            self.hi = 0;
            return;
        }

        let dptr = self.data.as_mut_ptr();

        // Only zero parts of old range outside new range
        if self.lo <= self.hi {
            let slo = self.lo as usize;
            let shi = self.hi as usize;
            let zero_below = lo.saturating_sub(slo).min(shi - slo + 1);
            if zero_below > 0 {
                unsafe {
                    simd_zero(dptr.add(slo), zero_below);
                }
            }
            let above_start = (hi + 1).max(slo);
            if above_start <= shi {
                unsafe {
                    simd_zero(dptr.add(above_start), shi - above_start + 1);
                }
            }
        }

        let aptr = a.data.as_ptr();
        let bptr = b.data.as_ptr();
        let count = hi - lo + 1;

        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        unsafe {
            let mut i = 0;
            while i + 4 <= count {
                let off = lo + i;
                let va = _mm256_loadu_si256((aptr as *const u8).add(off * 8) as *const __m256i);
                let vb = _mm256_loadu_si256((bptr as *const u8).add(off * 8) as *const __m256i);
                _mm256_storeu_si256(
                    (dptr as *mut u8).add(off * 8) as *mut __m256i,
                    _mm256_and_si256(va, vb),
                );
                i += 4;
            }
            while i < count {
                let off = lo + i;
                *dptr.add(off) = *aptr.add(off) & *bptr.add(off);
                i += 1;
            }
        }
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        unsafe {
            for i in lo..=hi {
                *dptr.add(i) = *aptr.add(i) & *bptr.add(i);
            }
        }

        self.lo = lo as u16;
        self.hi = hi as u16;
    }

    /// Fused `self = (a & b) << shift`. Computes AND and left-shift in one pass.
    #[inline]
    pub fn copy_and_shl(&mut self, a: &Self, b: &Self, shift: usize) {
        if a.lo > a.hi || b.lo > b.hi {
            self.clear_active_range();
            return;
        }
        let src_lo = std::cmp::max(a.lo as usize, b.lo as usize);
        let src_hi = std::cmp::min(a.hi as usize, b.hi as usize);
        if src_lo > src_hi {
            self.clear_active_range();
            return;
        }

        let len = self.data.len();
        let chunks_shift = shift / 64;
        let bits_shift = shift % 64;

        let dst_lo = src_lo + chunks_shift;
        if dst_lo >= len {
            self.clear_active_range();
            return;
        }
        let mut dst_hi = (src_hi + chunks_shift).min(len - 1);

        let dptr = self.data.as_mut_ptr();
        let aptr = a.data.as_ptr();
        let bptr = b.data.as_ptr();

        // Clear old active range outside new destination range
        self.clear_outside_range(dst_lo, dst_hi + 1); // +1 because carry might extend

        if bits_shift == 0 {
            // Pure chunk shift — just AND and place
            for src_i in src_lo..=src_hi {
                let dst_i = src_i + chunks_shift;
                if dst_i >= len {
                    break;
                }
                unsafe {
                    *dptr.add(dst_i) = *aptr.add(src_i) & *bptr.add(src_i);
                }
            }
        } else {
            let inv_shift = 64 - bits_shift;
            // Check if carry from top word extends dst_hi
            let top_and = unsafe { *aptr.add(src_hi) & *bptr.add(src_hi) };
            let top_carry = top_and >> inv_shift;
            if top_carry != 0 && dst_hi + 1 < len {
                dst_hi += 1;
                unsafe {
                    *dptr.add(dst_hi) = top_carry;
                }
            }
            // Process words top-down, caching AND to avoid recomputation.
            // Each dst word = (and_word[src_i] << bits_shift) | (and_word[src_i-1] >> inv_shift)
            let mut cur_and = top_and;
            for src_i in (src_lo..=src_hi).rev() {
                let dst_i = src_i + chunks_shift;
                if dst_i >= len {
                    // Recompute cur_and for the next iteration's prev
                    if src_i > src_lo {
                        cur_and = unsafe { *aptr.add(src_i - 1) & *bptr.add(src_i - 1) };
                    }
                    continue;
                }
                let prev_and = if src_i > src_lo {
                    unsafe { *aptr.add(src_i - 1) & *bptr.add(src_i - 1) }
                } else {
                    0
                };
                unsafe {
                    *dptr.add(dst_i) = (cur_and << bits_shift) | (prev_and >> inv_shift);
                }
                cur_and = prev_and;
            }
        }

        self.lo = dst_lo as u16;
        self.hi = dst_hi as u16;
    }

    /// Fused `self = (a & b) >> shift`. Computes AND and right-shift in one pass.
    #[inline]
    pub fn copy_and_shr(&mut self, a: &Self, b: &Self, shift: usize) {
        if a.lo > a.hi || b.lo > b.hi {
            self.clear_active_range();
            return;
        }
        let src_lo = std::cmp::max(a.lo as usize, b.lo as usize);
        let src_hi = std::cmp::min(a.hi as usize, b.hi as usize);
        if src_lo > src_hi {
            self.clear_active_range();
            return;
        }

        let chunks_shift = shift / 64;
        let bits_shift = shift % 64;

        if src_lo < chunks_shift && src_hi < chunks_shift {
            self.clear_active_range();
            return;
        }
        let dst_hi = src_hi.saturating_sub(chunks_shift);
        let mut dst_lo = src_lo.saturating_sub(chunks_shift);

        let dptr = self.data.as_mut_ptr();
        let aptr = a.data.as_ptr();
        let bptr = b.data.as_ptr();

        // Clear old active range outside new destination
        let clear_hi = if bits_shift > 0 && dst_lo > 0 {
            dst_lo - 1
        } else {
            dst_lo
        };
        self.clear_outside_range(clear_hi, dst_hi);

        if bits_shift == 0 {
            for src_i in src_lo..=src_hi {
                if src_i < chunks_shift {
                    continue;
                }
                let dst_i = src_i - chunks_shift;
                unsafe {
                    *dptr.add(dst_i) = *aptr.add(src_i) & *bptr.add(src_i);
                }
            }
        } else {
            let inv_shift = 64 - bits_shift;
            // Check if carry from bottom word extends dst_lo downward
            let effective_src_lo = if src_lo >= chunks_shift {
                src_lo
            } else {
                chunks_shift
            };
            let bot_and = unsafe { *aptr.add(effective_src_lo) & *bptr.add(effective_src_lo) };
            let bot_carry = bot_and << inv_shift;
            if bot_carry != 0
                && effective_src_lo >= chunks_shift
                && (effective_src_lo - chunks_shift) > 0
            {
                dst_lo = effective_src_lo - chunks_shift - 1;
                unsafe {
                    *dptr.add(dst_lo) = bot_carry;
                }
            }
            // Process words bottom-up, caching AND to avoid recomputation.
            let start = if src_lo >= chunks_shift {
                src_lo
            } else {
                chunks_shift
            };
            let mut cur_and = unsafe { *aptr.add(start) & *bptr.add(start) };
            for src_i in start..=src_hi {
                let dst_i = src_i - chunks_shift;
                let next_and = if src_i < src_hi {
                    unsafe { *aptr.add(src_i + 1) & *bptr.add(src_i + 1) }
                } else {
                    0
                };
                unsafe {
                    *dptr.add(dst_i) = (cur_and >> bits_shift) | (next_and << inv_shift);
                }
                cur_and = next_and;
            }
        }

        self.lo = dst_lo as u16;
        self.hi = dst_hi as u16;
    }

    /// Helper: clear current active range to zero, set lo>hi.
    #[inline]
    fn clear_active_range(&mut self) {
        if self.lo <= self.hi {
            let slo = self.lo as usize;
            let shi = self.hi as usize;
            unsafe {
                simd_zero(self.data.as_mut_ptr().add(slo), shi - slo + 1);
            }
        }
        self.lo = 1;
        self.hi = 0;
    }

    /// Helper: zero words in the old active range that fall outside [new_lo, new_hi].
    #[inline]
    fn clear_outside_range(&mut self, new_lo: usize, new_hi: usize) {
        if self.lo <= self.hi {
            let slo = self.lo as usize;
            let shi = self.hi as usize;
            let dptr = self.data.as_mut_ptr();
            // Zero below new_lo
            let below = new_lo.min(shi + 1).saturating_sub(slo);
            if below > 0 {
                unsafe {
                    simd_zero(dptr.add(slo), below);
                }
            }
            // Zero above new_hi
            let above_start = (new_hi + 1).max(slo);
            if above_start <= shi {
                unsafe {
                    simd_zero(dptr.add(above_start), shi - above_start + 1);
                }
            }
        }
    }

    /// Fused `self |= (a & b)` — avoids materializing the AND result.
    #[inline]
    pub fn or_and_assign(&mut self, a: &Self, b: &Self) {
        if a.lo > a.hi || b.lo > b.hi {
            return; // a & b is zero, self unchanged
        }
        let lo = std::cmp::max(a.lo as usize, b.lo as usize);
        let hi = std::cmp::min(a.hi as usize, b.hi as usize);
        if lo > hi {
            return;
        }

        let dptr = self.data.as_mut_ptr();
        let aptr = a.data.as_ptr();
        let bptr = b.data.as_ptr();
        let count = hi - lo + 1;

        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        unsafe {
            let mut i = 0;
            while i + 4 <= count {
                let off = lo + i;
                let va = _mm256_loadu_si256((aptr as *const u8).add(off * 8) as *const __m256i);
                let vb = _mm256_loadu_si256((bptr as *const u8).add(off * 8) as *const __m256i);
                let vd = _mm256_loadu_si256((dptr as *const u8).add(off * 8) as *const __m256i);
                _mm256_storeu_si256(
                    (dptr as *mut u8).add(off * 8) as *mut __m256i,
                    _mm256_or_si256(vd, _mm256_and_si256(va, vb)),
                );
                i += 4;
            }
            while i < count {
                let off = lo + i;
                *dptr.add(off) |= *aptr.add(off) & *bptr.add(off);
                i += 1;
            }
        }
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        unsafe {
            for i in lo..=hi {
                *dptr.add(i) |= *aptr.add(i) & *bptr.add(i);
            }
        }

        // Expand range
        if self.lo > self.hi {
            self.lo = lo as u16;
            self.hi = hi as u16;
        } else {
            if (lo as u16) < self.lo {
                self.lo = lo as u16;
            }
            if (hi as u16) > self.hi {
                self.hi = hi as u16;
            }
        }
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
        let len = total_cells.div_ceil(64);
        BitBoardLarge {
            data: smallvec![0u64; len],
            lo: 1,
            hi: 0,
        }
    }

    fn set_bit(&mut self, index: usize) {
        let vec_idx = index / 64;
        if vec_idx < self.data.len() {
            self.data[vec_idx] |= 1 << (index % 64);
            // Expand range
            let vi = vec_idx as u16;
            if self.lo > self.hi {
                self.lo = vi;
                self.hi = vi;
            } else {
                if vi < self.lo {
                    self.lo = vi;
                }
                if vi > self.hi {
                    self.hi = vi;
                }
            }
        }
    }

    fn clear_bit(&mut self, index: usize) {
        let vec_idx = index / 64;
        if vec_idx < self.data.len() {
            self.data[vec_idx] &= !(1 << (index % 64));
            // Don't shrink range eagerly — would require checking if word became zero
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
        if self.lo > self.hi {
            return 0;
        }
        let lo = self.lo as usize;
        let hi = self.hi as usize;
        let mut count = 0u32;
        for i in lo..=hi {
            count += self.data[i].count_ones();
        }
        count
    }

    fn iter_indices(&self) -> Self::Iter<'_> {
        if self.lo > self.hi || self.data.is_empty() {
            return BitIteratorLarge {
                board: self,
                current_chunk_idx: self.data.len(),
                current_chunk: 0,
            };
        }
        BitIteratorLarge {
            board: self,
            current_chunk_idx: self.lo as usize,
            current_chunk: self.data[self.lo as usize],
        }
    }

    fn copy_from(&mut self, other: &Self) {
        if self.data.len() != other.data.len() {
            self.data.resize(other.data.len(), 0);
        }
        if other.lo > other.hi {
            // Source is all-zero — clear our active range
            if self.lo <= self.hi {
                let lo = self.lo as usize;
                let hi = self.hi as usize;
                unsafe {
                    simd_zero(self.data.as_mut_ptr().add(lo), hi - lo + 1);
                }
            }
        } else {
            let olo = other.lo as usize;
            let ohi = other.hi as usize;
            // Clear words in self's range outside other's range
            if self.lo <= self.hi {
                let slo = self.lo as usize;
                let shi = self.hi as usize;
                let clear_below = std::cmp::min(olo, shi + 1).saturating_sub(slo);
                if clear_below > 0 {
                    unsafe {
                        simd_zero(self.data.as_mut_ptr().add(slo), clear_below);
                    }
                }
                let above_start = (ohi + 1).max(slo);
                if above_start <= shi {
                    unsafe {
                        simd_zero(
                            self.data.as_mut_ptr().add(above_start),
                            shi - above_start + 1,
                        );
                    }
                }
            }
            // Copy active range
            self.data[olo..=ohi].copy_from_slice(&other.data[olo..=ohi]);
        }
        self.lo = other.lo;
        self.hi = other.hi;
    }

    fn zero_like(&self) -> Self {
        BitBoardLarge {
            data: smallvec![0u64; self.data.len()],
            lo: 1,
            hi: 0,
        }
    }

    fn ensure_capacity_and_clear(&mut self, template: &Self) {
        if self.data.len() != template.data.len() {
            self.data.resize(template.data.len(), 0);
            let len = self.data.len();
            unsafe {
                simd_zero(self.data.as_mut_ptr(), len);
            }
        } else if self.lo <= self.hi {
            let lo = self.lo as usize;
            let hi = self.hi as usize;
            unsafe {
                simd_zero(self.data.as_mut_ptr().add(lo), hi - lo + 1);
            }
        }
        self.lo = 1;
        self.hi = 0;
    }

    #[inline]
    fn intersects_any(&self, other: &Self) -> bool {
        if self.lo > self.hi || other.lo > other.hi {
            return false;
        }
        let lo = (self.lo as usize).max(other.lo as usize);
        let hi = (self.hi as usize).min(other.hi as usize);
        if lo > hi {
            return false;
        }
        for i in lo..=hi {
            if self.data[i] & other.data[i] != 0 {
                return true;
            }
        }
        false
    }
}

impl BitAndAssign<&BitBoardLarge> for BitBoardLarge {
    #[inline]
    fn bitand_assign(&mut self, rhs: &BitBoardLarge) {
        if self.lo > self.hi {
            return; // already all-zero
        }
        let lo = self.lo as usize;
        let hi = self.hi as usize;
        let rhs_empty = rhs.lo > rhs.hi;
        let rhs_lo = rhs.lo as usize;
        let rhs_hi = rhs.hi as usize;
        let rhs_len = rhs.data.len();
        let ptr = self.data.as_mut_ptr();

        if rhs_empty {
            unsafe {
                simd_zero(ptr.add(lo), hi - lo + 1);
            }
            self.lo = 1;
            self.hi = 0;
            return;
        }

        // Compute intersection of active ranges
        let int_lo = std::cmp::max(lo, rhs_lo);
        let int_hi = std::cmp::min(hi, std::cmp::min(rhs_hi, rhs_len - 1));

        // Zero words in self's range that are below the intersection
        let zero_below = std::cmp::min(int_lo, hi + 1).saturating_sub(lo);
        if zero_below > 0 {
            unsafe {
                simd_zero(ptr.add(lo), zero_below);
            }
        }
        // AND within the intersection
        if int_lo <= int_hi {
            let rptr = rhs.data.as_ptr();
            unsafe {
                simd_and_assign(ptr.add(int_lo), rptr.add(int_lo), int_hi - int_lo + 1);
            }
        }
        // Zero words in self's range that are above the intersection
        let above_start = (int_hi + 1).max(lo);
        if above_start <= hi {
            unsafe {
                simd_zero(ptr.add(above_start), hi - above_start + 1);
            }
        }
        // Zero words beyond rhs length
        if hi >= rhs_len {
            unsafe {
                simd_zero(ptr.add(rhs_len), hi - rhs_len + 1);
            }
        }

        // Conservative range: intersection (may contain zeros, that's OK)
        if int_lo > int_hi {
            self.lo = 1;
            self.hi = 0;
        } else {
            self.lo = int_lo as u16;
            self.hi = int_hi as u16;
        }
    }
}

impl BitOrAssign<&BitBoardLarge> for BitBoardLarge {
    #[inline]
    fn bitor_assign(&mut self, rhs: &BitBoardLarge) {
        if rhs.lo > rhs.hi {
            return; // OR with zero is identity
        }
        let rhs_lo = rhs.lo as usize;
        let rhs_hi = rhs.hi as usize;

        // Only need to OR within rhs's active range
        if rhs_hi < self.data.len() {
            let ptr = self.data.as_mut_ptr();
            let rptr = rhs.data.as_ptr();
            unsafe {
                simd_or_assign(ptr.add(rhs_lo), rptr.add(rhs_lo), rhs_hi - rhs_lo + 1);
            }
        } else {
            // rhs extends beyond self — OR what fits, extend the rest
            let self_len = self.data.len();
            let overlap_hi = std::cmp::min(rhs_hi, self_len - 1);
            let count = overlap_hi - rhs_lo + 1;
            let ptr = self.data.as_mut_ptr();
            let rptr = rhs.data.as_ptr();
            unsafe {
                simd_or_assign(ptr.add(rhs_lo), rptr.add(rhs_lo), count);
            }
            if rhs_hi >= self_len {
                self.data.extend_from_slice(&rhs.data[self_len..=rhs_hi]);
            }
        }

        // Expand range
        if self.lo > self.hi {
            self.lo = rhs.lo;
            self.hi = rhs.hi;
        } else {
            if rhs.lo < self.lo {
                self.lo = rhs.lo;
            }
            if rhs.hi > self.hi {
                self.hi = rhs.hi;
            }
        }
    }
}

impl ShlAssign<usize> for BitBoardLarge {
    #[inline]
    fn shl_assign(&mut self, shift: usize) {
        if shift == 0 || self.lo > self.hi {
            return;
        }
        let len = self.data.len();
        let chunks_shift = shift / 64;
        let bits_shift = shift % 64;
        let mut lo = self.lo as usize;
        let mut hi = self.hi as usize;
        let ptr = self.data.as_mut_ptr();

        if chunks_shift > 0 {
            if lo + chunks_shift >= len {
                // All active words shift out of bounds
                for i in lo..=hi {
                    unsafe {
                        *ptr.add(i) = 0;
                    }
                }
                self.lo = 1;
                self.hi = 0;
                return;
            }
            let new_hi = (hi + chunks_shift).min(len - 1);
            let new_lo = lo + chunks_shift;
            for i in (new_lo..=new_hi).rev() {
                unsafe {
                    *ptr.add(i) = *ptr.add(i - chunks_shift);
                }
            }
            for i in lo..new_lo {
                unsafe {
                    *ptr.add(i) = 0;
                }
            }
            lo = new_lo;
            hi = new_hi;
        }

        if bits_shift > 0 {
            let inv_shift = 64 - bits_shift;
            let orig_hi = hi;
            // Check if carry expands upward (read from unmodified data[hi])
            if hi + 1 < len {
                let carry = unsafe { *ptr.add(hi) >> inv_shift };
                if carry != 0 {
                    unsafe {
                        *ptr.add(hi + 1) = carry;
                    }
                    hi += 1;
                }
            }
            // Process orig_hi down to lo+1 backward (carry word at hi is above this range)
            for i in (lo + 1..=orig_hi).rev() {
                unsafe {
                    let prev = *ptr.add(i - 1);
                    *ptr.add(i) = (*ptr.add(i) << bits_shift) | (prev >> inv_shift);
                }
            }
            // Handle the lowest word
            unsafe {
                *ptr.add(lo) = *ptr.add(lo) << bits_shift;
            }
        }

        self.lo = lo as u16;
        self.hi = hi as u16;
    }
}

impl ShrAssign<usize> for BitBoardLarge {
    #[inline]
    fn shr_assign(&mut self, shift: usize) {
        if shift == 0 || self.lo > self.hi {
            return;
        }
        let chunks_shift = shift / 64;
        let bits_shift = shift % 64;
        let mut lo = self.lo as usize;
        let mut hi = self.hi as usize;
        let ptr = self.data.as_mut_ptr();

        if chunks_shift > 0 {
            if hi < chunks_shift {
                // All active words shift out of bounds
                for i in lo..=hi {
                    unsafe {
                        *ptr.add(i) = 0;
                    }
                }
                self.lo = 1;
                self.hi = 0;
                return;
            }
            let new_lo = lo.saturating_sub(chunks_shift);
            let new_hi = hi - chunks_shift;
            for i in new_lo..=new_hi {
                unsafe {
                    *ptr.add(i) = *ptr.add(i + chunks_shift);
                }
            }
            // Zero out the old positions that are now stale
            let clear_start = (new_hi + 1).max(lo);
            for i in clear_start..=hi {
                unsafe {
                    *ptr.add(i) = 0;
                }
            }
            lo = new_lo;
            hi = new_hi;
        }

        if bits_shift > 0 {
            let inv_shift = 64 - bits_shift;
            let orig_lo = lo;
            // Check if carry expands downward (read from unmodified data[lo])
            if lo > 0 {
                let carry = unsafe { (*ptr.add(lo)) << inv_shift };
                if carry != 0 {
                    unsafe {
                        *ptr.add(lo - 1) = carry;
                    }
                    lo -= 1;
                }
            }
            // Process orig_lo..=hi forward (carry word at lo-1 is below this range)
            for i in orig_lo..hi {
                unsafe {
                    let next = *ptr.add(i + 1);
                    *ptr.add(i) = (*ptr.add(i) >> bits_shift) | (next << inv_shift);
                }
            }
            unsafe {
                *ptr.add(hi) >>= bits_shift;
            }
        }

        self.lo = lo as u16;
        self.hi = hi as u16;
    }
}

impl<'b> BitAnd<&'b BitBoardLarge> for &BitBoardLarge {
    type Output = BitBoardLarge;

    fn bitand(self, rhs: &'b BitBoardLarge) -> BitBoardLarge {
        let len = std::cmp::max(self.data.len(), rhs.data.len());
        let mut new_data = SmallVec::with_capacity(len);
        for i in 0..len {
            let val_a = self.data.get(i).copied().unwrap_or(0);
            let val_b = rhs.data.get(i).copied().unwrap_or(0);
            new_data.push(val_a & val_b);
        }
        let mut result = BitBoardLarge {
            data: new_data,
            lo: 1,
            hi: 0,
        };
        result.recompute_range();
        result
    }
}

impl<'b> BitOr<&'b BitBoardLarge> for &BitBoardLarge {
    type Output = BitBoardLarge;

    fn bitor(self, rhs: &'b BitBoardLarge) -> BitBoardLarge {
        let len = std::cmp::max(self.data.len(), rhs.data.len());
        let mut new_data = SmallVec::with_capacity(len);
        for i in 0..len {
            let val_a = self.data.get(i).copied().unwrap_or(0);
            let val_b = rhs.data.get(i).copied().unwrap_or(0);
            new_data.push(val_a | val_b);
        }
        let mut result = BitBoardLarge {
            data: new_data,
            lo: 1,
            hi: 0,
        };
        result.recompute_range();
        result
    }
}

impl Not for &BitBoardLarge {
    type Output = BitBoardLarge;

    fn not(self) -> BitBoardLarge {
        let mut new_data = SmallVec::with_capacity(self.data.len());
        for x in &self.data {
            new_data.push(!x);
        }
        // NOT of anything is non-zero everywhere
        let mut result = BitBoardLarge {
            data: new_data,
            lo: 1,
            hi: 0,
        };
        if !result.data.is_empty() {
            result.lo = 0;
            result.hi = (result.data.len() - 1) as u16;
        }
        result
    }
}

impl Shl<usize> for &BitBoardLarge {
    type Output = BitBoardLarge;
    fn shl(self, shift: usize) -> BitBoardLarge {
        let mut res = self.clone();
        res <<= shift;
        res
    }
}

impl Shr<usize> for &BitBoardLarge {
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
