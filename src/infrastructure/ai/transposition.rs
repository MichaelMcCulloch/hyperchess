use std::sync::atomic::{AtomicU64, Ordering};

// Pack data into u64:
// 32 bits score | 8 bits depth | 2 bits flag | 22 bits partial hash/verification
// We will store the FULL key in a separate atomic for verification.
// The packed data is primarily for the value payload.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Flag {
    Exact = 0,
    LowerBound = 1,
    UpperBound = 2,
}

impl Flag {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Flag::Exact,
            1 => Flag::LowerBound,
            2 => Flag::UpperBound,
            _ => Flag::Exact, // Default/Fallback
        }
    }

    fn to_u8(self) -> u8 {
        self as u8
    }
}

#[repr(align(64))]
pub struct TTEntry {
    /// Stores the packed value: score (32), depth (8), flag (2), extra (22)
    pub data: AtomicU64,
    /// Stores the full 64-bit Zobrist key to resolve collisions
    pub key: AtomicU64,
}

pub struct LockFreeTT {
    table: Vec<TTEntry>,
    size: usize,
}

impl LockFreeTT {
    pub fn new(size_mb: usize) -> Self {
        let entry_size = std::mem::size_of::<TTEntry>(); // Should be 16 bytes
        let num_entries = (size_mb * 1024 * 1024) / entry_size;

        let mut table = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            table.push(TTEntry {
                data: AtomicU64::new(0),
                key: AtomicU64::new(0),
            });
        }

        Self {
            table,
            size: num_entries,
        }
    }

    pub fn get(&self, hash: u64) -> Option<(i32, u8, Flag, Option<u16>)> {
        let index = (hash as usize) % self.size;
        // RELAXED ordering is sufficient because we strictly check the key *after* reading data?
        // Actually, to ensure consistency between key and data, we might need stronger ordering or accept tearing.
        // But the standard "lockless" TT in chess engines often accepts some race conditions.
        // A common pattern is:
        // 1. Read key.
        // 2. If match, read data.
        // 3. Verify key again? Or just XOR check?
        //
        // With struct of atomics:
        // We can't guarantee that `key` and `data` are updated atomically together.
        // But `key` check is the guard.
        // If we read `key` == hash, then we read `data`.
        // If `data` was from a previous entry, `key` would be different (mostly).
        // If `data` is being written while we read, we might get torn data? No, `AtomicU64` load is atomic.
        // We might get data from a NEW entry that overwrote the OLD entry but `key` hasn't been updated yet?
        // Or `key` updated but `data` not?
        //
        // High performance engines often bundle `key ^ data` to detect inconsistency,
        // OR just accept that data races are rare enough or benign.
        //
        // Let's stick to the user's suggestion: "Relaxed load is fine for TT; occasional data races are acceptable".

        let entry = &self.table[index];
        let stored_key = entry.key.load(Ordering::Relaxed);

        if stored_key != hash {
            return None;
        }

        let data = entry.data.load(Ordering::Relaxed);

        // Unpack
        // Low 32 bits: score (i32 cast to u32)
        // Next 8 bits: depth
        // Next 2 bits: flag
        // Next 16 bits: best_move (u16, 0xFFFF = None)
        let score_u32 = (data & 0xFFFFFFFF) as u32;
        let score = score_u32 as i32;
        let depth = ((data >> 32) & 0xFF) as u8;
        let flag_u8 = ((data >> 40) & 0x3) as u8;
        let best_move_raw = ((data >> 42) & 0xFFFF) as u16;

        let best_move = if best_move_raw == 0xFFFF {
            None
        } else {
            Some(best_move_raw)
        };

        Some((score, depth, Flag::from_u8(flag_u8), best_move))
    }

    pub fn store(&self, hash: u64, score: i32, depth: u8, flag: Flag, best_move: Option<u16>) {
        let index = (hash as usize) % self.size;
        let entry = &self.table[index];

        // Packing
        let score_u32 = score as u32;
        let depth_u64 = depth as u64;
        let flag_u64 = flag.to_u8() as u64;
        let best_move_val = best_move.unwrap_or(0xFFFF) as u64;

        let packed =
            (score_u32 as u64) | (depth_u64 << 32) | (flag_u64 << 40) | (best_move_val << 42);

        // Store
        // We overwrite unconditionally or based on depth?
        // Simple replacement strategy: always overwrite.
        // Or "depth-preferred" replacement?
        // For now, simple overwrite as per user snippet.

        entry.key.store(hash, Ordering::Relaxed);
        entry.data.store(packed, Ordering::Relaxed);
    }
}
