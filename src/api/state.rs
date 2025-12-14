use crate::domain::game::Game;
use crate::infrastructure::ai::minimax::MinimaxBot;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GameSession {
    pub game: Game,

    pub white_bot: Option<MinimaxBot>,
    pub black_bot: Option<MinimaxBot>,
}

pub type GameStore = Arc<DashMap<String, Arc<RwLock<GameSession>>>>;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub games: GameStore,
    pub config: AppConfig,
}
