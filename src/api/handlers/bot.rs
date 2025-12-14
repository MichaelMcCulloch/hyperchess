use std::sync::Arc;
use tokio::time::{Duration, sleep};

use crate::api::state::GameSession;
use crate::domain::models::{GameResult, Player};
use crate::domain::services::PlayerStrategy;

pub async fn trigger_bot_move(session_arc: Arc<tokio::sync::RwLock<GameSession>>) {
    loop {
        sleep(Duration::from_millis(500)).await;

        let session = session_arc.write().await;

        if session.game.status() != GameResult::InProgress {
            break;
        }

        let current = session.game.current_turn();

        let is_bot_turn = match current {
            Player::White => session.white_bot.is_some(),
            Player::Black => session.black_bot.is_some(),
        };

        if !is_bot_turn {
            break;
        }

        let board_clone = session.game.board().clone();

        drop(session);

        let mut session = session_arc.write().await;

        if session.game.status() != GameResult::InProgress {
            break;
        }
        let current_now = session.game.current_turn();
        if current_now != current {
            break;
        }

        let best_move_opt = match current {
            Player::White => {
                if let Some(bot) = &mut session.white_bot {
                    bot.get_move(&board_clone, current)
                } else {
                    None
                }
            }
            Player::Black => {
                if let Some(bot) = &mut session.black_bot {
                    bot.get_move(&board_clone, current)
                } else {
                    None
                }
            }
        };

        if let Some(mv) = best_move_opt {
            let _ = session.game.play_turn(mv);
        } else {
            break;
        }

        let next = session.game.current_turn();
        let next_is_bot = match next {
            Player::White => session.white_bot.is_some(),
            Player::Black => session.black_bot.is_some(),
        };

        if session.game.status() != GameResult::InProgress || !next_is_bot {
            break;
        }
    }
}
