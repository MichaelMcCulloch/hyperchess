use axum::{
    Router,
    routing::{get, post},
};

use crate::api::handlers::game::{create_game, get_game, take_turn};
use crate::api::state::AppState;

pub fn app_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/new_game", post(create_game))
        .route("/game/:uuid", get(get_game))
        .route("/take_turn", post(take_turn));

    Router::new().nest("/api/v1", api_routes).with_state(state)
}
