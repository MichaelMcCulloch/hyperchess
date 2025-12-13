use crate::domain::board::Board;
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, Player};
use crate::domain::services::PlayerStrategy;
use std::io::{self, Write};

pub struct HumanConsolePlayer;

impl HumanConsolePlayer {
    pub fn new() -> Self {
        Self
    }

    fn parse_coordinate(input: &str, dim: usize, side: usize) -> Result<Coordinate, String> {
        let mut remaining = input.trim();
        let mut coords = vec![0; dim];

        for d in (0..dim).rev() {
            if remaining.is_empty() {
                return Err(format!(
                    "Insufficient parts for {}-dimensional coordinate",
                    dim
                ));
            }

            if d % 2 != 0 {
                let end_idx = remaining
                    .find(|c: char| !c.is_ascii_alphabetic())
                    .unwrap_or(remaining.len());

                if end_idx == 0 {
                    return Err(format!(
                        "Expected Letter for Dimension {}, found number/symbol",
                        d + 1
                    ));
                }

                let letter_part = &remaining[..end_idx];
                remaining = &remaining[end_idx..];

                let val = if letter_part.len() == 1 {
                    let c = letter_part.chars().next().unwrap().to_ascii_uppercase();
                    (c as u8).saturating_sub(b'A') as usize
                } else {
                    let c = letter_part.chars().last().unwrap().to_ascii_uppercase();
                    (c as u8).saturating_sub(b'A') as usize
                };

                if val >= side {
                    return Err(format!(
                        "Coordinate letter '{}' out of bounds (Max {})",
                        letter_part,
                        (b'A' + (side - 1) as u8) as char
                    ));
                }
                coords[d] = val;
            } else {
                let end_idx = remaining
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(remaining.len());

                if end_idx == 0 {
                    return Err(format!(
                        "Expected Number for Dimension {}, found letter",
                        d + 1
                    ));
                }

                let number_part = &remaining[..end_idx];
                remaining = &remaining[end_idx..];

                let val: usize = number_part.parse().map_err(|_| "Invalid number")?;
                if val == 0 || val > side {
                    return Err(format!(
                        "Coordinate number '{}' out of bounds (1-{})",
                        val, side
                    ));
                }
                coords[d] = val - 1;
            }
        }

        Ok(Coordinate::new(coords))
    }
}

impl PlayerStrategy for HumanConsolePlayer {
    fn get_move(&mut self, board: &Board, _player: Player) -> Option<Move> {
        let dim = board.dimension();
        let side = board.side();

        loop {
            let example = match dim {
                2 => "e4 e5",
                3 => "1e4 1e5",
                4 => "A1e4 A1e5",
                _ => "coord1 coord2",
            };

            println!(
                "Enter Move (Format: From To). Alternating Letter/Number. Example: '{}'",
                example
            );
            print!("> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            let parts: Vec<&str> = input.trim().split_whitespace().collect();
            if parts.len() < 2 {
                println!("Please provide two coordinates: From To");
                continue;
            }

            let from_res = Self::parse_coordinate(parts[0], dim, side);
            let to_res = Self::parse_coordinate(parts[1], dim, side);

            match (from_res, to_res) {
                (Ok(from_coord), Ok(to_coord)) => {
                    let promotion = if parts.len() > 2 {
                        match parts[2].to_lowercase().as_str() {
                            "q" | "queen" | "4" => Some(crate::domain::models::PieceType::Queen),
                            "r" | "rook" | "3" => Some(crate::domain::models::PieceType::Rook),
                            "b" | "bishop" | "2" => Some(crate::domain::models::PieceType::Bishop),
                            "n" | "knight" | "1" => Some(crate::domain::models::PieceType::Knight),
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
                (Err(e), _) => println!("Invalid 'From': {}", e),
                (_, Err(e)) => println!("Invalid 'To': {}", e),
            }
        }
    }
}
