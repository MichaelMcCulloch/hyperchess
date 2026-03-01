/// Mutable game-rule state that changes with each move.
/// Separated from PieceMap so functions that only need piece layout
/// (e.g., evaluation, mobility) don't pay for this data.
#[derive(Clone, Debug)]
pub struct PositionState {
    pub hash: u64,
    pub history: Vec<u64>,
    pub en_passant_target: Option<(usize, usize)>,
    pub castling_rights: u8,
    pub halfmove_clock: u16,
    pub fullmove_number: u16,
    /// Total game-phase value at position creation (for tapered eval).
    /// Computed from piece counts × phase weights; dimension-agnostic.
    pub start_phase: i32,
}

impl PositionState {
    pub fn new() -> Self {
        Self {
            hash: 0,
            history: Vec::new(),
            en_passant_target: None,
            castling_rights: 0,
            halfmove_clock: 0,
            fullmove_number: 1,
            start_phase: 0,
        }
    }

    pub fn is_repetition(&self) -> bool {
        self.history.iter().filter(|&&h| h == self.hash).count() >= 1
    }
}

impl Default for PositionState {
    fn default() -> Self {
        Self::new()
    }
}
