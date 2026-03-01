use std::fmt;

use crate::domain::board::Board;
use crate::domain::models::{Piece, PieceType, Player};

#[derive(Debug)]
pub enum FenError {
    InvalidFieldCount,
    InvalidRankCount { expected: usize, got: usize },
    InvalidPiece(char),
    InvalidSideToMove(String),
    InvalidCastling(char),
    InvalidEnPassant(String),
    InvalidHalfmoveClock(String),
    InvalidFullmoveNumber(String),
    RankOverflow { rank: usize, file: usize },
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFieldCount => write!(f, "FEN must have exactly 6 space-separated fields"),
            Self::InvalidRankCount { expected, got } => {
                write!(f, "Expected {expected} ranks, got {got}")
            }
            Self::InvalidPiece(c) => write!(f, "Invalid piece character: '{c}'"),
            Self::InvalidSideToMove(s) => write!(f, "Invalid side to move: '{s}'"),
            Self::InvalidCastling(c) => write!(f, "Invalid castling character: '{c}'"),
            Self::InvalidEnPassant(s) => write!(f, "Invalid en passant square: '{s}'"),
            Self::InvalidHalfmoveClock(s) => write!(f, "Invalid halfmove clock: '{s}'"),
            Self::InvalidFullmoveNumber(s) => write!(f, "Invalid fullmove number: '{s}'"),
            Self::RankOverflow { rank, file } => {
                write!(f, "Rank {rank} overflows at file {file}")
            }
        }
    }
}

impl std::error::Error for FenError {}

fn char_to_piece(c: char) -> Result<Piece, FenError> {
    let owner = if c.is_ascii_uppercase() {
        Player::White
    } else {
        Player::Black
    };
    let piece_type = match c.to_ascii_lowercase() {
        'p' => PieceType::Pawn,
        'n' => PieceType::Knight,
        'b' => PieceType::Bishop,
        'r' => PieceType::Rook,
        'q' => PieceType::Queen,
        'k' => PieceType::King,
        _ => return Err(FenError::InvalidPiece(c)),
    };
    Ok(Piece { piece_type, owner })
}

fn parse_castling(s: &str) -> Result<u8, FenError> {
    if s == "-" {
        return Ok(0);
    }
    let mut rights = 0u8;
    for c in s.chars() {
        match c {
            'K' => rights |= 0x1,
            'Q' => rights |= 0x2,
            'k' => rights |= 0x4,
            'q' => rights |= 0x8,
            _ => return Err(FenError::InvalidCastling(c)),
        }
    }
    Ok(rights)
}

fn parse_en_passant(s: &str) -> Result<Option<(usize, usize)>, FenError> {
    if s == "-" {
        return Ok(None);
    }
    let bytes = s.as_bytes();
    if bytes.len() != 2 {
        return Err(FenError::InvalidEnPassant(s.to_string()));
    }
    let file = bytes[0].wrapping_sub(b'a');
    let rank = bytes[1].wrapping_sub(b'1');
    if file >= 8 || rank >= 8 {
        return Err(FenError::InvalidEnPassant(s.to_string()));
    }

    // Coordinate order: [rank, file], index = rank + file * 8
    let target_idx = rank as usize + file as usize * 8;

    // victim_idx = the pawn that double-pushed
    // EP square on rank 2 (index 2) → victim is on rank 3 (white captured en passant)
    // EP square on rank 5 (index 5) → victim is on rank 4 (black captured en passant)
    let victim_rank = match rank {
        2 => 3u8,
        5 => 4u8,
        _ => return Err(FenError::InvalidEnPassant(s.to_string())),
    };
    let victim_idx = victim_rank as usize + file as usize * 8;

    Ok(Some((target_idx, victim_idx)))
}

impl Board {
    /// Parse a FEN string into a 2D 8×8 board.
    ///
    /// Only valid for standard chess positions (dimension=2, side=8).
    pub fn from_fen(fen: &str) -> Result<Board, FenError> {
        let fields: Vec<&str> = fen.split_whitespace().collect();
        if fields.len() != 6 {
            return Err(FenError::InvalidFieldCount);
        }

        let mut board = Board::new_empty(2, 8);

        // Field 1: piece placement (ranks 8→1, separated by '/')
        let ranks: Vec<&str> = fields[0].split('/').collect();
        if ranks.len() != 8 {
            return Err(FenError::InvalidRankCount {
                expected: 8,
                got: ranks.len(),
            });
        }

        for (fen_rank_idx, rank_str) in ranks.iter().enumerate() {
            // FEN rank 0 = rank 8 (top) → internal rank 7
            let internal_rank = 7 - fen_rank_idx;
            let mut file = 0usize;

            for c in rank_str.chars() {
                if let Some(skip) = c.to_digit(10) {
                    file += skip as usize;
                } else {
                    if file >= 8 {
                        return Err(FenError::RankOverflow {
                            rank: internal_rank,
                            file,
                        });
                    }
                    let piece = char_to_piece(c)?;
                    // Coordinate order: [rank, file], index = rank + file * 8
                    let idx = internal_rank + file * 8;
                    board.pieces.place_piece_at_index(idx, piece);
                    file += 1;
                }
            }
        }

        // Field 2: side to move
        let side_to_move = match fields[1] {
            "w" => Player::White,
            "b" => Player::Black,
            other => return Err(FenError::InvalidSideToMove(other.to_string())),
        };

        // Field 3: castling rights
        board.state.castling_rights = parse_castling(fields[2])?;

        // Field 4: en passant target
        board.state.en_passant_target = parse_en_passant(fields[3])?;

        // Field 5: halfmove clock
        board.state.halfmove_clock = fields[4]
            .parse::<u16>()
            .map_err(|_| FenError::InvalidHalfmoveClock(fields[4].to_string()))?;

        // Field 6: fullmove number
        board.state.fullmove_number = fields[5]
            .parse::<u16>()
            .map_err(|_| FenError::InvalidFullmoveNumber(fields[5].to_string()))?;

        // Compute Zobrist hash
        board.state.hash = board.zobrist.get_hash_with_player(
            &board.pieces,
            &board.state,
            board.geo.total_cells,
            side_to_move,
        );

        Ok(board)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{PieceType, Player};

    #[test]
    fn test_startpos() {
        let board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();

        // White pieces on rank 0
        let expected = Board::new(2, 8);
        for idx in 0..64 {
            assert_eq!(
                board.pieces.get_piece_at_index(idx),
                expected.pieces.get_piece_at_index(idx),
                "Mismatch at index {idx}"
            );
        }
        assert_eq!(board.state.castling_rights, 0xF);
        assert_eq!(board.state.en_passant_target, None);
        assert_eq!(board.state.halfmove_clock, 0);
        assert_eq!(board.state.fullmove_number, 1);
    }

    #[test]
    fn test_after_e4() {
        let board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1").unwrap();

        // e4 pawn at [3, 4] → index = 3 + 4*8 = 35
        let piece = board.pieces.get_piece_at_index(3 + 4 * 8).unwrap();
        assert_eq!(piece.piece_type, PieceType::Pawn);
        assert_eq!(piece.owner, Player::White);

        // e2 should be empty: [1, 4] → index = 1 + 4*8 = 33
        assert!(board.pieces.get_piece_at_index(1 + 4 * 8).is_none());

        // En passant target is e3 = [2, 4] → 2 + 4*8 = 34
        // victim is e4 = [3, 4] → 3 + 4*8 = 35
        assert_eq!(board.state.en_passant_target, Some((34, 35)));
    }

    #[test]
    fn test_partial_castling() {
        let board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w Kq - 0 1").unwrap();
        assert_eq!(board.state.castling_rights, 0x1 | 0x8); // WK + Bq
    }

    #[test]
    fn test_no_castling() {
        let board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1").unwrap();
        assert_eq!(board.state.castling_rights, 0);
    }

    #[test]
    fn test_kiwipete() {
        // A famous test position
        let board =
            Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1")
                .unwrap();

        // White knight on e5 = [4, 4] → index = 4 + 4*8 = 36
        let piece = board.pieces.get_piece_at_index(4 + 4 * 8).unwrap();
        assert_eq!(piece.piece_type, PieceType::Knight);
        assert_eq!(piece.owner, Player::White);

        // Black bishop on a6 = [5, 0] → index = 5 + 0*8 = 5
        let piece = board.pieces.get_piece_at_index(5).unwrap();
        assert_eq!(piece.piece_type, PieceType::Bishop);
        assert_eq!(piece.owner, Player::Black);
    }

    #[test]
    fn test_invalid_fen_field_count() {
        assert!(Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w").is_err());
    }

    #[test]
    fn test_invalid_piece_char() {
        assert!(
            Board::from_fen("xnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").is_err()
        );
    }
}
