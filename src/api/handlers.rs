use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

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
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::minimax::MinimaxBot;

pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/new_game", post(create_game))
        .route("/game/:uuid", get(get_game))
        .route("/take_turn", post(take_turn))
        .with_state(state)
}

async fn create_game(
    State(state): State<AppState>,
    Json(payload): Json<NewGameRequest>,
) -> impl IntoResponse {
    let dimension = payload.dimension.unwrap_or(2);
    let side = payload.side.unwrap_or(8);

    let board = Board::new(dimension, side);
    let game = Game::new(board);
    let time_limit = (state.config.time.minutes * 60.0 * 1000.0) as u64;

    let mut white_bot = None;
    let mut black_bot = None;

    match payload.mode.to_lowercase().as_str() {
        "cc" => {
            white_bot = Some(
                MinimaxBot::new(&state.config.minimax, time_limit, dimension, side)
                    .with_mcts(state.config.mcts.clone()),
            );
            black_bot = Some(
                MinimaxBot::new(&state.config.minimax, time_limit, dimension, side)
                    .with_mcts(state.config.mcts.clone()),
            );
        }
        "hc" => {
            black_bot = Some(
                MinimaxBot::new(&state.config.minimax, time_limit, dimension, side)
                    .with_mcts(state.config.mcts.clone()),
            );
        }
        "ch" => {
            white_bot = Some(
                MinimaxBot::new(&state.config.minimax, time_limit, dimension, side)
                    .with_mcts(state.config.mcts.clone()),
            );
        }
        "hh" => {}
        _ => {
            return (StatusCode::BAD_REQUEST, "Invalid mode").into_response();
        }
    }

    let uuid = Uuid::new_v4().to_string();
    let session = GameSession {
        game,
        white_bot,
        black_bot,
    };

    state
        .games
        .insert(uuid.clone(), Arc::new(tokio::sync::RwLock::new(session)));

    (StatusCode::CREATED, Json(NewGameResponse { uuid })).into_response()
}

async fn get_game(State(state): State<AppState>, Path(uuid): Path<String>) -> impl IntoResponse {
    if let Some(session_arc) = state.games.get(&uuid) {
        let session = session_arc.read().await;
        let response = build_api_state(&session.game);
        (StatusCode::OK, Json(response)).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Game not found").into_response()
    }
}

async fn take_turn(
    State(state): State<AppState>,
    Json(payload): Json<TurnRequest>,
) -> impl IntoResponse {
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

    let coord_start = Coordinate::new(payload.start);
    let coord_end = Coordinate::new(payload.end);

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

    let response_state = build_api_state(&session.game);
    let game_status = session.game.status();

    let next_player = session.game.current_turn();
    let next_is_bot = match next_player {
        Player::White => session.white_bot.is_some(),
        Player::Black => session.black_bot.is_some(),
    };

    if next_is_bot && game_status == GameResult::InProgress {
        let session_clone = session_arc.clone();
        tokio::spawn(async move {
            {
                let mut session = session_clone.write().await;
                let current = session.game.current_turn();

                let is_bot_turn = match current {
                    Player::White => session.white_bot.is_some(),
                    Player::Black => session.black_bot.is_some(),
                };
                if !is_bot_turn || session.game.status() != GameResult::InProgress {
                    return;
                }

                let board_clone = session.game.board().clone();

                let best_move_opt = match current {
                    Player::White => session
                        .white_bot
                        .as_mut()
                        .unwrap()
                        .get_move(&board_clone, current),
                    Player::Black => session
                        .black_bot
                        .as_mut()
                        .unwrap()
                        .get_move(&board_clone, current),
                };

                if let Some(mv) = best_move_opt {
                    let _ = session.game.play_turn(mv);
                }
            }
        });
    }

    (StatusCode::OK, Json(response_state)).into_response()
}

fn build_api_state(game: &Game) -> ApiGameState {
    let board = game.board();
    let pieces = board
        .white_occupancy
        .iter_indices()
        .chain(board.black_occupancy.iter_indices())
        .map(|idx| {
            let p = board.get_piece_at_index(idx).unwrap();
            let coords = board.index_to_coords(idx).into_vec();
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

            if opp_moves.is_empty() {
                if let Some(k_pos) = temp_board.get_king_coordinate(opponent) {
                    if Rules::is_square_attacked(&temp_board, &k_pos, current_player) {
                        consequence = MoveConsequence::Victory;
                    }
                }
            }

            temp_board.unmake_move(&mv, info);
        }

        let valid_move = ApiValidMove {
            to: mv.to.values.into_vec(),
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
    }
}
