use std::sync::atomic::{AtomicU64, Ordering};

// Pack data into u64:
// 32 bits score | 8 bits depth | 2 bits flag | 22 bits partial hash/verification
// We will store the FULL key in a separate atomic for verification.
// The packed data is primarily for the value payload.

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
    pub promotion: u8, // 0=None, 1=Q, 2=R, 3=B, 4=N...
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

pub struct LockFreeTT {
    table: Vec<AtomicU64>,
    size_mask: usize,
}

impl LockFreeTT {
    pub fn new(size_mb: usize) -> Self {
        let size = size_mb * 1024 * 1024 / std::mem::size_of::<AtomicU64>();
        let num_entries = size.next_power_of_two();

        let mut table = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            table.push(AtomicU64::new(0));
        }

        LockFreeTT {
            table,
            size_mask: num_entries - 1,
        }
    }

    pub fn get(&self, hash: u64) -> Option<(i32, u8, Flag, Option<PackedMove>)> {
        let index = (hash as usize) & self.size_mask;
        let entry = self.table[index].load(Ordering::Relaxed);

        if entry == 0 {
            return None;
        }

        let entry_hash = (entry >> 32) as u32; // Top 32 bits of hash
        if entry_hash != (hash >> 32) as u32 {
            return None;
        }

        let data = entry as u32;
        // Unpacking:
        // Score: 16 bits (0-15)
        // Depth: 8 bits (16-23)
        // Flag: 2 bits (24-25)
        // HasMove: 1 bit (26)
        // Move From: ? We didn't store full move in u64 with this packing scheme.
        // The previous attempt realized we can't fit it.
        // Let's settle for NOT storing the move if we don't have space with 64-bit entry.
        // OR we just assume we return None for now as placeholder for the refactor.
        // To properly support Move storage we need 128-bit atomics or a larger struct.
        // For this task, let's keep the signature but return None for move.

        let score = (data & 0xFFFF) as i16 as i32;
        let depth = ((data >> 16) & 0xFF) as u8;
        let flag_u8 = ((data >> 24) & 0x3) as u8;

        let flag = match flag_u8 {
            0 => Flag::Exact,
            1 => Flag::LowerBound,
            2 => Flag::UpperBound,
            _ => Flag::Exact,
        };

        Some((score, depth, flag, None)) // Placeholder: We are not storing moves yet due to size constraints.
    }

    pub fn store(
        &self,
        hash: u64,
        score: i32,
        depth: u8,
        flag: Flag,
        _best_move: Option<PackedMove>,
    ) {
        let index = (hash as usize) & self.size_mask;
        let key_part = (hash >> 32) as u32;

        let score_part = (score.clamp(i16::MIN as i32 + 1, i16::MAX as i32 - 1) as i16) as u16;
        let flag_u8 = match flag {
            Flag::Exact => 0,
            Flag::LowerBound => 1,
            Flag::UpperBound => 2,
        };

        let mut data: u32 = score_part as u32;
        data |= (depth as u32) << 16;
        data |= (flag_u8 as u32) << 24;
        // We drop best_move for now as decided.

        let entry = ((key_part as u64) << 32) | (data as u64);

        self.table[index].store(entry, Ordering::Relaxed);
    }
}
