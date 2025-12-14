use crate::domain::board::Board;
use crate::domain::models::{GameResult, Move, Player};

#[derive(Debug)]
pub enum GameError {
    InvalidMove(String),
}

pub struct Game {
    board: Board,
    turn: Player,
    status: GameResult,
    move_history: Vec<(Player, Move)>,
}

impl Game {
    pub fn new(board: Board) -> Self {
        Self {
            board,
            turn: Player::White,
            status: GameResult::InProgress,
            move_history: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.status = GameResult::InProgress;
        self.turn = Player::White;
    }

    pub fn play_turn(&mut self, mv: Move) -> Result<GameResult, GameError> {
        if self.status != GameResult::InProgress {
            return Err(GameError::InvalidMove("Game is already over".to_string()));
        }

        self.board.apply_move(&mv).map_err(GameError::InvalidMove)?;

        self.move_history.push((self.turn, mv.clone()));

        let result = self.board.check_status(self.turn);
        self.status = result;

        if result == GameResult::InProgress {
            self.turn = self.turn.opponent();
        }

        Ok(result)
    }

    pub fn current_turn(&self) -> Player {
        self.turn
    }

    pub fn status(&self) -> GameResult {
        self.status
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn move_history(&self) -> &Vec<(Player, Move)> {
        &self.move_history
    }
}
