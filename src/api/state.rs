use crate::domain::game::Game;

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::domain::services::PlayerStrategy;

pub struct GameSession {
    pub game: Game,

    pub white_bot: Option<Box<dyn PlayerStrategy + Send + Sync>>,
    pub black_bot: Option<Box<dyn PlayerStrategy + Send + Sync>>,
}

pub type GameStore = Arc<DashMap<String, Arc<RwLock<GameSession>>>>;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    /// In-memory game store (used in standalone mode, or as local bot cache in gateway mode).
    pub games: GameStore,
    pub config: AppConfig,

    /// Redis session store (gateway mode only).
    #[cfg(feature = "distributed")]
    pub redis: Option<Arc<super::redis_store::RedisSessionStore>>,

    /// Distributed search coordinator (gateway mode only).
    #[cfg(feature = "distributed")]
    pub coordinator:
        Option<Arc<crate::infrastructure::distributed::coordinator::DistributedSearch>>,
}
