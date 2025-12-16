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
use crate::infrastructure::ai::{MctsBot, MinimaxBot};

pub async fn create_game(
    State(state): State<AppState>,
    Json(payload): Json<NewGameRequest>,
) -> impl IntoResponse {
    let dimension = payload.dimension.unwrap_or(2);
    let side = payload.side.unwrap_or(8);

    let board = Board::new(dimension, side);
    let game = Game::new(board);

    let mut white_bot = None;
    let mut black_bot = None;

    let create_bot =
        |config: &crate::config::AppConfig| -> Box<dyn crate::domain::services::PlayerStrategy + Send + Sync> {
            if config.mcts.is_some() {
                Box::new(MctsBot::new(config))
            } else {
                Box::new(
                    MinimaxBot::new(
                        &config,
                        dimension,
                        side,
                    )
                )
            }
        };

    match payload.mode.to_lowercase().as_str() {
        "cc" => {
            white_bot = Some(create_bot(&state.config));
            black_bot = Some(create_bot(&state.config));
        }
        "hc" => {
            black_bot = Some(create_bot(&state.config));
        }
        "ch" => {
            white_bot = Some(create_bot(&state.config));
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
    if let Some(session_arc) = state.games.get(&uuid) {
        let session = session_arc.read().await;
        let response = build_api_state(&session.game);
        (StatusCode::OK, Json(response)).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Game not found").into_response()
    }
}

pub async fn take_turn(
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
            trigger_bot_move(session_clone).await;
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
