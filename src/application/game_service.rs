use crate::domain::models::{Board, BoardState, GameResult, Player};
use crate::domain::services::PlayerStrategy;
use std::fmt::Display;

pub struct GameService<'a, S: BoardState> {
    board: Board<S>,
    player_white: Box<dyn PlayerStrategy<S> + 'a>,
    player_black: Box<dyn PlayerStrategy<S> + 'a>, // Assuming PlayerStrategy<S> is object safe or we use generic S?
    // Box<dyn PlayerStrategy<S>> is valid if PlayerStrategy is object safe.
    turn: Player,
}

impl<'a, S: BoardState + Display> GameService<'a, S> {
    pub fn new(
        board: Board<S>,
        player_white: Box<dyn PlayerStrategy<S> + 'a>,
        player_black: Box<dyn PlayerStrategy<S> + 'a>,
    ) -> Self {
        GameService {
            board,
            player_white,
            player_black,
            turn: Player::White,
        }
    }

    pub fn board(&self) -> &Board<S> {
        &self.board
    }

    pub fn turn(&self) -> Player {
        self.turn
    }

    pub fn is_game_over(&self) -> Option<GameResult> {
        match self.board.check_status(self.turn) {
            GameResult::InProgress => None,
            result => Some(result),
        }
    }

    pub fn perform_next_move(&mut self) -> Result<GameResult, String> {
        if self.is_game_over().is_some() {
            return Err("Game is over".to_string());
        }

        let strategy = match self.turn {
            Player::White => &mut self.player_white,
            Player::Black => &mut self.player_black,
        };

        // Assuming get_move returns Option<Move>
        if let Some(mv) = strategy.get_move(self.board.state(), self.turn) {
            self.board.apply_move(&mv).map_err(|e| e.to_string())?;

            self.turn = self.turn.opponent();

            Ok(self.board.check_status(self.turn))
        } else {
            Err("No move available".to_string())
        }
    }
}
