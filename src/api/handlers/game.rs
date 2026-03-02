use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::handlers::bot::trigger_bot_move;
use crate::api::models::{
    ApiGameState, ApiPiece, ApiValidMove, MoveConsequence, NewGameRequest, NewGameResponse,
    TurnRequest,
};
use crate::api::state::{AppState, GameSession};
use crate::domain::board::Board;
use crate::domain::coordinate::Coordinate;
use crate::domain::game::Game;
use crate::domain::models::{GameResult, PieceType, Player};
use crate::domain::rules::Rules;
use crate::infrastructure::ai::MinimaxBot;

pub async fn create_game(
    State(state): State<AppState>,
    Json(payload): Json<NewGameRequest>,
) -> impl IntoResponse {
    let dimension = payload.dimension.unwrap_or(2);
    let side = payload.side.unwrap_or(8);

    let board = Board::new(dimension, side);
    let game = Game::new(board);

    let uuid = Uuid::new_v4().to_string();

    // Determine bot config
    let (has_white_bot, has_black_bot) = match payload.mode.to_lowercase().as_str() {
        "cc" => (true, true),
        "hc" => (false, true),
        "ch" => (true, false),
        "hh" => (false, false),
        _ => {
            return (StatusCode::BAD_REQUEST, "Invalid mode").into_response();
        }
    };

    #[cfg(feature = "distributed")]
    if let Some(redis) = &state.redis {
        // Distributed / gateway mode: store session in Redis
        use crate::api::redis_store::{BotConfig, RedisSession};

        let session = RedisSession {
            game,
            white_bot_config: if has_white_bot {
                Some(BotConfig { dimension, side })
            } else {
                None
            },
            black_bot_config: if has_black_bot {
                Some(BotConfig { dimension, side })
            } else {
                None
            },
        };

        if let Err(e) = redis.save_session(&uuid, &session).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Redis error: {}", e),
            )
                .into_response();
        }

        // Trigger bot move if it's a bot's turn
        if session.is_bot_turn()
            && let Some(coordinator) = &state.coordinator
        {
            let uuid_clone = uuid.clone();
            let redis_clone = redis.clone();
            let coord_clone = coordinator.clone();
            tokio::spawn(async move {
                trigger_bot_move_distributed(uuid_clone, redis_clone, coord_clone).await;
            });
        }

        return (StatusCode::CREATED, Json(NewGameResponse { uuid })).into_response();
    }

    // Standalone mode: store in-memory
    let create_bot =
        |config: &crate::config::AppConfig| -> Box<dyn crate::domain::services::PlayerStrategy + Send + Sync> {
            Box::new(MinimaxBot::new(config, dimension, side))
        };

    let white_bot = if has_white_bot {
        Some(create_bot(&state.config))
    } else {
        None
    };
    let black_bot = if has_black_bot {
        Some(create_bot(&state.config))
    } else {
        None
    };

    let session = GameSession {
        game,
        white_bot,
        black_bot,
    };

    state
        .games
        .insert(uuid.clone(), Arc::new(tokio::sync::RwLock::new(session)));

    let session_arc = state.games.get(&uuid).unwrap().clone();
    tokio::spawn(async move {
        trigger_bot_move(session_arc).await;
    });

    (StatusCode::CREATED, Json(NewGameResponse { uuid })).into_response()
}

pub async fn get_game(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> impl IntoResponse {
    #[cfg(feature = "distributed")]
    if let Some(redis) = &state.redis {
        return match redis.get_session(&uuid).await {
            Ok(Some(session)) => {
                let response = build_api_state_from_game(&session.game);
                (StatusCode::OK, Json(response)).into_response()
            }
            Ok(None) => (StatusCode::NOT_FOUND, "Game not found").into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Redis error: {}", e),
            )
                .into_response(),
        };
    }

    // Standalone mode
    if let Some(session_arc) = state.games.get(&uuid) {
        let session = session_arc.read().await;
        let response = build_api_state_from_game(&session.game);
        (StatusCode::OK, Json(response)).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Game not found").into_response()
    }
}

pub async fn take_turn(
    State(state): State<AppState>,
    Json(payload): Json<TurnRequest>,
) -> impl IntoResponse {
    #[cfg(feature = "distributed")]
    if let Some(redis) = &state.redis {
        return take_turn_distributed(state.clone(), redis.clone(), payload).await;
    }

    // Standalone mode
    take_turn_standalone(state, payload).await
}

async fn take_turn_standalone(state: AppState, payload: TurnRequest) -> axum::response::Response {
    let session_arc = if let Some(s) = state.games.get(&payload.uuid) {
        s.clone()
    } else {
        return (StatusCode::NOT_FOUND, "Game not found").into_response();
    };

    let mut session = session_arc.write().await;

    let current_player = session.game.current_turn();

    let is_bot = match current_player {
        Player::White => session.white_bot.is_some(),
        Player::Black => session.black_bot.is_some(),
    };

    if is_bot {
        return (StatusCode::FORBIDDEN, "Not human turn").into_response();
    }

    let coord_start = Coordinate::new(payload.start.iter().map(|&x| x as u8).collect::<Vec<u8>>());
    let coord_end = Coordinate::new(payload.end.iter().map(|&x| x as u8).collect::<Vec<u8>>());

    let mut chosen_move = None;
    let mut temp_board_valid = session.game.board().clone();
    let move_candidates = Rules::generate_legal_moves(&mut temp_board_valid, current_player);
    for mv in move_candidates {
        if mv.from == coord_start && mv.to == coord_end {
            if let Some(p) = mv.promotion {
                if p == PieceType::Queen {
                    chosen_move = Some(mv);
                    break;
                }
            } else {
                chosen_move = Some(mv);
                break;
            }
        }
    }

    let mv_to_play = match chosen_move {
        Some(m) => m,
        None => return (StatusCode::BAD_REQUEST, "Invalid move").into_response(),
    };

    let result = session.game.play_turn(mv_to_play);

    if let Err(e) = result {
        return (StatusCode::BAD_REQUEST, format!("Move failed: {:?}", e)).into_response();
    }

    let response_state = build_api_state_from_game(&session.game);
    let game_status = session.game.status();

    let next_player = session.game.current_turn();
    let next_is_bot = match next_player {
        Player::White => session.white_bot.is_some(),
        Player::Black => session.black_bot.is_some(),
    };

    if next_is_bot && game_status == GameResult::InProgress {
        let session_clone = session_arc.clone();
        tokio::spawn(async move {
            trigger_bot_move(session_clone).await;
        });
    }

    (StatusCode::OK, Json(response_state)).into_response()
}

#[cfg(feature = "distributed")]
async fn take_turn_distributed(
    state: AppState,
    redis: Arc<crate::api::redis_store::RedisSessionStore>,
    payload: TurnRequest,
) -> axum::response::Response {
    let uuid = &payload.uuid;

    // Acquire lock
    let holder = format!(
        "{}:{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string()),
        std::process::id()
    );

    for _ in 0..3 {
        if let Ok(true) = redis.acquire_lock(uuid, &holder).await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let mut session = match redis.get_session(uuid).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            let _ = redis.release_lock(uuid, &holder).await;
            return (StatusCode::NOT_FOUND, "Game not found").into_response();
        }
        Err(e) => {
            let _ = redis.release_lock(uuid, &holder).await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Redis error: {}", e),
            )
                .into_response();
        }
    };

    let current_player = session.game.current_turn();
    let is_bot = match current_player {
        Player::White => session.white_bot_config.is_some(),
        Player::Black => session.black_bot_config.is_some(),
    };

    if is_bot {
        let _ = redis.release_lock(uuid, &holder).await;
        return (StatusCode::FORBIDDEN, "Not human turn").into_response();
    }

    let coord_start = Coordinate::new(payload.start.iter().map(|&x| x as u8).collect::<Vec<u8>>());
    let coord_end = Coordinate::new(payload.end.iter().map(|&x| x as u8).collect::<Vec<u8>>());

    let mut chosen_move = None;
    let mut temp_board = session.game.board().clone();
    let move_candidates = Rules::generate_legal_moves(&mut temp_board, current_player);
    for mv in move_candidates {
        if mv.from == coord_start && mv.to == coord_end {
            if let Some(p) = mv.promotion {
                if p == PieceType::Queen {
                    chosen_move = Some(mv);
                    break;
                }
            } else {
                chosen_move = Some(mv);
                break;
            }
        }
    }

    let mv_to_play = match chosen_move {
        Some(m) => m,
        None => {
            let _ = redis.release_lock(uuid, &holder).await;
            return (StatusCode::BAD_REQUEST, "Invalid move").into_response();
        }
    };

    let result = session.game.play_turn(mv_to_play);
    if let Err(e) = result {
        let _ = redis.release_lock(uuid, &holder).await;
        return (StatusCode::BAD_REQUEST, format!("Move failed: {:?}", e)).into_response();
    }

    let response_state = build_api_state_from_game(&session.game);

    // Save updated session
    if let Err(e) = redis.save_session(uuid, &session).await {
        let _ = redis.release_lock(uuid, &holder).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Redis save error: {}", e),
        )
            .into_response();
    }

    let _ = redis.release_lock(uuid, &holder).await;

    // Trigger bot move if next player is bot
    if session.is_bot_turn()
        && session.game.status() == GameResult::InProgress
        && let Some(coordinator) = &state.coordinator
    {
        let uuid_clone = uuid.clone();
        let redis_clone = redis.clone();
        let coord_clone = coordinator.clone();
        tokio::spawn(async move {
            trigger_bot_move_distributed(uuid_clone, redis_clone, coord_clone).await;
        });
    }

    (StatusCode::OK, Json(response_state)).into_response()
}

/// Trigger bot moves in distributed mode using the coordinator.
#[cfg(feature = "distributed")]
async fn trigger_bot_move_distributed(
    uuid: String,
    redis: Arc<crate::api::redis_store::RedisSessionStore>,
    coordinator: Arc<crate::infrastructure::distributed::coordinator::DistributedSearch>,
) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Acquire lock
        let holder = format!(
            "{}:bot:{}",
            std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string()),
            std::process::id()
        );

        let locked = redis.acquire_lock(&uuid, &holder).await.unwrap_or(false);
        if !locked {
            // Another pod is handling this game
            eprintln!("[gateway] Could not acquire lock for {}, skipping", uuid);
            break;
        }

        let session = match redis.get_session(&uuid).await {
            Ok(Some(s)) => s,
            _ => {
                let _ = redis.release_lock(&uuid, &holder).await;
                break;
            }
        };

        if session.game.status() != GameResult::InProgress {
            let _ = redis.release_lock(&uuid, &holder).await;
            break;
        }

        if !session.is_bot_turn() {
            let _ = redis.release_lock(&uuid, &holder).await;
            break;
        }

        let board = session.game.board().clone();
        let player = session.game.current_turn();

        // Release lock during search (search takes a long time)
        let _ = redis.release_lock(&uuid, &holder).await;

        // Execute distributed search
        let best_move = coordinator.search(&board, player).await;

        // Re-acquire lock to apply move
        let locked = redis.acquire_lock(&uuid, &holder).await.unwrap_or(false);
        if !locked {
            break;
        }

        let mut session = match redis.get_session(&uuid).await {
            Ok(Some(s)) => s,
            _ => {
                let _ = redis.release_lock(&uuid, &holder).await;
                break;
            }
        };

        // Verify state hasn't changed
        if session.game.status() != GameResult::InProgress || session.game.current_turn() != player
        {
            let _ = redis.release_lock(&uuid, &holder).await;
            break;
        }

        if let Some(mv) = best_move {
            let _ = session.game.play_turn(mv);
            let _ = redis.save_session(&uuid, &session).await;
        }

        let _ = redis.release_lock(&uuid, &holder).await;

        // Continue if next player is also a bot
        if !session.is_bot_turn() || session.game.status() != GameResult::InProgress {
            break;
        }
    }
}

fn build_api_state_from_game(game: &Game) -> ApiGameState {
    let board = game.board();
    let pieces = board
        .pieces
        .white_occupancy
        .iter_indices()
        .chain(board.pieces.black_occupancy.iter_indices())
        .map(|idx| {
            let p = board.get_piece_at_index(idx).unwrap();
            let coords = board
                .index_to_coords(idx)
                .iter()
                .map(|&x| x as usize)
                .collect();
            ApiPiece {
                piece_type: p.piece_type,
                owner: p.owner,
                coordinate: coords,
            }
        })
        .collect();

    let current_player = game.current_turn();

    let mut temp_board = board.clone();
    let moves = Rules::generate_legal_moves(&mut temp_board, current_player);

    let mut valid_moves_map: HashMap<String, Vec<ApiValidMove>> = HashMap::new();

    for mv in moves {
        let from_str = format!("{:?}", mv.from);

        let mut consequence = MoveConsequence::NoEffect;
        let dest_piece = board.get_piece(&mv.to);
        if dest_piece.is_some() {
            consequence = MoveConsequence::Capture;
        }

        if let Ok(info) = temp_board.apply_move(&mv) {
            if info.captured.is_some() {
                consequence = MoveConsequence::Capture;
            }

            let opponent = current_player.opponent();

            let opp_moves = Rules::generate_legal_moves(&mut temp_board, opponent);

            if opp_moves.is_empty()
                && let Some(k_pos) = temp_board.get_king_coordinate(opponent)
                && Rules::is_square_attacked(&temp_board, &k_pos, current_player)
            {
                consequence = MoveConsequence::Victory;
            }

            temp_board.unmake_move(&mv, info);
        }

        let valid_move = ApiValidMove {
            to: mv.to.values.iter().map(|&x| x as usize).collect(),
            consequence,
        };

        valid_moves_map
            .entry(from_str)
            .or_default()
            .push(valid_move);
    }

    ApiGameState {
        pieces,
        current_player,
        valid_moves: valid_moves_map,
        status: game.status(),
        dimension: board.dimension(),
        side: board.side(),
        in_check: false,
        sequence: game.move_history().len(),
    }
}
