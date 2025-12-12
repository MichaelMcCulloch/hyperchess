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

/// Trait defining the storage and core mechanics of the board backend.
pub trait BoardState: Debug + Clone {
    fn new(dimension: usize, side: usize) -> Self
    where
        Self: Sized;
    fn dimension(&self) -> usize;
    fn side(&self) -> usize;
    fn total_cells(&self) -> usize;

    // Core Piece Access
    fn get_piece(&self, coord: &Coordinate) -> Option<Piece>;

    // Core Movement Logic
    fn apply_move(&mut self, mv: &Move) -> Result<(), String>;

    // State Queries
    fn get_king_coordinate(&self, player: Player) -> Option<Coordinate>;

    fn set_piece(&mut self, coord: &Coordinate, piece: Piece) -> Result<(), String>;
    fn clear_cell(&mut self, coord: &Coordinate);

    // Game Status
    fn check_status(&self, player_to_move: Player) -> GameResult;
}

/// The Domain Entity representing the Game Board.
#[derive(Clone, Debug)]
pub struct Board<S: BoardState> {
    state: S,
}

impl<S: BoardState> Board<S> {
    pub fn new(dimension: usize, side: usize) -> Self {
        Self {
            state: S::new(dimension, side),
        }
    }

    pub fn dimension(&self) -> usize {
        self.state.dimension()
    }

    pub fn side(&self) -> usize {
        self.state.side()
    }

    pub fn total_cells(&self) -> usize {
        self.state.total_cells()
    }

    pub fn apply_move(&mut self, mv: &Move) -> Result<(), String> {
        self.state.apply_move(mv)
    }

    pub fn get_piece(&self, coord: &Coordinate) -> Option<Piece> {
        self.state.get_piece(coord)
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    pub fn get_king_coordinate(&self, player: Player) -> Option<Coordinate> {
        self.state.get_king_coordinate(player)
    }

    pub fn check_status(&self, player: Player) -> GameResult {
        self.state.check_status(player)
    }
}
