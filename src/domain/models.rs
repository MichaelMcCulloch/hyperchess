use std::fmt::Debug;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Player {
    X,
    O,
}

impl Player {
    pub fn opponent(&self) -> Self {
        match self {
            Player::X => Player::O,
            Player::O => Player::X,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameResult {
    Win(Player),
    Draw,
    InProgress,
}

use crate::domain::coordinate::Coordinate;

/// Trait defining the storage and core mechanics of the board backend.
/// This allows us to strictly separate the "BitBoard" optimization (Infrastructure)
/// from the "Board" concept (Domain).
pub trait BoardState: Debug + Clone {
    fn new(dimension: usize) -> Self
    where
        Self: Sized;
    fn dimension(&self) -> usize;
    fn side(&self) -> usize;
    fn total_cells(&self) -> usize;
    fn get_cell(&self, coord: &Coordinate) -> Option<Player>;
    fn set_cell(&mut self, coord: &Coordinate, player: Player) -> Result<(), String>;
    fn clear_cell(&mut self, coord: &Coordinate);
    fn check_win(&self) -> Option<Player>;
    fn is_full(&self) -> bool;
}

/// The Domain Entity representing the Game Board.
/// It wraps a BoardState implementation.
#[derive(Clone, Debug)]
pub struct Board<S: BoardState> {
    state: S,
}

impl<S: BoardState> Board<S> {
    pub fn new(dimension: usize) -> Self {
        Self {
            state: S::new(dimension),
        }
    }

    pub fn dimension(&self) -> usize {
        self.state.dimension()
    }

    pub fn make_move(&mut self, coord: Coordinate, player: Player) -> Result<(), String> {
        self.state.set_cell(&coord, player)
    }

    pub fn get_cell(&self, coord: &Coordinate) -> Option<Player> {
        self.state.get_cell(coord)
    }

    pub fn check_status(&self) -> GameResult {
        if let Some(winner) = self.state.check_win() {
            return GameResult::Win(winner);
        }
        if self.state.is_full() {
            return GameResult::Draw;
        }
        GameResult::InProgress
    }

    pub fn state(&self) -> &S {
        &self.state
    }
}
