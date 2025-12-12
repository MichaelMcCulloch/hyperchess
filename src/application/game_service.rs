use crate::domain::board::Board;
use crate::domain::models::{GameResult, Player};
use crate::domain::services::PlayerStrategy;

pub struct GameService<'a> {
    board: Board,
    player_white: Box<dyn PlayerStrategy + 'a>,
    player_black: Box<dyn PlayerStrategy + 'a>,
    turn: Player,
}

impl<'a> GameService<'a> {
    pub fn new(
        board: Board,
        player_white: Box<dyn PlayerStrategy + 'a>,
        player_black: Box<dyn PlayerStrategy + 'a>,
    ) -> Self {
        GameService {
            board,
            player_white,
            player_black,
            turn: Player::White,
        }
    }

    pub fn board(&self) -> &Board {
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

        if let Some(mv) = strategy.get_move(&self.board, self.turn) {
            self.board.apply_move(&mv).map_err(|e| e.to_string())?;

            self.turn = self.turn.opponent();

            Ok(self.board.check_status(self.turn))
        } else {
            Err("No move available".to_string())
        }
    }
}
