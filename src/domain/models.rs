use crate::domain::coordinate::Coordinate;
use std::fmt::Debug;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Player {
    White,
    Black,
}

impl Player {
    pub fn opponent(&self) -> Self {
        match self {
            Player::White => Player::Black,
            Player::Black => Player::White,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PieceType {
    Pawn,
    Rook,
    Knight,
    Bishop,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Piece {
    pub piece_type: PieceType,
    pub owner: Player,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Move {
    pub from: Coordinate,
    pub to: Coordinate,
    pub promotion: Option<PieceType>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameResult {
    Checkmate(Player),
    Stalemate,
    Draw,
    InProgress,
}
