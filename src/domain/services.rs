use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use std::time::Duration;

pub trait Clock {
    fn now(&self) -> Duration;
}

pub trait PlayerStrategy {
    fn get_move(&mut self, board: &Board, player: Player) -> Option<Move>;
}
