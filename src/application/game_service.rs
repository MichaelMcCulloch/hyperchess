use crate::domain::models::{Board, BoardState, GameResult, Player};
use crate::domain::services::PlayerStrategy;
use std::fmt::Display;

pub struct GameService<'a, S: BoardState> {
    board: Board<S>,
    player_x: Box<dyn PlayerStrategy<S> + 'a>, // Boxing traits requires lifetime if they capture env?
    player_o: Box<dyn PlayerStrategy<S> + 'a>,
    turn: Player,
}

impl<'a, S: BoardState + Display> GameService<'a, S> {
    pub fn new(
        board: Board<S>,
        player_x: Box<dyn PlayerStrategy<S> + 'a>,
        player_o: Box<dyn PlayerStrategy<S> + 'a>,
    ) -> Self {
        GameService {
            board,
            player_x,
            player_o,
            turn: Player::X,
        }
    }

    pub fn board(&self) -> &Board<S> {
        &self.board
    }

    pub fn turn(&self) -> Player {
        self.turn
    }

    pub fn is_game_over(&self) -> Option<GameResult> {
        match self.board.check_status() {
            GameResult::InProgress => None,
            result => Some(result),
        }
    }

    pub fn perform_next_move(&mut self) -> Result<(), String> {
        if self.is_game_over().is_some() {
            return Err("Game is over".to_string());
        }

        let strategy = match self.turn {
            Player::X => &mut self.player_x,
            Player::O => &mut self.player_o,
        };

        if let Some(coord) = strategy.get_best_move(self.board.state(), self.turn) {
            self.board.make_move(coord, self.turn)?;
            self.turn = self.turn.opponent();
            Ok(())
        } else {
            Err("No move available".to_string())
        }
    }
}
