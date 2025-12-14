use crate::domain::board::Board;
use crate::domain::models::{PieceType, Player};
use crate::domain::rules::Rules;

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

const PHASE_PAWN: i32 = 0;
const PHASE_KNIGHT: i32 = 1;
const PHASE_BISHOP: i32 = 1;
const PHASE_ROOK: i32 = 2;
const PHASE_QUEEN: i32 = 4;

const MOBILITY_KNIGHT_MG: i32 = 4;
const MOBILITY_KNIGHT_EG: i32 = 4;

const MOBILITY_BISHOP_MG: i32 = 5;
const MOBILITY_BISHOP_EG: i32 = 5;

const MOBILITY_ROOK_MG: i32 = 2;
const MOBILITY_ROOK_EG: i32 = 4;

const MOBILITY_QUEEN_MG: i32 = 1;
const MOBILITY_QUEEN_EG: i32 = 2;

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

pub struct Evaluator;

impl Evaluator {
    pub fn evaluate(board: &Board) -> i32 {
        let (mg_score, eg_score, phase) = Self::gather_scores(board);

        let start_phase = Self::calculate_start_phase(board);

        let phase = phase.min(start_phase);

        let score = (mg_score * phase + eg_score * (start_phase - phase)) / start_phase;

        if board.hash ^ board.zobrist.black_to_move == board.hash {
            score
        } else {
            score
        }
    }

    fn gather_scores(board: &Board) -> (i32, i32, i32) {
        let mut mg_score = 0;
        let mut eg_score = 0;
        let mut phase = 0;

        let center = (board.side as f32 - 1.0) / 2.0;

        for (idx, piece_type) in Self::iter_pieces(board, Player::White) {
            let (mg, eg, p) = Self::evaluate_piece(board, idx, piece_type, Player::White, center);
            mg_score += mg;
            eg_score += eg;
            phase += p;
        }

        for (idx, piece_type) in Self::iter_pieces(board, Player::Black) {
            let (mg, eg, p) = Self::evaluate_piece(board, idx, piece_type, Player::Black, center);
            mg_score -= mg;
            eg_score -= eg;
            phase += p;
        }

        (mg_score, eg_score, phase)
    }

    fn iter_pieces<'a>(
        board: &'a Board,
        player: Player,
    ) -> impl Iterator<Item = (usize, PieceType)> + 'a {
        let occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        occupancy.iter_indices().map(move |idx| {
            let pt = if board.pawns.get_bit(idx) {
                PieceType::Pawn
            } else if board.knights.get_bit(idx) {
                PieceType::Knight
            } else if board.bishops.get_bit(idx) {
                PieceType::Bishop
            } else if board.rooks.get_bit(idx) {
                PieceType::Rook
            } else if board.queens.get_bit(idx) {
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
        _player: Player,
        center: f32,
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

        let coords = board.index_to_coords(index);
        let dist: f32 = coords.iter().map(|&c| (c as f32 - center).abs()).sum();
        let dist_int = dist as i32;

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
            let mobility = count_mobility(board, index, piece_type);
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

    fn calculate_start_phase(_board: &Board) -> i32 {
        24
    }
}

fn count_mobility(board: &Board, index: usize, piece_type: PieceType) -> i32 {
    Rules::count_piece_mobility(board, index, piece_type)
}
