use smallvec::SmallVec;

use crate::domain::board::Board;
use crate::domain::models::{PieceType, Player};
use crate::domain::rules::{Rules, attacks};

// ── Material values ──────────────────────────────────────────────────

const PAWN_MG: i32 = 100;
const PAWN_EG: i32 = 150;

const KNIGHT_MG: i32 = 320;
const KNIGHT_EG: i32 = 300;

const BISHOP_MG: i32 = 330;
const BISHOP_EG: i32 = 330;

const ROOK_MG: i32 = 500;
const ROOK_EG: i32 = 500;

const QUEEN_MG: i32 = 900;
const QUEEN_EG: i32 = 900;

// ── Phase weights ────────────────────────────────────────────────────

const PHASE_PAWN: i32 = 0;
const PHASE_KNIGHT: i32 = 1;
const PHASE_BISHOP: i32 = 1;
const PHASE_ROOK: i32 = 2;
const PHASE_QUEEN: i32 = 4;

// ── Mobility ─────────────────────────────────────────────────────────

const MOBILITY_KNIGHT_MG: i32 = 4;
const MOBILITY_KNIGHT_EG: i32 = 4;

const MOBILITY_BISHOP_MG: i32 = 5;
const MOBILITY_BISHOP_EG: i32 = 5;

const MOBILITY_ROOK_MG: i32 = 2;
const MOBILITY_ROOK_EG: i32 = 4;

const MOBILITY_QUEEN_MG: i32 = 1;
const MOBILITY_QUEEN_EG: i32 = 2;

// ── Piece-square table (distance from center) ────────────────────────

const PST_PAWN_DIST_PENALTY_MG: i32 = 2;
const PST_PAWN_DIST_PENALTY_EG: i32 = 5;

const PST_KNIGHT_DIST_PENALTY_MG: i32 = 4;
const PST_KNIGHT_DIST_PENALTY_EG: i32 = 4;

const PST_BISHOP_DIST_PENALTY_MG: i32 = 1;
const PST_BISHOP_DIST_PENALTY_EG: i32 = 1;

const PST_ROOK_DIST_PENALTY_MG: i32 = 0;
const PST_ROOK_DIST_PENALTY_EG: i32 = 0;

const PST_QUEEN_DIST_PENALTY_MG: i32 = 1;
const PST_QUEEN_DIST_PENALTY_EG: i32 = 2;

const PST_KING_DIST_BONUS_MG: i32 = 5;
const PST_KING_DIST_PENALTY_EG: i32 = 10;

// ── King safety (N-dimensional, geometry-normalized) ─────────────────
//
// Four signals, all dimension-independent with ratios bounded in [0,1]:
//
//   1. PAWN SHELTER — only pawns in the forward king zone count as
//      durable shelter (other pieces move away).
//
//   2. OPEN LINES — walk each sliding direction from the king. If no
//      friendly pawn shields the ray within MAX_PAWN_SCAN_DEPTH steps,
//      it's open. If an enemy slider sits on that open ray, it's attacked.
//
//   3. HEAVY PIECE TROPISM — Chebyshev distance of enemy Q/R to king.
//      Closer heavy pieces are more dangerous.
//
//   4. KING ZONE CONTACT — enemy pieces in the Chebyshev-1 neighborhood,
//      weighted by piece type (queen=6, rook=3, bishop/knight=2, pawn=1).

const PAWN_SHELTER_TOTAL_MG: i32 = 160;
const PAWN_SHELTER_TOTAL_EG: i32 = 0;

const OPEN_LINE_PENALTY_MG: i32 = 80;
const OPEN_LINE_PENALTY_EG: i32 = 10;
const ATTACKED_LINE_PENALTY_MG: i32 = 120;
const ATTACKED_LINE_PENALTY_EG: i32 = 15;
const MAX_PAWN_SCAN_DEPTH: usize = 3;

const TROPISM_QUEEN_WEIGHT: i32 = 4;
const TROPISM_ROOK_WEIGHT: i32 = 2;
const TROPISM_TOTAL_MG: i32 = 50;
const TROPISM_TOTAL_EG: i32 = 10;

const KING_ZONE_CONTACT_MG: i32 = 70;
const KING_ZONE_CONTACT_EG: i32 = 10;

// ── Pawn structure ───────────────────────────────────────────────────

const PASSED_PAWN_BONUS_MG: i32 = 20;
const PASSED_PAWN_BONUS_EG: i32 = 40;
const PASSED_PAWN_ADVANCE_MG: i32 = 3;
const PASSED_PAWN_ADVANCE_EG: i32 = 6;

const ISOLATED_PAWN_PENALTY_MG: i32 = 10;
const ISOLATED_PAWN_PENALTY_EG: i32 = 15;

const DOUBLED_PAWN_PENALTY_MG: i32 = 10;
const DOUBLED_PAWN_PENALTY_EG: i32 = 15;

const CONNECTED_PAWN_BONUS_MG: i32 = 5;
const CONNECTED_PAWN_BONUS_EG: i32 = 8;

// ── Piece bonuses ────────────────────────────────────────────────────

const BISHOP_PAIR_BONUS_MG: i32 = 30;
const BISHOP_PAIR_BONUS_EG: i32 = 50;

const ROOK_OPEN_FILE_BONUS_MG: i32 = 15;
const ROOK_OPEN_FILE_BONUS_EG: i32 = 10;

const ROOK_SEMI_OPEN_FILE_BONUS_MG: i32 = 8;
const ROOK_SEMI_OPEN_FILE_BONUS_EG: i32 = 5;

const CASTLING_RIGHTS_BONUS_MG: i32 = 10;
const CASTLING_RIGHTS_BONUS_EG: i32 = 0;

// ── Trade bonus (simplification awareness) ─────────────────────────
//
// When one side has more non-pawn material, piece trades amplify
// that advantage.  The bonus scales with:
//   (a) material imbalance (non-pawn, centipawns)
//   (b) how much material has left the board (simplification ratio)
//
// Fully dimension-agnostic: uses only piece counts and stored
// start_phase, not board topology.

const TRADE_BONUS_MG: i32 = 0; // negligible in middlegame
const TRADE_BONUS_EG: i32 = 48; // ~½ pawn at maximum simplification

pub struct Evaluator;

impl Evaluator {
    pub fn evaluate(board: &Board) -> i32 {
        let (mg_score, eg_score, phase) = Self::gather_scores(board);

        let start_phase = board.state.start_phase.max(1);
        let phase = phase.min(start_phase);

        (mg_score * phase + eg_score * (start_phase - phase)) / start_phase
    }

    fn gather_scores(board: &Board) -> (i32, i32, i32) {
        let mut mg_score = 0;
        let mut eg_score = 0;
        let mut phase = 0;

        // Track non-pawn material per side (for trade bonus)
        let mut white_npm = 0i32;
        let mut black_npm = 0i32;

        // Per-piece evaluation (material + PST + mobility)
        for (idx, piece_type) in Self::iter_pieces(board, Player::White) {
            let (mg, eg, p) = Self::evaluate_piece(board, idx, piece_type, Player::White);
            mg_score += mg;
            eg_score += eg;
            phase += p;
            if piece_type != PieceType::Pawn && piece_type != PieceType::King {
                white_npm += Self::piece_value_mg(piece_type);
            }
        }

        for (idx, piece_type) in Self::iter_pieces(board, Player::Black) {
            let (mg, eg, p) = Self::evaluate_piece(board, idx, piece_type, Player::Black);
            mg_score -= mg;
            eg_score -= eg;
            phase += p;
            if piece_type != PieceType::Pawn && piece_type != PieceType::King {
                black_npm += Self::piece_value_mg(piece_type);
            }
        }

        // Positional evaluation (N-dimensional)
        let (ks_mg, ks_eg) = Self::evaluate_king_safety(board);
        mg_score += ks_mg;
        eg_score += ks_eg;

        let (ps_mg, ps_eg) = Self::evaluate_pawn_structure(board);
        mg_score += ps_mg;
        eg_score += ps_eg;

        let (bp_mg, bp_eg) = Self::evaluate_bishop_pair(board);
        mg_score += bp_mg;
        eg_score += bp_eg;

        let (rf_mg, rf_eg) = Self::evaluate_rook_files(board);
        mg_score += rf_mg;
        eg_score += rf_eg;

        let (cr_mg, cr_eg) = Self::evaluate_castling_rights(board);
        mg_score += cr_mg;
        eg_score += cr_eg;

        // ── Trade bonus ──
        // npm_diff > 0 → white has more material → white benefits from trades.
        // simplification = (start_phase - phase) / start_phase ∈ [0, 1].
        // We multiply in integer arithmetic to avoid float.
        let start_phase = board.state.start_phase.max(1);
        let npm_diff = white_npm - black_npm;
        let simplification = start_phase - phase.min(start_phase);
        // trade = npm_diff_sign * BONUS * simplification / start_phase
        // Normalize npm_diff to roughly ±1..±5 range by dividing by rook value.
        let npm_norm = npm_diff / ROOK_MG.max(1);
        mg_score += npm_norm * TRADE_BONUS_MG * simplification / start_phase;
        eg_score += npm_norm * TRADE_BONUS_EG * simplification / start_phase;

        (mg_score, eg_score, phase)
    }

    fn piece_value_mg(piece_type: PieceType) -> i32 {
        match piece_type {
            PieceType::Knight => KNIGHT_MG,
            PieceType::Bishop => BISHOP_MG,
            PieceType::Rook => ROOK_MG,
            PieceType::Queen => QUEEN_MG,
            _ => 0,
        }
    }

    fn iter_pieces<'a>(
        board: &'a Board,
        player: Player,
    ) -> impl Iterator<Item = (usize, PieceType)> + 'a {
        let occupancy = match player {
            Player::White => &board.pieces.white_occupancy,
            Player::Black => &board.pieces.black_occupancy,
        };

        occupancy.iter_indices().map(move |idx| {
            let pt = if board.pieces.pawns.get_bit(idx) {
                PieceType::Pawn
            } else if board.pieces.knights.get_bit(idx) {
                PieceType::Knight
            } else if board.pieces.bishops.get_bit(idx) {
                PieceType::Bishop
            } else if board.pieces.rooks.get_bit(idx) {
                PieceType::Rook
            } else if board.pieces.queens.get_bit(idx) {
                PieceType::Queen
            } else {
                PieceType::King
            };
            (idx, pt)
        })
    }

    fn evaluate_piece(
        board: &Board,
        index: usize,
        piece_type: PieceType,
        player: Player,
    ) -> (i32, i32, i32) {
        let mut mg = 0;
        let mut eg = 0;
        let mut phase = 0;

        let (mat_mg, mat_eg, ph) = match piece_type {
            PieceType::Pawn => (PAWN_MG, PAWN_EG, PHASE_PAWN),
            PieceType::Knight => (KNIGHT_MG, KNIGHT_EG, PHASE_KNIGHT),
            PieceType::Bishop => (BISHOP_MG, BISHOP_EG, PHASE_BISHOP),
            PieceType::Rook => (ROOK_MG, ROOK_EG, PHASE_ROOK),
            PieceType::Queen => (QUEEN_MG, QUEEN_EG, PHASE_QUEEN),
            PieceType::King => (0, 0, 0),
        };
        mg += mat_mg;
        eg += mat_eg;
        phase += ph;

        let dist_int = board.geo.cache.center_dist[index];

        let (pst_mg, pst_eg) = match piece_type {
            PieceType::Pawn => (
                -dist_int * PST_PAWN_DIST_PENALTY_MG,
                -dist_int * PST_PAWN_DIST_PENALTY_EG,
            ),
            PieceType::Knight => (
                -dist_int * PST_KNIGHT_DIST_PENALTY_MG,
                -dist_int * PST_KNIGHT_DIST_PENALTY_EG,
            ),
            PieceType::Bishop => (
                -dist_int * PST_BISHOP_DIST_PENALTY_MG,
                -dist_int * PST_BISHOP_DIST_PENALTY_EG,
            ),
            PieceType::Rook => (
                -dist_int * PST_ROOK_DIST_PENALTY_MG,
                -dist_int * PST_ROOK_DIST_PENALTY_EG,
            ),
            PieceType::Queen => (
                -dist_int * PST_QUEEN_DIST_PENALTY_MG,
                -dist_int * PST_QUEEN_DIST_PENALTY_EG,
            ),
            PieceType::King => (
                dist_int * PST_KING_DIST_BONUS_MG,
                -dist_int * PST_KING_DIST_PENALTY_EG,
            ),
        };
        mg += pst_mg;
        eg += pst_eg;

        if piece_type != PieceType::Pawn && piece_type != PieceType::King {
            let mobility = Rules::count_piece_mobility_for(board, index, piece_type, player);
            let (mob_mg, mob_eg) = match piece_type {
                PieceType::Knight => (mobility * MOBILITY_KNIGHT_MG, mobility * MOBILITY_KNIGHT_EG),
                PieceType::Bishop => (mobility * MOBILITY_BISHOP_MG, mobility * MOBILITY_BISHOP_EG),
                PieceType::Rook => (mobility * MOBILITY_ROOK_MG, mobility * MOBILITY_ROOK_EG),
                PieceType::Queen => (mobility * MOBILITY_QUEEN_MG, mobility * MOBILITY_QUEEN_EG),
                _ => (0, 0),
            };
            mg += mob_mg;
            eg += mob_eg;
        }

        (mg, eg, phase)
    }

    // ── King Safety (N-dimensional) ───────────────────────────────────

    fn evaluate_king_safety(board: &Board) -> (i32, i32) {
        let mut mg = 0i32;
        let mut eg = 0i32;

        let zone_size = board.geo.cache.king_offsets.len() as i32;
        if zone_size == 0 {
            return (0, 0);
        }

        // Forward zone sizes (pre-computed once, same for all positions).
        let forward_white = board
            .geo
            .cache
            .king_offsets
            .iter()
            .filter(|off| off[0] > 0)
            .count() as i32;
        let forward_black = board
            .geo
            .cache
            .king_offsets
            .iter()
            .filter(|off| off[0] < 0)
            .count() as i32;

        let num_rook_dirs = board.geo.cache.rook_directions.len() as i32;
        let num_bishop_dirs = board.geo.cache.bishop_directions.len() as i32;
        let total_dirs = num_rook_dirs + num_bishop_dirs;
        let max_scan = MAX_PAWN_SCAN_DEPTH.min(board.side() - 1);
        let max_dist = (board.side() as i32) - 1;

        for player in [Player::White, Player::Black] {
            let sign = if player == Player::White { 1 } else { -1 };
            let king_coord = match board.get_king_coordinate(player) {
                Some(k) => k,
                None => continue,
            };
            let king_idx = board.coords_to_index(&king_coord.values).unwrap();

            let my_occ = match player {
                Player::White => &board.pieces.white_occupancy,
                Player::Black => &board.pieces.black_occupancy,
            };
            let enemy_occ = match player {
                Player::White => &board.pieces.black_occupancy,
                Player::Black => &board.pieces.white_occupancy,
            };
            let enemy = player.opponent();

            let forward_size = match player {
                Player::White => forward_white,
                Player::Black => forward_black,
            };

            // ── Signal 1: Pawn shelter (forward zone, pawns only) ──
            // Use precomputed king targets + filter forward offsets by checking
            // coordinate difference on axis 0.
            let mut shelter_count = 0i32;
            let king_rank = king_coord.values[0];
            for &neighbor_idx in &board.geo.cache.king_targets[king_idx] {
                let neighbor_rank = board.geo.cache.index_to_coords[neighbor_idx][0];
                let is_forward = match player {
                    Player::White => neighbor_rank > king_rank,
                    Player::Black => neighbor_rank < king_rank,
                };
                if !is_forward {
                    continue;
                }
                if my_occ.get_bit(neighbor_idx) && board.pieces.pawns.get_bit(neighbor_idx) {
                    shelter_count += 1;
                }
            }
            if forward_size > 0 {
                mg += sign * (shelter_count * PAWN_SHELTER_TOTAL_MG) / forward_size;
                eg += sign * (shelter_count * PAWN_SHELTER_TOTAL_EG) / forward_size;
            }

            // ── Signal 2: Open lines toward king ──

            let mut open_count = 0i32;
            let mut attacked_count = 0i32;

            for dir_info in &board.geo.cache.rook_directions {
                if !Self::ray_has_pawn_shield_idx(board, king_idx, dir_info, my_occ, max_scan) {
                    open_count += 1;
                    if attacks::scan_ray_for_threat_idx(
                        board,
                        king_idx,
                        dir_info,
                        enemy,
                        &[PieceType::Rook, PieceType::Queen],
                    ) {
                        attacked_count += 1;
                    }
                }
            }

            for dir_info in &board.geo.cache.bishop_directions {
                if !Self::ray_has_pawn_shield_idx(board, king_idx, dir_info, my_occ, max_scan) {
                    open_count += 1;
                    if attacks::scan_ray_for_threat_idx(
                        board,
                        king_idx,
                        dir_info,
                        enemy,
                        &[PieceType::Bishop, PieceType::Queen],
                    ) {
                        attacked_count += 1;
                    }
                }
            }

            if total_dirs > 0 {
                mg -= sign * (open_count * OPEN_LINE_PENALTY_MG) / total_dirs;
                eg -= sign * (open_count * OPEN_LINE_PENALTY_EG) / total_dirs;
                mg -= sign * (attacked_count * ATTACKED_LINE_PENALTY_MG) / total_dirs;
                eg -= sign * (attacked_count * ATTACKED_LINE_PENALTY_EG) / total_dirs;
            }

            // ── Signal 3: Heavy piece tropism (Q/R Chebyshev distance) ──

            let mut tropism_score = 0i32;
            for idx in enemy_occ.iter_indices() {
                let is_queen = board.pieces.queens.get_bit(idx);
                let is_rook = board.pieces.rooks.get_bit(idx);
                if !is_queen && !is_rook {
                    continue;
                }
                let pc = &board.geo.cache.index_to_coords[idx];
                let chebyshev = king_coord
                    .values
                    .iter()
                    .zip(pc.iter())
                    .map(|(&k, &p)| (k as i32 - p as i32).abs())
                    .max()
                    .unwrap_or(0);
                let proximity = (max_dist - chebyshev).max(0);
                let weight = if is_queen {
                    TROPISM_QUEEN_WEIGHT
                } else {
                    TROPISM_ROOK_WEIGHT
                };
                tropism_score += proximity * weight;
            }

            let normalizer = max_dist * (TROPISM_QUEEN_WEIGHT + 2 * TROPISM_ROOK_WEIGHT);
            if normalizer > 0 {
                mg -= sign * (tropism_score * TROPISM_TOTAL_MG) / normalizer;
                eg -= sign * (tropism_score * TROPISM_TOTAL_EG) / normalizer;
            }

            // ── Signal 4: King zone enemy contact (weighted by piece type) ──

            let mut contact_score = 0i32;
            for &neighbor_idx in &board.geo.cache.king_targets[king_idx] {
                if enemy_occ.get_bit(neighbor_idx) {
                    let w = if board.pieces.queens.get_bit(neighbor_idx) {
                        6
                    } else if board.pieces.rooks.get_bit(neighbor_idx) {
                        3
                    } else if board.pieces.bishops.get_bit(neighbor_idx)
                        || board.pieces.knights.get_bit(neighbor_idx)
                    {
                        2
                    } else if board.pieces.pawns.get_bit(neighbor_idx) {
                        1
                    } else {
                        0
                    };
                    contact_score += w;
                }
            }
            let max_contact = zone_size * 6;
            if max_contact > 0 {
                mg -= sign * (contact_score * KING_ZONE_CONTACT_MG) / max_contact;
                eg -= sign * (contact_score * KING_ZONE_CONTACT_EG) / max_contact;
            }
        }

        (mg, eg)
    }

    /// Index-based pawn shield scan — no allocations.
    fn ray_has_pawn_shield_idx(
        board: &Board,
        origin_idx: usize,
        dir_info: &crate::domain::board::cache::DirectionInfo,
        friendly_occ: &crate::domain::board::BitBoardLarge,
        max_steps: usize,
    ) -> bool {
        let stride = dir_info.stride;
        let mask = &board.geo.cache.validity_masks[dir_info.id * board.side() + 1];
        let mut idx = origin_idx;
        for _ in 0..max_steps {
            if !mask.get_bit(idx) {
                return false;
            }
            idx = (idx as isize + stride) as usize;
            if friendly_occ.get_bit(idx) && board.pieces.pawns.get_bit(idx) {
                return true;
            }
            if board.pieces.white_occupancy.get_bit(idx)
                || board.pieces.black_occupancy.get_bit(idx)
            {
                return false;
            }
        }
        false
    }

    // ── Pawn Structure (N-dimensional) ───────────────────────────────

    fn evaluate_pawn_structure(board: &Board) -> (i32, i32) {
        let mut mg = 0;
        let mut eg = 0;
        let dim = board.dimension();

        for player in [Player::White, Player::Black] {
            let sign = if player == Player::White { 1 } else { -1 };
            let my_pawns = Self::get_pawn_indices(board, player);
            let enemy_pawns = Self::get_pawn_indices(board, player.opponent());

            for &pawn_idx in &my_pawns {
                let coords = &board.geo.cache.index_to_coords[pawn_idx];

                // Passed pawn: no enemy pawn ahead on same or adjacent file column
                if Self::is_passed_pawn(board, coords, player, &enemy_pawns, dim) {
                    let advancement = match player {
                        Player::White => coords[0] as i32,
                        Player::Black => (board.side() as i32 - 1) - coords[0] as i32,
                    };
                    mg += sign * (PASSED_PAWN_BONUS_MG + advancement * PASSED_PAWN_ADVANCE_MG);
                    eg += sign * (PASSED_PAWN_BONUS_EG + advancement * PASSED_PAWN_ADVANCE_EG);
                }

                // Isolated pawn: no friendly pawn on adjacent file columns
                if Self::is_isolated_pawn(board, coords, &my_pawns, dim) {
                    mg -= sign * ISOLATED_PAWN_PENALTY_MG;
                    eg -= sign * ISOLATED_PAWN_PENALTY_EG;
                }

                // Doubled pawn: another friendly pawn on same file column
                if Self::is_doubled_pawn(board, coords, &my_pawns, pawn_idx, dim) {
                    mg -= sign * DOUBLED_PAWN_PENALTY_MG;
                    eg -= sign * DOUBLED_PAWN_PENALTY_EG;
                }

                // Connected pawn: protected by a friendly pawn
                if Self::is_connected_pawn(board, pawn_idx, player) {
                    mg += sign * CONNECTED_PAWN_BONUS_MG;
                    eg += sign * CONNECTED_PAWN_BONUS_EG;
                }
            }
        }

        (mg, eg)
    }

    fn get_pawn_indices(board: &Board, player: Player) -> Vec<usize> {
        let occ = match player {
            Player::White => &board.pieces.white_occupancy,
            Player::Black => &board.pieces.black_occupancy,
        };
        occ.iter_indices()
            .filter(|&idx| board.pieces.pawns.get_bit(idx))
            .collect()
    }

    /// A pawn is passed if no enemy pawn can block or capture it.
    /// N-dim: no enemy pawn exists with same higher-dim coords, adjacent file
    /// (axis 1 ±1), and ahead on rank (axis 0).
    fn is_passed_pawn(
        board: &Board,
        coords: &SmallVec<[u8; 8]>,
        player: Player,
        enemy_pawn_indices: &[usize],
        dim: usize,
    ) -> bool {
        let rank = coords[0];
        let file = coords[1];

        for &enemy_idx in enemy_pawn_indices {
            let ec = &board.geo.cache.index_to_coords[enemy_idx];

            // Higher dimensions must match
            if !higher_dims_match(coords, ec, dim) {
                continue;
            }

            // Adjacent or same file?
            let file_diff = (ec[1] as i32 - file as i32).abs();
            if file_diff > 1 {
                continue;
            }

            // Is enemy pawn ahead or at same rank on adjacent file?
            let enemy_ahead = match player {
                Player::White => ec[0] > rank,
                Player::Black => ec[0] < rank,
            };
            let same_rank_adjacent = ec[0] == rank && file_diff == 1;

            if enemy_ahead || same_rank_adjacent {
                return false;
            }
        }
        true
    }

    /// No friendly pawn on adjacent file columns (axis 1 ±1, higher dims match).
    fn is_isolated_pawn(
        board: &Board,
        coords: &SmallVec<[u8; 8]>,
        my_pawn_indices: &[usize],
        dim: usize,
    ) -> bool {
        let file = coords[1];
        for &idx in my_pawn_indices {
            let oc = &board.geo.cache.index_to_coords[idx];
            let file_diff = (oc[1] as i32 - file as i32).abs();
            if file_diff != 1 {
                continue;
            }
            if higher_dims_match(coords, oc, dim) {
                return false;
            }
        }
        true
    }

    /// Another friendly pawn on same file column (axis 1 + higher dims match).
    fn is_doubled_pawn(
        board: &Board,
        coords: &SmallVec<[u8; 8]>,
        my_pawn_indices: &[usize],
        self_idx: usize,
        dim: usize,
    ) -> bool {
        let file = coords[1];
        for &idx in my_pawn_indices {
            if idx == self_idx {
                continue;
            }
            let oc = &board.geo.cache.index_to_coords[idx];
            if oc[1] != file {
                continue;
            }
            if higher_dims_match(coords, oc, dim) {
                return true;
            }
        }
        false
    }

    /// A pawn is connected if a friendly pawn protects it (uses N-dim pawn
    /// capture offsets from the geometry cache).
    fn is_connected_pawn(board: &Board, pawn_idx: usize, player: Player) -> bool {
        // We look for a friendly pawn that *could attack* this square.
        // Use the opponent's capture targets from this square to find
        // squares where a defending pawn would be.
        let defender_targets = match player {
            Player::White => &board.geo.cache.black_pawn_capture_targets[pawn_idx],
            Player::Black => &board.geo.cache.white_pawn_capture_targets[pawn_idx],
        };
        let my_occ = match player {
            Player::White => &board.pieces.white_occupancy,
            Player::Black => &board.pieces.black_occupancy,
        };
        for &idx in defender_targets {
            if my_occ.get_bit(idx) && board.pieces.pawns.get_bit(idx) {
                return true;
            }
        }
        false
    }

    // ── Bishop pair ──────────────────────────────────────────────────

    fn evaluate_bishop_pair(board: &Board) -> (i32, i32) {
        let mut mg = 0;
        let mut eg = 0;

        for player in [Player::White, Player::Black] {
            let sign = if player == Player::White { 1 } else { -1 };
            let occ = match player {
                Player::White => &board.pieces.white_occupancy,
                Player::Black => &board.pieces.black_occupancy,
            };

            let mut bishop_count = 0u32;
            for idx in occ.iter_indices() {
                if board.pieces.bishops.get_bit(idx) {
                    bishop_count += 1;
                }
            }
            if bishop_count >= 2 {
                mg += sign * BISHOP_PAIR_BONUS_MG;
                eg += sign * BISHOP_PAIR_BONUS_EG;
            }
        }

        (mg, eg)
    }

    // ── Rook on open/semi-open file ──────────────────────────────────

    fn evaluate_rook_files(board: &Board) -> (i32, i32) {
        let mut mg = 0;
        let mut eg = 0;

        for player in [Player::White, Player::Black] {
            let sign = if player == Player::White { 1 } else { -1 };
            let occ = match player {
                Player::White => &board.pieces.white_occupancy,
                Player::Black => &board.pieces.black_occupancy,
            };

            for idx in occ.iter_indices() {
                if !board.pieces.rooks.get_bit(idx) {
                    continue;
                }

                let has_friendly = Self::file_column_has_pawn(board, idx, player);
                let has_enemy = Self::file_column_has_pawn(board, idx, player.opponent());

                if !has_friendly && !has_enemy {
                    mg += sign * ROOK_OPEN_FILE_BONUS_MG;
                    eg += sign * ROOK_OPEN_FILE_BONUS_EG;
                } else if !has_friendly {
                    mg += sign * ROOK_SEMI_OPEN_FILE_BONUS_MG;
                    eg += sign * ROOK_SEMI_OPEN_FILE_BONUS_EG;
                }
            }
        }

        (mg, eg)
    }

    /// Check if any pawn of `player` exists on the same file column
    /// (same Axis 1 + same higher-dim coords, any rank on Axis 0).
    /// Uses index arithmetic: axis 0 has stride 1 in the indexing scheme.
    fn file_column_has_pawn(board: &Board, piece_idx: usize, player: Player) -> bool {
        let occ = match player {
            Player::White => &board.pieces.white_occupancy,
            Player::Black => &board.pieces.black_occupancy,
        };
        let base = piece_idx - (piece_idx % board.side());
        for rank in 0..board.side() {
            let idx = base + rank;
            if occ.get_bit(idx) && board.pieces.pawns.get_bit(idx) {
                return true;
            }
        }
        false
    }

    // ── Castling rights bonus ────────────────────────────────────────

    fn evaluate_castling_rights(board: &Board) -> (i32, i32) {
        let white_rights = (board.state.castling_rights & 0x3).count_ones() as i32;
        let black_rights = ((board.state.castling_rights >> 2) & 0x3).count_ones() as i32;

        let mg = (white_rights - black_rights) * CASTLING_RIGHTS_BONUS_MG;
        let eg = (white_rights - black_rights) * CASTLING_RIGHTS_BONUS_EG;

        (mg, eg)
    }
}

/// Check if all axes beyond 0 and 1 match between two coordinate vectors.
fn higher_dims_match(a: &SmallVec<[u8; 8]>, b: &SmallVec<[u8; 8]>, dim: usize) -> bool {
    for d in 2..dim {
        if a[d] != b[d] {
            return false;
        }
    }
    true
}
