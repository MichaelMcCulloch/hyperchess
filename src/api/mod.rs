pub mod handlers;
pub mod models;
pub mod routes;
pub mod state;

#[cfg(feature = "distributed")]
pub mod redis_store;

use std::sync::Arc;

use crate::config::AppConfig;
use dashmap::DashMap;

pub async fn start_server() {
    let config = AppConfig::load();
    start_server_with_config(config).await;
}

pub async fn start_server_with_config(config: AppConfig) {
    #[cfg(feature = "distributed")]
    {
        if config.distributed.enabled && config.distributed.mode == "worker" {
            // Worker mode: only run gRPC search service
            crate::infrastructure::distributed::worker::start_grpc_worker(config).await;
            return;
        }
    }

    // Gateway / standalone mode: run HTTP API
    let games = Arc::new(DashMap::new());

    #[cfg(feature = "distributed")]
    let redis = if config.distributed.enabled {
        Some(Arc::new(
            redis_store::RedisSessionStore::new(&config.distributed.redis_url).await,
        ))
    } else {
        None
    };

    #[cfg(feature = "distributed")]
    let coordinator = if config.distributed.enabled {
        Some(Arc::new(
            crate::infrastructure::distributed::coordinator::DistributedSearch::new(&config),
        ))
    } else {
        None
    };

    let app_state = state::AppState {
        games,
        config: config.clone(),
        #[cfg(feature = "distributed")]
        redis,
        #[cfg(feature = "distributed")]
        coordinator,
    };

    let app = routes::app_router(app_state);

    let port = config.api.port;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
