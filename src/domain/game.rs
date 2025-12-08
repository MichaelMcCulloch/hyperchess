use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Board, BoardState, GameResult, Player};

#[derive(Debug)]
pub enum GameError {
    InvalidMove(String),
}

/// The Game Aggregate Root.
/// It controls the lifecycle of the game, turns, and winning conditions.
pub struct Game<S: BoardState> {
    board: Board<S>,
    turn: Player,
    status: GameResult,
    move_history: Vec<(Player, Coordinate)>,
}

impl<S: BoardState> Game<S> {
    pub fn new(board: Board<S>) -> Self {
        Self {
            board,
            turn: Player::X,
            status: GameResult::InProgress,
            move_history: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        // Any initialization logic can go here
        self.status = GameResult::InProgress;
        self.turn = Player::X;
    }

    pub fn play_turn(&mut self, coord: Coordinate) -> Result<GameResult, GameError> {
        if self.status != GameResult::InProgress {
            return Err(GameError::InvalidMove("Game is already over".to_string()));
        }

        self.board
            .make_move(coord.clone(), self.turn)
            .map_err(GameError::InvalidMove)?;

        self.move_history.push((self.turn, coord));

        let result = self.board.check_status();
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

    pub fn board(&self) -> &Board<S> {
        &self.board
    }

    // Expose inner state for read-only if needed, or projection
    pub fn state(&self) -> &S {
        self.board.state()
    }
}
