use crate::domain::models::{Board, BoardState, GameResult, Move, Player};

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
    move_history: Vec<(Player, Move)>,
}

impl<S: BoardState> Game<S> {
    pub fn new(board: Board<S>) -> Self {
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

        self.board
            .apply_move(&mv) // Updated for Board::apply_move taking &Move
            .map_err(GameError::InvalidMove)?;

        self.move_history.push((self.turn, mv));

        let result = self.board.check_status(self.turn); // Pass turn
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

    pub fn state(&self) -> &S {
        self.board.state()
    }
}
