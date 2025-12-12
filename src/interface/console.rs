use crate::application::game_service::GameService;
use crate::domain::models::GameResult;
use crate::infrastructure::display::render_board;

pub struct ConsoleInterface;

impl ConsoleInterface {
    pub fn run(mut game_service: GameService) {
        println!("Starting Game...");
        println!("{}", render_board(game_service.board()));

        let mut move_count = 0;
        loop {
            if move_count >= 2 {
                println!("Terminating game after 10 moves (temporary limit).");
                break;
            }
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
                    println!("{}", render_board(game_service.board()));
                    move_count += 1;
                }
                Err(e) => {
                    println!("Error: {}", e);
                    if e == "No move available" {
                        break;
                    }
                }
            }
        }
    }
}
