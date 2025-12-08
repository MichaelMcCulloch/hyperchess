use crate::application::game_service::GameService;
use crate::domain::models::{BoardState, GameResult};
use std::fmt::Display;

pub struct ConsoleInterface;

impl ConsoleInterface {
    pub fn run<S>(mut game_service: GameService<S>)
    where
        S: BoardState + Display,
    {
        println!("Starting Game...");
        println!("{}", game_service.board().state());

        loop {
            if let Some(result) = game_service.is_game_over() {
                match result {
                    GameResult::Win(p) => println!("Player {:?} Wins!", p),
                    GameResult::Draw => println!("It's a Draw!"),
                    _ => {}
                }
                break;
            }

            println!("Player {:?}'s turn", game_service.turn());

            match game_service.perform_next_move() {
                Ok(_) => {
                    println!("{}", game_service.board().state());
                }
                Err(e) => {
                    println!("Error: {}", e);
                    // In a real game we might want to retry immediately if it was input error,
                    // but here the strategy (HumanConsolePlayer) loops internally for valid input.
                    // If we get an error here it's likely "No move available" or "Game Over".
                    if e == "No move available" {
                        break;
                    }
                }
            }
        }
    }
}
