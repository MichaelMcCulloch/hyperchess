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

pub struct LockFreeTT {
    table: Vec<AtomicU64>,
    size_mask: usize,
}

impl LockFreeTT {
    pub fn new(size_mb: usize) -> Self {
        // Entry size is 16 bytes (2 * u64)
        let size = size_mb * 1024 * 1024 / 16;
        let num_entries = size.next_power_of_two();

        let mut table = Vec::with_capacity(num_entries * 2);
        for _ in 0..(num_entries * 2) {
            table.push(AtomicU64::new(0));
        }

        LockFreeTT {
            table,
            size_mask: num_entries - 1,
        }
    }

    pub fn get(&self, hash: u64) -> Option<(i32, u8, Flag, Option<PackedMove>)> {
        let index = (hash as usize) & self.size_mask;
        // Key is at 2*index, Data is at 2*index + 1
        let key_entry = self.table[index * 2].load(Ordering::Relaxed);
        let data_entry = self.table[index * 2 + 1].load(Ordering::Relaxed);

        if key_entry != hash {
            return None;
        }

        // Decode data
        // Layout:
        // Score: 0-15 (16 bits)
        // Depth: 16-23 (8 bits)
        // Flag:  24-25 (2 bits)
        // Promo: 26-29 (4 bits)
        // From:  30-45 (16 bits)
        // To:    46-61 (16 bits)

        // Cast to i16 then i32 for sign extension
        let score = (data_entry & 0xFFFF) as i16 as i32;
        let depth = ((data_entry >> 16) & 0xFF) as u8;
        let flag_u8 = ((data_entry >> 24) & 0x3) as u8;

        let flag = match flag_u8 {
            0 => Flag::Exact,
            1 => Flag::LowerBound,
            2 => Flag::UpperBound,
            _ => Flag::Exact,
        };

        let promo = ((data_entry >> 26) & 0xF) as u8;
        let from = ((data_entry >> 30) & 0xFFFF) as u16;
        let to = ((data_entry >> 46) & 0xFFFF) as u16;

        let best_move = if from != 0 || to != 0 {
            Some(PackedMove {
                from_idx: from,
                to_idx: to,
                promotion: promo,
            })
        } else {
            None
        };

        Some((score, depth, flag, best_move))
    }

    pub fn store(
        &self,
        hash: u64,
        score: i32,
        depth: u8,
        flag: Flag,
        best_move: Option<PackedMove>,
    ) {
        let index = (hash as usize) & self.size_mask;
        let key_idx = index * 2;
        let data_idx = index * 2 + 1;

        // Optimistic replace: Always replace? Or depth-preferred?
        // Stockfish uses depth-preferred or always-replace for new generation.
        // For simple Lockless, always replace is fine, or check depth.
        // Let's read current to check depth.

        let current_key = self.table[key_idx].load(Ordering::Relaxed);
        if current_key == hash {
            let current_data = self.table[data_idx].load(Ordering::Relaxed);
            let current_depth = ((current_data >> 16) & 0xFF) as u8;
            if current_depth > depth {
                return; // Don't overwrite deeper search results
            }
        }

        // Encode data
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

        let new_data = score_part | depth_part | flag_part | move_part;

        // Store data first then key? No, inconsistent.
        // Ideally we XOR key with data, but here we have separate slots.
        // Just store.
        self.table[data_idx].store(new_data, Ordering::Relaxed);
        self.table[key_idx].store(hash, Ordering::Relaxed);
    }
}
