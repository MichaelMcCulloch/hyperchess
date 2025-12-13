use crate::domain::game::Game;
use crate::infrastructure::ai::minimax::MinimaxBot;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GameSession {
    pub game: Game,
    // Bots are not Clone/Send/Sync automatically if they use raw pointers or Rc, but MinimaxBot uses Arc/Atomic.
    // However PlayerStrategy trait uses &mut self. So we need to lock the bot if we want to use it.
    // Putting them in RwLock-ed session handles that.
    // But wait, MinimaxBot is in infrastructure/ai/minimax.rs.
    pub white_bot: Option<MinimaxBot>,
    pub black_bot: Option<MinimaxBot>,
}

pub type GameStore = Arc<DashMap<String, Arc<RwLock<GameSession>>>>;

#[derive(Clone)]
pub struct AppState {
    pub games: GameStore,
}
