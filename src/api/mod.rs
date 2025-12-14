pub mod handlers;
pub mod models;
pub mod routes;
pub mod state;

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::AppConfig;
use dashmap::DashMap;

pub async fn start_server() {
    let config = AppConfig::load();
    let games = Arc::new(DashMap::new());

    let app_state = state::AppState { games, config };

    let app = routes::app_router(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
