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
                    GameResult::Checkmate(p) => println!("Checkmate! Player {:?} Wins!", p),
                    GameResult::Stalemate => println!("Stalemate! It's a Draw!"),
                    GameResult::Draw => println!("Draw!"),
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
                    if e == "No move available" {
                        // Should be caught by is_game_over if checks are correct,
                        // but if perform_next_move fails explicitly:
                        break;
                    }
                }
            }
        }
    }
}
