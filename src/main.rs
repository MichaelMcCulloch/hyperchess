use hyperchess::application::game_service::GameService;
use hyperchess::config::AppConfig;
use hyperchess::domain::board::Board;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::{MctsBot, MinimaxBot};
use hyperchess::infrastructure::console::HumanConsolePlayer;
use std::env;

#[cfg(feature = "api")]
#[tokio::main]
async fn main() {
    println!("Starting HyperChess API Server...");
    hyperchess::api::start_server().await;
}

#[cfg(not(feature = "api"))]
fn main() {
    run_cli();
}

#[allow(dead_code)]
fn run_cli() {
    let args: Vec<String> = env::args().collect();

    let mut config = AppConfig::load();
    let mut dimension = 2;
    let side = 8;
    let mut player_white_type = "h";
    let mut player_black_type = "c";

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
            config.minimax.depth = d;
        }
    }

    let create_bot = |config: &AppConfig| -> Box<dyn PlayerStrategy> {
        if config.mcts.is_some() {
            Box::new(MctsBot::new(config))
        } else {
            Box::new(MinimaxBot::new(&config, dimension, side))
        }
    };

    let player_white: Box<dyn PlayerStrategy> = match player_white_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => create_bot(&config),
        _ => Box::new(HumanConsolePlayer::new()),
    };

    let player_black: Box<dyn PlayerStrategy> = match player_black_type {
        "h" => Box::new(HumanConsolePlayer::new()),
        "c" => create_bot(&config),
        _ => create_bot(&config),
    };

    let board = Board::new(dimension, side);

    let game = GameService::new(board, player_white, player_black);
    hyperchess::interface::console::ConsoleInterface::run(game);
}
