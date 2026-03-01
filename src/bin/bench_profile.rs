use hyperchess::config::AppConfig;
use hyperchess::domain::board::Board;
use hyperchess::domain::models::Player;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;
use std::env;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();

    let dimension: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let depth: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);

    let mut config = AppConfig::load();
    config.minimax.depth = depth;

    println!(
        "Profiling {}D chess, depth {}, concurrency {}",
        dimension, depth, config.compute.concurrency
    );

    let board = Board::new(dimension, 8);
    let mut bot = MinimaxBot::new(&config, dimension, 8);

    let start = Instant::now();
    let mv = bot.get_move(&board, Player::White);
    let elapsed = start.elapsed();

    match mv {
        Some(m) => println!("Best move: {:?} in {:.2?}", m, elapsed),
        None => println!("No move found in {:.2?}", elapsed),
    }
}
