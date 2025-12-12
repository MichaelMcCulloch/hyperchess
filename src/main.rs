use hyperchess::application::game_service::GameService;
use hyperchess::domain::models::{Board, BoardState};
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;
use hyperchess::infrastructure::console::HumanConsolePlayer;
use hyperchess::infrastructure::persistence::BitBoardState;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dimension = 3;
    let side = 8; // Default side 8 for HyperChess
    let mut player_white_type = "h";
    let mut player_black_type = "c";
    let mut depth = 4;
    let time_limit = 1000; // ms

    if args.len() > 1 {
        if let Ok(d) = args[1].parse::<usize>() {
            dimension = d;
        }
    }
    if args.len() > 2 {
        let mode = args[2].as_str();
        if mode.len() >= 2 {
            player_white_type = &mode[0..1];
            player_black_type = &mode[1..2];
        }
    }
    if args.len() > 3 {
        if let Ok(d) = args[3].parse::<usize>() {
            depth = d;
        }
    }

    // Support custom side via args? For now default 4.

    let _board_state = BitBoardState::new(dimension, side);

    let player_white: Box<dyn PlayerStrategy<BitBoardState>> = match player_white_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth, time_limit, dimension, side)),
        _ => Box::new(HumanConsolePlayer::new()),
    };

    let player_black: Box<dyn PlayerStrategy<BitBoardState>> = match player_black_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth, time_limit, dimension, side)),
        _ => Box::new(MinimaxBot::new(depth, time_limit, dimension, side)),
    };

    let board = Board::<BitBoardState>::new(dimension, side);

    let game = GameService::new(board, player_white, player_black);
    hyperchess::interface::console::ConsoleInterface::run(game);
}
