use std::fmt;

use crate::domain::board::Board;
use crate::domain::models::{Move, PieceType, Player};
use crate::domain::rules::Rules;

#[derive(Debug)]
pub enum SanError {
    InvalidFormat(String),
    NoMatchingMove(String),
    AmbiguousMove(String),
}

impl fmt::Display for SanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(s) => write!(f, "Invalid SAN format: '{s}'"),
            Self::NoMatchingMove(s) => write!(f, "No legal move matches SAN: '{s}'"),
            Self::AmbiguousMove(s) => write!(f, "Multiple legal moves match SAN: '{s}'"),
        }
    }
}

impl std::error::Error for SanError {}

/// Parse a SAN (Standard Algebraic Notation) move string and return the
/// matching legal move. Requires the board to resolve disambiguation.
///
/// Handles: `e4`, `Nf3`, `Bxe5`, `exd5`, `O-O`, `O-O-O`, `e8=Q`,
///          `Nbd2`, `R1a3`, `Qh4e1`, check/mate indicators (`+`, `#`).
pub fn parse_san(board: &mut Board, player: Player, san: &str) -> Result<Move, SanError> {
    // Strip annotations
    let clean = san.trim_end_matches(['+', '#', '!', '?']);

    if clean.is_empty() {
        return Err(SanError::InvalidFormat(san.to_string()));
    }

    let legal_moves = Rules::generate_legal_moves(board, player);

    // Handle castling
    if clean == "O-O-O" || clean == "0-0-0" {
        return find_castling_move(&legal_moves, player, true, san);
    }
    if clean == "O-O" || clean == "0-0" {
        return find_castling_move(&legal_moves, player, false, san);
    }

    let mut chars: Vec<char> = clean.chars().collect();

    // Determine piece type
    let piece_type = if chars[0].is_ascii_uppercase() {
        let pt = match chars[0] {
            'K' => PieceType::King,
            'Q' => PieceType::Queen,
            'R' => PieceType::Rook,
            'B' => PieceType::Bishop,
            'N' => PieceType::Knight,
            _ => return Err(SanError::InvalidFormat(san.to_string())),
        };
        chars.remove(0);
        pt
    } else {
        PieceType::Pawn
    };

    // Parse promotion from end (e.g., "=Q", "=N")
    let promotion = if chars.len() >= 2 && chars[chars.len() - 2] == '=' {
        let promo = match chars[chars.len() - 1] {
            'Q' => PieceType::Queen,
            'R' => PieceType::Rook,
            'B' => PieceType::Bishop,
            'N' => PieceType::Knight,
            _ => return Err(SanError::InvalidFormat(san.to_string())),
        };
        chars.truncate(chars.len() - 2);
        Some(promo)
    } else {
        // Also handle promotion without '=' (e.g., "e8Q")
        if piece_type == PieceType::Pawn
            && chars.len() >= 3
            && chars.last().is_some_and(|c| "QRBN".contains(*c))
        {
            let promo = match chars.pop().unwrap() {
                'Q' => PieceType::Queen,
                'R' => PieceType::Rook,
                'B' => PieceType::Bishop,
                'N' => PieceType::Knight,
                _ => unreachable!(),
            };
            Some(promo)
        } else {
            None
        }
    };

    // Need at least 2 chars for destination square
    if chars.len() < 2 {
        return Err(SanError::InvalidFormat(san.to_string()));
    }

    // Destination square is always the last 2 characters
    let dest_file = chars[chars.len() - 2];
    let dest_rank = chars[chars.len() - 1];

    if !dest_file.is_ascii_lowercase() || !dest_rank.is_ascii_digit() {
        return Err(SanError::InvalidFormat(san.to_string()));
    }

    let to_file = (dest_file as u8 - b'a') as usize;
    let to_rank = (dest_rank as u8 - b'1') as usize;

    if to_file >= 8 || to_rank >= 8 {
        return Err(SanError::InvalidFormat(san.to_string()));
    }

    // Everything before the destination (minus piece type already removed) is
    // disambiguation + optional capture marker 'x'
    let middle: Vec<char> = chars[..chars.len() - 2]
        .iter()
        .filter(|&&c| c != 'x')
        .copied()
        .collect();

    let mut dis_file: Option<usize> = None;
    let mut dis_rank: Option<usize> = None;

    for &c in &middle {
        if c.is_ascii_lowercase() && ('a'..='h').contains(&c) {
            dis_file = Some((c as u8 - b'a') as usize);
        } else if c.is_ascii_digit() && ('1'..='8').contains(&c) {
            dis_rank = Some((c as u8 - b'1') as usize);
        }
    }

    // Filter legal moves to find the match
    let candidates: Vec<&Move> = legal_moves
        .iter()
        .filter(|mv| {
            // Destination must match
            if mv.to.values[0] as usize != to_rank || mv.to.values[1] as usize != to_file {
                return false;
            }

            // Promotion must match
            if mv.promotion != promotion {
                return false;
            }

            // Piece at source must match
            let source_piece = board
                .pieces
                .get_piece_at_index(board.coords_to_index(&mv.from.values).unwrap_or(usize::MAX));
            match source_piece {
                Some(p) if p.piece_type == piece_type && p.owner == player => {}
                _ => return false,
            }

            // Disambiguation
            if let Some(df) = dis_file
                && mv.from.values[1] as usize != df
            {
                return false;
            }
            if let Some(dr) = dis_rank
                && mv.from.values[0] as usize != dr
            {
                return false;
            }

            true
        })
        .collect();

    match candidates.len() {
        0 => Err(SanError::NoMatchingMove(san.to_string())),
        1 => Ok(candidates[0].clone()),
        _ => Err(SanError::AmbiguousMove(san.to_string())),
    }
}

fn find_castling_move(
    legal_moves: &[Move],
    player: Player,
    queenside: bool,
    san: &str,
) -> Result<Move, SanError> {
    let king_from_file = 4u8;
    let king_to_file = if queenside { 2u8 } else { 6u8 };
    let king_rank = match player {
        Player::White => 0u8,
        Player::Black => 7u8,
    };

    for mv in legal_moves {
        if mv.from.values[0] == king_rank
            && mv.from.values[1] == king_from_file
            && mv.to.values[0] == king_rank
            && mv.to.values[1] == king_to_file
        {
            return Ok(mv.clone());
        }
    }
    Err(SanError::NoMatchingMove(san.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::Board;

    #[test]
    fn test_parse_pawn_move() {
        let mut board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mv = parse_san(&mut board, Player::White, "e4").unwrap();
        assert_eq!(mv.from.values[1], 4); // e-file
        assert_eq!(mv.from.values[0], 1); // rank 2
        assert_eq!(mv.to.values[1], 4); // e-file
        assert_eq!(mv.to.values[0], 3); // rank 4
    }

    #[test]
    fn test_parse_knight_move() {
        let mut board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let mv = parse_san(&mut board, Player::White, "Nf3").unwrap();
        assert_eq!(mv.from.values[1], 6); // g-file
        assert_eq!(mv.to.values[1], 5); // f-file
        assert_eq!(mv.to.values[0], 2); // rank 3
    }

    #[test]
    fn test_parse_capture() {
        // Position where pawn can capture
        let mut board =
            Board::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2")
                .unwrap();
        let mv = parse_san(&mut board, Player::White, "exd5").unwrap();
        assert_eq!(mv.from.values[1], 4); // e-file
        assert_eq!(mv.to.values[1], 3); // d-file
        assert_eq!(mv.to.values[0], 4); // rank 5
    }

    #[test]
    fn test_parse_castling_kingside() {
        let mut board =
            Board::from_fen("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1").unwrap();
        let mv = parse_san(&mut board, Player::White, "O-O").unwrap();
        assert_eq!(mv.from.values[1], 4); // king from e1
        assert_eq!(mv.to.values[1], 6); // king to g1
    }

    #[test]
    fn test_parse_castling_queenside() {
        let mut board =
            Board::from_fen("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1").unwrap();
        let mv = parse_san(&mut board, Player::White, "O-O-O").unwrap();
        assert_eq!(mv.from.values[1], 4); // king from e1
        assert_eq!(mv.to.values[1], 2); // king to c1
    }

    #[test]
    fn test_parse_promotion() {
        let mut board = Board::from_fen("8/4P3/8/8/8/8/8/4K2k w - - 0 1").unwrap();
        let mv = parse_san(&mut board, Player::White, "e8=Q").unwrap();
        assert_eq!(mv.to.values[0], 7); // rank 8
        assert_eq!(mv.promotion, Some(PieceType::Queen));
    }

    #[test]
    fn test_parse_promotion_without_equals() {
        let mut board = Board::from_fen("8/4P3/8/8/8/8/8/4K2k w - - 0 1").unwrap();
        let mv = parse_san(&mut board, Player::White, "e8Q").unwrap();
        assert_eq!(mv.promotion, Some(PieceType::Queen));
    }

    #[test]
    fn test_parse_with_check_annotation() {
        let mut board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        // Nf3 with check annotation should still parse
        let mv = parse_san(&mut board, Player::White, "Nf3+");
        assert!(mv.is_ok());
    }

    #[test]
    fn test_no_matching_move() {
        let mut board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        assert!(parse_san(&mut board, Player::White, "Qd4").is_err());
    }
}
