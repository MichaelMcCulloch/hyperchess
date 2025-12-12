use crate::domain::coordinate::Coordinate;
use crate::domain::models::{BoardState, Move, Player};
use crate::domain::services::PlayerStrategy;
use std::io::{self, Write};

pub struct HumanConsolePlayer;

impl HumanConsolePlayer {
    pub fn new() -> Self {
        Self
    }

    fn parse_index(input: &str) -> Result<usize, String> {
        input
            .trim()
            .parse::<usize>()
            .map_err(|_| "Invalid number".to_string())
    }

    fn index_to_coord(idx: usize, dim: usize, side: usize) -> Coordinate {
        let mut coords = vec![0; dim];
        let mut temp = idx;
        for i in 0..dim {
            coords[i] = temp % side;
            temp /= side;
        }
        Coordinate::new(coords)
    }
}

impl<S: BoardState> PlayerStrategy<S> for HumanConsolePlayer {
    fn get_move(&mut self, board: &S, _player: Player) -> Option<Move> {
        let dim = board.dimension();
        let side = board.side();
        let total = board.total_cells();

        loop {
            println!(
                "Enter Move (From Index -> To Index, e.g. '0 10'). Max Index: {}",
                total - 1
            );
            print!("> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            let parts: Vec<&str> = input.trim().split_whitespace().collect();
            if parts.len() < 2 {
                println!("Please provide two indices: From To");
                continue;
            }

            let from_res = Self::parse_index(parts[0]);
            let to_res = Self::parse_index(parts[1]);

            match (from_res, to_res) {
                (Ok(from_idx), Ok(to_idx)) => {
                    if from_idx >= total || to_idx >= total {
                        println!("Index out of bounds!");
                        continue;
                    }

                    let from_coord = Self::index_to_coord(from_idx, dim, side);
                    let to_coord = Self::index_to_coord(to_idx, dim, side);

                    // Optional promotion?
                    let promotion = if parts.len() > 2 {
                        // primitive parsing for now
                        match parts[2] {
                            "Q" | "q" | "4" => Some(crate::domain::models::PieceType::Queen),
                            "R" | "r" | "3" => Some(crate::domain::models::PieceType::Rook),
                            "B" | "b" | "2" => Some(crate::domain::models::PieceType::Bishop),
                            "N" | "n" | "1" => Some(crate::domain::models::PieceType::Knight),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    return Some(Move {
                        from: from_coord,
                        to: to_coord,
                        promotion,
                    });
                }
                _ => println!("Invalid indices."),
            }
        }
    }
}
