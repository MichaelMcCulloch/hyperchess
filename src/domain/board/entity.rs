use smallvec::{SmallVec, smallvec};
use std::fmt;
use std::sync::Arc;

use crate::domain::board::bitboard::BitBoard;
use crate::domain::board::board_representation::BoardRepresentation;
use crate::domain::board::geometry::BoardGeometry;
use crate::domain::board::pieces::PieceMap;
use crate::domain::board::position::PositionState;
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{GameResult, Move, Piece, PieceType, Player};
use crate::domain::zobrist::ZobristKeys;

#[derive(Clone, Debug)]
pub struct UnmakeInfo {
    pub captured: Option<(usize, Piece)>,
    pub en_passant_target: Option<(usize, usize)>,
    pub castling_rights: u8,
}

#[derive(Clone, Debug)]
pub struct GenericBoard<R: BoardRepresentation = BitBoard> {
    pub geo: Arc<BoardGeometry<R>>,
    pub zobrist: Arc<ZobristKeys>,
    pub pieces: PieceMap<R>,
    pub state: PositionState,
}

pub type Board = GenericBoard<crate::domain::board::BitBoardLarge>;

impl<R: BoardRepresentation> GenericBoard<R> {
    pub fn new_empty(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let geo = Arc::new(BoardGeometry::new(dimension, side));
        let zobrist = Arc::new(ZobristKeys::new(total_cells));
        let pieces = PieceMap::new_empty(dimension, side);
        let state = PositionState::new();

        GenericBoard {
            geo,
            zobrist,
            pieces,
            state,
        }
    }

    pub fn new(dimension: usize, side: usize) -> Self {
        let mut board = Self::new_empty(dimension, side);
        board.state.castling_rights = 0xF;
        board.setup_standard_chess();
        board
    }

    // ── Forwarding accessors (backward compatibility) ───────────────

    #[inline]
    pub fn dimension(&self) -> usize {
        self.geo.dimension
    }

    #[inline]
    pub fn side(&self) -> usize {
        self.geo.side
    }

    #[inline]
    pub fn total_cells(&self) -> usize {
        self.geo.total_cells
    }

    #[inline]
    pub fn is_repetition(&self) -> bool {
        self.state.is_repetition()
    }

    // ── Piece field forwarding (backward compat for direct field access) ──

    // These are accessed via `board.pieces.*` now, but we keep convenience
    // accessors for code that uses `board.white_occupancy` etc. through
    // the public `pieces` field.

    // ── Coordinate helpers ──────────────────────────────────────────

    pub fn coords_to_index(&self, coords: &[u8]) -> Option<usize> {
        let mut index = 0;
        let mut multiplier = 1;
        for &c in coords {
            if c as usize >= self.geo.side {
                return None;
            }
            index += (c as usize) * multiplier;
            multiplier *= self.geo.side;
        }
        Some(index)
    }

    pub fn index_to_coords(&self, index: usize) -> SmallVec<[u8; 8]> {
        self.geo.cache.index_to_coords[index].clone()
    }

    // ── Hash helpers ────────────────────────────────────────────────

    fn hash_xor_piece(&mut self, index: usize, piece: Piece) {
        let offset = match (piece.owner, piece.piece_type) {
            (Player::White, PieceType::Pawn) => 0,
            (Player::White, PieceType::Knight) => 1,
            (Player::White, PieceType::Bishop) => 2,
            (Player::White, PieceType::Rook) => 3,
            (Player::White, PieceType::Queen) => 4,
            (Player::White, PieceType::King) => 5,
            (Player::Black, PieceType::Pawn) => 6,
            (Player::Black, PieceType::Knight) => 7,
            (Player::Black, PieceType::Bishop) => 8,
            (Player::Black, PieceType::Rook) => 9,
            (Player::Black, PieceType::Queen) => 10,
            (Player::Black, PieceType::King) => 11,
        };
        self.state.hash ^= self.zobrist.piece_keys[offset * self.geo.total_cells + index];
    }

    // ── Piece queries ───────────────────────────────────────────────

    pub fn get_piece(&self, coord: &Coordinate) -> Option<Piece> {
        let index = self.coords_to_index(&coord.values)?;
        self.pieces.get_piece_at_index(index)
    }

    pub fn get_piece_at_index(&self, index: usize) -> Option<Piece> {
        self.pieces.get_piece_at_index(index)
    }

    // ── Setup ───────────────────────────────────────────────────────

    pub fn setup_standard_chess(&mut self) {
        for file_y in 0..self.geo.side {
            let mut white_coords: SmallVec<[u8; 8]> = smallvec![0; self.geo.dimension];
            white_coords[1] = file_y as u8;

            white_coords[0] = 1;

            for d in 2..self.geo.dimension {
                white_coords[d] = 1;
            }

            if let Some(idx) = self.coords_to_index(&white_coords) {
                self.pieces.place_piece_at_index(
                    idx,
                    Piece {
                        piece_type: PieceType::Pawn,
                        owner: Player::White,
                    },
                );
            }

            white_coords[0] = 0;

            for d in 2..self.geo.dimension {
                white_coords[d] = 0;
            }
            if let Some(idx) = self.coords_to_index(&white_coords) {
                let piece_type = self.determine_backrank_piece(file_y, self.geo.side);
                self.pieces.place_piece_at_index(
                    idx,
                    Piece {
                        piece_type,
                        owner: Player::White,
                    },
                );
            }

            let mut black_coords: SmallVec<[u8; 8]> =
                smallvec![(self.geo.side - 1) as u8; self.geo.dimension];
            black_coords[1] = file_y as u8;

            if self.geo.side > 3 {
                black_coords[0] = (self.geo.side - 2) as u8;

                for d in 2..self.geo.dimension {
                    black_coords[d] = (self.geo.side - 2) as u8;
                }

                if let Some(idx) = self.coords_to_index(&black_coords) {
                    self.pieces.place_piece_at_index(
                        idx,
                        Piece {
                            piece_type: PieceType::Pawn,
                            owner: Player::Black,
                        },
                    );
                }
            }

            black_coords[0] = (self.geo.side - 1) as u8;

            for d in 2..self.geo.dimension {
                black_coords[d] = (self.geo.side - 1) as u8;
            }

            if let Some(idx) = self.coords_to_index(&black_coords) {
                let piece_type = self.determine_backrank_piece(file_y, self.geo.side);
                self.pieces.place_piece_at_index(
                    idx,
                    Piece {
                        piece_type,
                        owner: Player::Black,
                    },
                );
            }
        }
        self.state.hash = self
            .zobrist
            .get_hash(&self.pieces, &self.state, self.geo.total_cells);
        self.state.start_phase = self.compute_phase();
    }

    /// Sum phase weights for all non-pawn, non-king pieces on the board.
    /// Dimension-agnostic: just walks the bitboards.
    pub fn compute_phase(&self) -> i32 {
        let mut phase = 0i32;
        let total = self.geo.total_cells;
        for idx in 0..total {
            let occupied = self.pieces.white_occupancy.get_bit(idx)
                || self.pieces.black_occupancy.get_bit(idx);
            if !occupied {
                continue;
            }
            if self.pieces.knights.get_bit(idx) {
                phase += 1;
            } else if self.pieces.bishops.get_bit(idx) {
                phase += 1;
            } else if self.pieces.rooks.get_bit(idx) {
                phase += 2;
            } else if self.pieces.queens.get_bit(idx) {
                phase += 4;
            }
        }
        phase
    }

    fn determine_backrank_piece(&self, file_idx: usize, total_files: usize) -> PieceType {
        if self.geo.dimension == 2 && self.geo.side == 8 {
            return match file_idx {
                0 | 7 => PieceType::Rook,
                1 | 6 => PieceType::Knight,
                2 | 5 => PieceType::Bishop,
                3 => PieceType::Queen,
                4 => PieceType::King,
                _ => PieceType::Pawn,
            };
        }

        let king_pos = total_files / 2;
        if file_idx == king_pos {
            return PieceType::King;
        }
        if file_idx + 1 == king_pos {
            return PieceType::Queen;
        }

        if file_idx == 0 || file_idx == total_files - 1 {
            return PieceType::Rook;
        }
        if file_idx == 1 || file_idx == total_files - 2 {
            return PieceType::Knight;
        }

        PieceType::Bishop
    }

    pub fn update_hash(&mut self, player_to_move: Player) {
        self.state.hash = self.zobrist.get_hash_with_player(
            &self.pieces,
            &self.state,
            self.geo.total_cells,
            player_to_move,
        );
    }

    // ── Move application ────────────────────────────────────────────

    pub fn apply_move(&mut self, mv: &Move) -> Result<UnmakeInfo, String> {
        let from_idx = self
            .coords_to_index(&mv.from.values)
            .ok_or("Invalid from")?;
        let to_idx = self.coords_to_index(&mv.to.values).ok_or("Invalid to")?;

        let moving_piece = self
            .pieces
            .get_piece_at_index(from_idx)
            .ok_or("No piece at from")?;

        let saved_ep = self.state.en_passant_target;
        let saved_castling = self.state.castling_rights;
        let mut captured = None;

        self.state.history.push(self.state.hash);

        self.state.hash ^= self.zobrist.black_to_move;

        if self.state.castling_rights > 0 {
            self.state.hash ^= self.zobrist.castling_keys[self.state.castling_rights as usize];
        }

        if let Some((ep, _)) = self.state.en_passant_target
            && ep < self.zobrist.en_passant_keys.len()
        {
            self.state.hash ^= self.zobrist.en_passant_keys[ep];
        }

        self.hash_xor_piece(from_idx, moving_piece);

        if let Some(target_p) = self.pieces.get_piece_at_index(to_idx) {
            captured = Some((to_idx, target_p));
            self.hash_xor_piece(to_idx, target_p);
        }

        if moving_piece.piece_type == PieceType::Pawn
            && let Some((target, victim)) = self.state.en_passant_target
            && to_idx == target
        {
            if let Some(victim_p) = self.pieces.get_piece_at_index(victim) {
                captured = Some((victim, victim_p));
                self.hash_xor_piece(victim, victim_p);
            }
            self.pieces.remove_piece_at_index(victim);
        }

        self.state.en_passant_target = None;

        if moving_piece.piece_type == PieceType::Pawn {
            let mut diffs: SmallVec<[usize; 4]> = SmallVec::new();
            for i in 0..self.geo.dimension {
                let d = (mv.from.values[i] as isize - mv.to.values[i] as isize).abs();
                diffs.push(d as usize);
            }

            let double_step_axis = diffs.iter().position(|&d| d == 2);
            let any_other_movement = diffs
                .iter()
                .enumerate()
                .any(|(i, &d)| i != double_step_axis.unwrap_or(999) && d != 0);

            if let Some(axis) = double_step_axis
                && !any_other_movement
            {
                let dir = if mv.to.values[axis] > mv.from.values[axis] {
                    1
                } else {
                    -1
                };
                let mut target_vals = mv.from.values.clone();
                target_vals[axis] = (target_vals[axis] as isize + dir) as u8;
                if let Some(target_idx) = self.coords_to_index(&target_vals) {
                    self.state.en_passant_target = Some((target_idx, to_idx));
                    if target_idx < self.zobrist.en_passant_keys.len() {
                        self.state.hash ^= self.zobrist.en_passant_keys[target_idx];
                    }
                }
            }
        }

        let mut castling_rook_move: Option<(usize, usize, Piece)> = None;

        if moving_piece.piece_type == PieceType::King {
            match moving_piece.owner {
                Player::White => self.state.castling_rights &= !0x3,
                Player::Black => self.state.castling_rights &= !0xC,
            }
        }

        if self.geo.side == 8 {
            let w_rank = 0;
            let b_rank = 7;
            let mut w_qs_c: SmallVec<[u8; 8]> = smallvec![w_rank; self.geo.dimension];
            w_qs_c[1] = 0;
            let mut w_ks_c: SmallVec<[u8; 8]> = smallvec![w_rank; self.geo.dimension];
            w_ks_c[1] = 7;

            let mut b_qs_c: SmallVec<[u8; 8]> = smallvec![b_rank; self.geo.dimension];
            b_qs_c[1] = 0;
            let mut b_ks_c: SmallVec<[u8; 8]> = smallvec![b_rank; self.geo.dimension];
            b_ks_c[1] = 7;

            let w_qs = self.coords_to_index(&w_qs_c);
            let w_ks = self.coords_to_index(&w_ks_c);
            let b_qs = self.coords_to_index(&b_qs_c);
            let b_ks = self.coords_to_index(&b_ks_c);

            for idx in [from_idx, to_idx] {
                if Some(idx) == w_qs {
                    self.state.castling_rights &= !0x2;
                } else if Some(idx) == w_ks {
                    self.state.castling_rights &= !0x1;
                } else if Some(idx) == b_qs {
                    self.state.castling_rights &= !0x8;
                } else if Some(idx) == b_ks {
                    self.state.castling_rights &= !0x4;
                }
            }
        }

        if moving_piece.piece_type == PieceType::King {
            let dist_file = (mv.from.values[1] as isize - mv.to.values[1] as isize).abs();

            let mut other_axes_moved = false;
            for i in 0..self.geo.dimension {
                if i != 1 && mv.from.values[i] != mv.to.values[i] {
                    other_axes_moved = true;
                    break;
                }
            }

            if dist_file == 2 && !other_axes_moved {
                let is_kingside = mv.to.values[1] > mv.from.values[1];
                let rook_file_from = if is_kingside { 7 } else { 0 };
                let rook_file_to = if is_kingside { 5 } else { 3 };

                let mut rook_from_coords = mv.from.values.clone();
                rook_from_coords[1] = rook_file_from;

                let mut rook_to_coords = mv.from.values.clone();
                rook_to_coords[1] = rook_file_to;

                let r_from_idx = self.coords_to_index(&rook_from_coords);
                let r_to_idx = self.coords_to_index(&rook_to_coords);
                if let (Some(r_from), Some(r_to)) = (r_from_idx, r_to_idx) {
                    let rook_piece = Piece {
                        piece_type: PieceType::Rook,
                        owner: moving_piece.owner,
                    };
                    castling_rook_move = Some((r_from, r_to, rook_piece));
                }
            }
        }

        self.pieces.remove_piece_at_index(from_idx);
        self.pieces.remove_piece_at_index(to_idx);

        let piece_to_place = if let Some(promo_type) = mv.promotion {
            Piece {
                piece_type: promo_type,
                owner: moving_piece.owner,
            }
        } else {
            moving_piece
        };

        self.pieces.place_piece_at_index(to_idx, piece_to_place);

        self.hash_xor_piece(to_idx, piece_to_place);

        if let Some((r_from, r_to, r_piece)) = castling_rook_move {
            self.hash_xor_piece(r_from, r_piece);
            self.pieces.remove_piece_at_index(r_from);
            self.hash_xor_piece(r_to, r_piece);
            self.pieces.place_piece_at_index(r_to, r_piece);
        }

        if self.state.castling_rights > 0 {
            self.state.hash ^= self.zobrist.castling_keys[self.state.castling_rights as usize];
        }

        Ok(UnmakeInfo {
            captured,
            en_passant_target: saved_ep,
            castling_rights: saved_castling,
        })
    }

    pub fn unmake_move(&mut self, mv: &Move, info: UnmakeInfo) {
        if let Some(h) = self.state.history.pop() {
            self.state.hash = h;
        }

        self.state.en_passant_target = info.en_passant_target;
        self.state.castling_rights = info.castling_rights;

        let from_idx = self.coords_to_index(&mv.from.values).unwrap();
        let to_idx = self.coords_to_index(&mv.to.values).unwrap();

        let moved_piece = self
            .pieces
            .get_piece_at_index(to_idx)
            .expect("Piece missing in unmake");

        if moved_piece.piece_type == PieceType::King {
            let dist_file = (mv.from.values[1] as isize - mv.to.values[1] as isize).abs();
            let mut other_axes_moved = false;
            for i in 0..self.geo.dimension {
                if i != 1 && mv.from.values[i] != mv.to.values[i] {
                    other_axes_moved = true;
                    break;
                }
            }
            if dist_file == 2 && !other_axes_moved {
                let is_kingside = mv.to.values[1] > mv.from.values[1];
                let rook_file_from = if is_kingside { 7 } else { 0 };
                let rook_file_to = if is_kingside { 5 } else { 3 };

                let mut rook_from_coords = mv.from.values.clone();
                rook_from_coords[1] = rook_file_from;
                let mut rook_to_coords = mv.from.values.clone();
                rook_to_coords[1] = rook_file_to;

                let r_from_idx = self.coords_to_index(&rook_from_coords).unwrap();
                let r_to_idx = self.coords_to_index(&rook_to_coords).unwrap();

                let rook_piece = self
                    .pieces
                    .get_piece_at_index(r_to_idx)
                    .expect("Rook missing unmake");
                self.pieces.remove_piece_at_index(r_to_idx);
                self.pieces.place_piece_at_index(r_from_idx, rook_piece);
            }
        }

        self.pieces.remove_piece_at_index(to_idx);

        let original_piece = if mv.promotion.is_some() {
            Piece {
                piece_type: PieceType::Pawn,
                owner: moved_piece.owner,
            }
        } else {
            moved_piece
        };
        self.pieces.place_piece_at_index(from_idx, original_piece);

        if let Some((idx, piece)) = info.captured {
            self.pieces.place_piece_at_index(idx, piece);
        }
    }

    pub fn make_null_move(&mut self) -> UnmakeInfo {
        let saved_ep = self.state.en_passant_target;
        let saved_castling = self.state.castling_rights;

        self.state.history.push(self.state.hash);
        self.state.en_passant_target = None;

        self.state.hash ^= self.zobrist.black_to_move;

        if let Some((ep, _)) = saved_ep
            && ep < self.zobrist.en_passant_keys.len()
        {
            self.state.hash ^= self.zobrist.en_passant_keys[ep];
        }

        UnmakeInfo {
            captured: None,
            en_passant_target: saved_ep,
            castling_rights: saved_castling,
        }
    }

    pub fn unmake_null_move(&mut self, info: UnmakeInfo) {
        if let Some(h) = self.state.history.pop() {
            self.state.hash = h;
        }
        self.state.en_passant_target = info.en_passant_target;
        self.state.castling_rights = info.castling_rights;
    }

    pub fn get_king_coordinate(&self, player: Player) -> Option<Coordinate> {
        let occupancy = match player {
            Player::White => &self.pieces.white_occupancy,
            Player::Black => &self.pieces.black_occupancy,
        };

        // Iterate only set bits in kings bitboard (at most 2 kings)
        for idx in self.pieces.kings.iter_indices() {
            if occupancy.get_bit(idx) {
                return Some(Coordinate::new(self.index_to_coords(idx)));
            }
        }
        None
    }

    pub fn set_piece(&mut self, coord: &Coordinate, piece: Piece) -> Result<(), String> {
        let index = self.coords_to_index(&coord.values).ok_or("Invalid coord")?;
        self.pieces.remove_piece_at_index(index);
        self.pieces.place_piece_at_index(index, piece);
        self.state.hash = self
            .zobrist
            .get_hash(&self.pieces, &self.state, self.geo.total_cells);
        Ok(())
    }

    pub fn clear_cell(&mut self, coord: &Coordinate) {
        if let Some(index) = self.coords_to_index(&coord.values) {
            self.pieces.remove_piece_at_index(index);
        }
    }

    pub fn get_smallest_attacker(
        &self,
        target_sq: &Coordinate,
        attacker: Player,
    ) -> Option<(i32, usize)> {
        let occupancy = match attacker {
            Player::White => &self.pieces.white_occupancy,
            Player::Black => &self.pieces.black_occupancy,
        };

        let pawn_attacker_offsets = match attacker.opponent() {
            Player::White => &self.geo.cache.white_pawn_capture_offsets,
            Player::Black => &self.geo.cache.black_pawn_capture_offsets,
        };

        for offset in pawn_attacker_offsets {
            if let Some(src) =
                crate::domain::rules::Rules::apply_offset(&target_sq.values, offset, self.geo.side)
                && let Some(idx) = self.coords_to_index(&src)
                && occupancy.get_bit(idx)
                && self.pieces.pawns.get_bit(idx)
            {
                return Some((100, idx));
            }
        }

        for offset in &self.geo.cache.knight_offsets {
            if let Some(src) =
                crate::domain::rules::Rules::apply_offset(&target_sq.values, offset, self.geo.side)
                && let Some(idx) = self.coords_to_index(&src)
                && occupancy.get_bit(idx)
                && self.pieces.knights.get_bit(idx)
            {
                return Some((320, idx));
            }
        }

        for dir_info in &self.geo.cache.bishop_directions {
            let dir = &dir_info.offsets;
            if crate::domain::rules::Rules::scan_ray_for_threat(
                self,
                &target_sq.values,
                dir,
                attacker,
                &[PieceType::Bishop],
            ) && let Some(idx) =
                self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Bishop)
            {
                return Some((330, idx));
            }
        }

        for dir_info in &self.geo.cache.rook_directions {
            let dir = &dir_info.offsets;
            if crate::domain::rules::Rules::scan_ray_for_threat(
                self,
                &target_sq.values,
                dir,
                attacker,
                &[PieceType::Rook],
            ) && let Some(idx) =
                self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Rook)
            {
                return Some((500, idx));
            }
        }

        for dir_info in &self.geo.cache.bishop_directions {
            let dir = &dir_info.offsets;
            if let Some(idx) = self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Queen)
            {
                return Some((900, idx));
            }
        }
        for dir_info in &self.geo.cache.rook_directions {
            let dir = &dir_info.offsets;
            if let Some(idx) = self.trace_ray_for_piece(target_sq, dir, attacker, PieceType::Queen)
            {
                return Some((900, idx));
            }
        }

        for offset in &self.geo.cache.king_offsets {
            if let Some(src) =
                crate::domain::rules::Rules::apply_offset(&target_sq.values, offset, self.geo.side)
                && let Some(idx) = self.coords_to_index(&src)
                && occupancy.get_bit(idx)
                && self.pieces.kings.get_bit(idx)
            {
                return Some((20000, idx));
            }
        }

        None
    }

    fn trace_ray_for_piece(
        &self,
        origin_coord: &Coordinate,
        dir: &[isize],
        owner: Player,
        pt: PieceType,
    ) -> Option<usize> {
        let mut current = origin_coord.values.clone();
        let occupancy = self.pieces.white_occupancy.clone() | &self.pieces.black_occupancy;
        let my_occ = match owner {
            Player::White => &self.pieces.white_occupancy,
            Player::Black => &self.pieces.black_occupancy,
        };

        loop {
            if let Some(next) =
                crate::domain::rules::Rules::apply_offset(&current, dir, self.geo.side)
            {
                if let Some(idx) = self.coords_to_index(&next) {
                    if occupancy.get_bit(idx) {
                        if my_occ.get_bit(idx) {
                            let is_type = match pt {
                                PieceType::Bishop => self.pieces.bishops.get_bit(idx),
                                PieceType::Rook => self.pieces.rooks.get_bit(idx),
                                PieceType::Queen => self.pieces.queens.get_bit(idx),
                                _ => false,
                            };
                            if is_type {
                                return Some(idx);
                            }
                        }
                        break;
                    }
                    current = next;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        None
    }

    pub fn check_status(&self, _player_to_move: Player) -> GameResult {
        GameResult::InProgress
    }
}

// ── Backward-compatible field access via Deref-like properties ──────
// Code that previously accessed `board.white_occupancy`, `board.hash`, etc.
// now needs to use `board.pieces.white_occupancy`, `board.state.hash`, etc.
// The public fields on GenericBoard make this possible without method calls.

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Board(dim={}, side={})",
            self.geo.dimension, self.geo.side
        )
    }
}
