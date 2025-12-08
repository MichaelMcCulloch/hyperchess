use hypertictactoe::application::game_service::GameService;
use hypertictactoe::domain::models::{Board, BoardState};
use hypertictactoe::domain::services::PlayerStrategy;
use hypertictactoe::infrastructure::ai::MinimaxBot;
use hypertictactoe::infrastructure::console::HumanConsolePlayer;
use hypertictactoe::infrastructure::persistence::BitBoardState;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dimension = 3;
    let mut player_x_type = "h";
    let mut player_o_type = "c";
    let mut depth = usize::MAX;

    if args.len() > 1 {
        if let Ok(d) = args[1].parse::<usize>() {
            dimension = d;
        }
    }
    if args.len() > 2 {
        let mode = args[2].as_str();
        if mode.len() >= 2 {
            player_x_type = &mode[0..1];
            player_o_type = &mode[1..2];
        }
    }
    if args.len() > 3 {
        if let Ok(d) = args[3].parse::<usize>() {
            depth = d;
        }
    }

    let _board_state = BitBoardState::new(dimension);

    let player_x: Box<dyn PlayerStrategy<BitBoardState>> = match player_x_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth)),
        _ => Box::new(HumanConsolePlayer::new()),
    };

    let player_o: Box<dyn PlayerStrategy<BitBoardState>> = match player_o_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => Box::new(MinimaxBot::new(depth)),
        _ => Box::new(MinimaxBot::new(depth)),
    };

    // Board generic param inference?
    // We need to explicitly type the Board or let it infer from GameService.
    let board = Board::<BitBoardState>::new(dimension);

    let game = GameService::new(board, player_x, player_o);
    hypertictactoe::interface::console::ConsoleInterface::run(game);
}
