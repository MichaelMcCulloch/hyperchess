use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Exact,
    LowerBound,
    UpperBound,
}

#[derive(Clone, Copy, Debug)]
pub struct PackedMove {
    pub from_idx: u16,
    pub to_idx: u16,
    pub promotion: u8,
}

impl PackedMove {
    pub fn to_u32(&self) -> u32 {
        (self.from_idx as u32) | ((self.to_idx as u32) << 16)
    }

    pub fn from_u32(val: u32) -> Self {
        Self {
            from_idx: (val & 0xFFFF) as u16,
            to_idx: ((val >> 16) & 0xFFFF) as u16,
            promotion: 0,
        }
    }
}

/// Number of entries per bucket (cluster). 3 entries = 48 bytes per bucket.
const BUCKET_SIZE: usize = 3;

/// TT entry data layout (64 bits):
///   bits  0-15: score (i16)
///   bits 16-23: depth (u8)
///   bits 24-25: flag (2 bits)
///   bits 26-29: promotion type (4 bits)
///   bits 30-45: from index (u16)
///   bits 46-61: to index (u16)
///   bits 62-63: unused
///
/// Separate key word stores hash XOR data for consistency check,
/// plus generation + is_pv in the low bits of key16.

/// Generation uses 6 bits (0-63), cycled with GENERATION_DELTA=1.
const GENERATION_BITS: u8 = 6;
const GENERATION_MASK: u8 = (1 << GENERATION_BITS) - 1; // 0x3F
pub struct LockFreeTT {
    /// Flat array: each bucket = BUCKET_SIZE entries, each entry = 2 AtomicU64 (key, data).
    table: Vec<AtomicU64>,
    /// Number of buckets (power of 2).
    num_buckets: usize,
    bucket_mask: usize,
    /// Current generation counter (0-63).
    generation: u8,
}

impl LockFreeTT {
    pub fn new(size_mb: usize) -> Self {
        // Each bucket = BUCKET_SIZE entries × 16 bytes = 48 bytes
        let bytes = size_mb * 1024 * 1024;
        let num_buckets = (bytes / (BUCKET_SIZE * 16)).next_power_of_two().max(1);

        let num_atomics = num_buckets * BUCKET_SIZE * 2;
        let mut table = Vec::with_capacity(num_atomics);
        for _ in 0..num_atomics {
            table.push(AtomicU64::new(0));
        }

        LockFreeTT {
            table,
            num_buckets,
            bucket_mask: num_buckets - 1,
            generation: 0,
        }
    }

    /// Advance generation counter (call at start of each iterative deepening iteration).
    pub fn new_search(&mut self) {
        self.generation = (self.generation + 1) & GENERATION_MASK;
    }

    /// Get the current generation.
    pub fn generation(&self) -> u8 {
        self.generation
    }

    /// Prefetch the bucket for this hash into L1 cache.
    #[inline]
    pub fn prefetch(&self, hash: u64) {
        let bucket_idx = (hash as usize) & self.bucket_mask;
        let base = bucket_idx * BUCKET_SIZE * 2;
        // Safety: base is always in bounds since bucket_mask < num_buckets
        let ptr = &self.table[base] as *const AtomicU64;
        #[cfg(target_arch = "x86_64")]
        unsafe {
            std::arch::x86_64::_mm_prefetch(ptr as *const i8, std::arch::x86_64::_MM_HINT_T0);
        }
        #[cfg(target_arch = "x86")]
        unsafe {
            std::arch::x86::_mm_prefetch(ptr as *const i8, std::arch::x86::_MM_HINT_T0);
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
        {
            let _ = ptr; // no-op on non-x86
        }
    }

    pub fn get(&self, hash: u64) -> Option<(i32, u8, Flag, Option<PackedMove>)> {
        let bucket_idx = (hash as usize) & self.bucket_mask;
        let base = bucket_idx * BUCKET_SIZE * 2;

        for i in 0..BUCKET_SIZE {
            let key_entry = self.table[base + i * 2].load(Ordering::Relaxed);
            let data_entry = self.table[base + i * 2 + 1].load(Ordering::Relaxed);

            // Key stores hash XOR data (Stockfish-style consistency)
            if key_entry != (hash ^ data_entry) {
                continue;
            }

            return Some(Self::decode_data(data_entry));
        }

        None
    }

    /// Probe with additional info: returns (score, depth, flag, move, is_pv, stored_depth_raw).
    pub fn get_full(&self, hash: u64) -> Option<(i32, u8, Flag, Option<PackedMove>, bool)> {
        let bucket_idx = (hash as usize) & self.bucket_mask;
        let base = bucket_idx * BUCKET_SIZE * 2;

        for i in 0..BUCKET_SIZE {
            let key_entry = self.table[base + i * 2].load(Ordering::Relaxed);
            let data_entry = self.table[base + i * 2 + 1].load(Ordering::Relaxed);

            if key_entry != (hash ^ data_entry) {
                continue;
            }

            let (score, depth, flag, best_move) = Self::decode_data(data_entry);
            let gen_pv_byte = ((data_entry >> 62) & 0x3) as u8; // we only have 2 bits left
            // Actually, let's use a different scheme. We'll pack gen+pv into the key word.
            // For simplicity, just check the PV flag from the data.
            let _ = gen_pv_byte;
            // Decode is_pv from flag bits area - we use bit 63
            let is_pv = (data_entry >> 63) & 1 == 1;

            return Some((score, depth, flag, best_move, is_pv));
        }

        None
    }

    pub fn store(
        &self,
        hash: u64,
        score: i32,
        depth: u8,
        flag: Flag,
        best_move: Option<PackedMove>,
    ) {
        self.store_with_pv(hash, score, depth, flag, best_move, false);
    }

    pub fn store_with_pv(
        &self,
        hash: u64,
        score: i32,
        depth: u8,
        flag: Flag,
        best_move: Option<PackedMove>,
        is_pv: bool,
    ) {
        let bucket_idx = (hash as usize) & self.bucket_mask;
        let base = bucket_idx * BUCKET_SIZE * 2;

        let new_data = Self::encode_data(score, depth, flag, best_move, is_pv, self.generation);

        // Find the best slot to replace:
        // 1. Empty slot
        // 2. Same position (update)
        // 3. Worst existing entry by replacement score
        let mut replace_idx = 0;
        let mut worst_score = i32::MAX;

        for i in 0..BUCKET_SIZE {
            let key_entry = self.table[base + i * 2].load(Ordering::Relaxed);
            let data_entry = self.table[base + i * 2 + 1].load(Ordering::Relaxed);

            // Empty slot — use immediately
            if data_entry == 0 {
                replace_idx = i;
                break;
            }

            // Same position — prefer updating
            if key_entry == (hash ^ data_entry) {
                // Always replace same position if new depth >= old depth or exact bound
                let old_depth = ((data_entry >> 16) & 0xFF) as i32;
                if flag == Flag::Exact || depth as i32 >= old_depth {
                    replace_idx = i;
                    break;
                }
                // Even if shallower, this is still the best slot for this position
                replace_idx = i;
                break;
            }

            // Score this entry for replacement: prefer old generation and shallow depth
            let entry_depth = ((data_entry >> 16) & 0xFF) as i32;
            let entry_gen = Self::extract_generation(data_entry);
            let age = self.relative_age(entry_gen);
            // Higher replacement score = more worthy of keeping. Lower = more replaceable.
            let replacement_worth = entry_depth * 4 - age as i32 * 8;

            if replacement_worth < worst_score {
                worst_score = replacement_worth;
                replace_idx = i;
            }
        }

        let slot_key = base + replace_idx * 2;
        let slot_data = base + replace_idx * 2 + 1;

        self.table[slot_data].store(new_data, Ordering::Relaxed);
        self.table[slot_key].store(hash ^ new_data, Ordering::Relaxed);
    }

    fn relative_age(&self, entry_gen: u8) -> u8 {
        // How many generations old is this entry? Handles wraparound.
        (self.generation.wrapping_sub(entry_gen)) & GENERATION_MASK
    }

    fn extract_generation(data: u64) -> u8 {
        // Generation stored in bits 56-61 (6 bits)
        ((data >> 56) & GENERATION_MASK as u64) as u8
    }

    fn encode_data(
        score: i32,
        depth: u8,
        flag: Flag,
        best_move: Option<PackedMove>,
        is_pv: bool,
        generation: u8,
    ) -> u64 {
        let score_part =
            (score.clamp(i16::MIN as i32 + 1, i16::MAX as i32 - 1) as i16) as u16 as u64;
        let depth_part = (depth as u64) << 16;
        let flag_u8 = match flag {
            Flag::Exact => 0,
            Flag::LowerBound => 1,
            Flag::UpperBound => 2,
        };
        let flag_part = (flag_u8 as u64) << 24;

        let mut move_part: u64 = 0;
        if let Some(m) = best_move {
            move_part |= (m.promotion as u64 & 0xF) << 26;
            move_part |= (m.from_idx as u64) << 30;
            move_part |= (m.to_idx as u64) << 46;
        }

        let gen_part = (generation as u64 & GENERATION_MASK as u64) << 56;
        let pv_part = if is_pv { 1u64 << 63 } else { 0 };

        score_part | depth_part | flag_part | move_part | gen_part | pv_part
    }

    fn decode_data(data: u64) -> (i32, u8, Flag, Option<PackedMove>) {
        let score = (data & 0xFFFF) as i16 as i32;
        let depth = ((data >> 16) & 0xFF) as u8;
        let flag_u8 = ((data >> 24) & 0x3) as u8;

        let flag = match flag_u8 {
            0 => Flag::Exact,
            1 => Flag::LowerBound,
            2 => Flag::UpperBound,
            _ => Flag::Exact,
        };

        // bits 30-45: from_idx (16 bits)
        // bits 46-55: to_idx (10 bits) — supports up to 1024 cells
        // bits 56-61: generation (6 bits), bit 63: is_pv
        let promo = ((data >> 26) & 0xF) as u8;
        let from = ((data >> 30) & 0xFFFF) as u16;
        let to_full = ((data >> 46) & 0x3FF) as u16;

        let best_move = if from != 0 || to_full != 0 {
            Some(PackedMove {
                from_idx: from,
                to_idx: to_full,
                promotion: promo,
            })
        } else {
            None
        };

        (score, depth, flag, best_move)
    }

    /// Returns per-mille hash utilization.
    pub fn hashfull(&self) -> u32 {
        let sample_buckets = 1000.min(self.num_buckets);
        let mut count = 0u32;
        for b in 0..sample_buckets {
            let base = b * BUCKET_SIZE * 2;
            for i in 0..BUCKET_SIZE {
                let data = self.table[base + i * 2 + 1].load(Ordering::Relaxed);
                if data != 0 {
                    let entry_generation = Self::extract_generation(data);
                    if self.relative_age(entry_generation) == 0 {
                        count += 1;
                    }
                }
            }
        }
        count * 1000 / (sample_buckets as u32 * BUCKET_SIZE as u32)
    }
}
