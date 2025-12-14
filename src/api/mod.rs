pub mod handlers;
pub mod models;
pub mod routes;
pub mod state;

use std::sync::Arc;

use crate::config::AppConfig;
use dashmap::DashMap;

pub async fn start_server() {
    let config = AppConfig::load();
    let games = Arc::new(DashMap::new());

    let app_state = state::AppState {
        games,
        config: config.clone(),
    };

    let app = routes::app_router(app_state);

    let port = config.api.port;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
