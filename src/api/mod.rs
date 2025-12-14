pub mod handlers;
pub mod models;
pub mod state;

pub use handlers::app_router;

use crate::api::state::AppState;
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

use crate::config::AppConfig;

pub async fn start_server() {
    let config = AppConfig::load();
    let state = AppState {
        games: Arc::new(DashMap::new()),
        config,
    };

    let cors = CorsLayer::permissive();

    let app = app_router(state).layer(cors);
    let addr = SocketAddr::from(([127, 0, 0, 1], 3123));
    println!("Listening on {}", addr);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
