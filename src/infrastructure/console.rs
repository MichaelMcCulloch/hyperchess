use crate::domain::models::{BoardState, Player};
use crate::domain::services::PlayerStrategy;
use std::io::{self, Write};

pub struct HumanConsolePlayer;

impl HumanConsolePlayer {
    pub fn new() -> Self {
        Self
    }
}

use crate::domain::coordinate::Coordinate;

impl<S: BoardState> PlayerStrategy<S> for HumanConsolePlayer {
    fn get_best_move(&mut self, board: &S, _player: Player) -> Option<Coordinate> {
        loop {
            print!("Enter move index (0-{}): ", board.total_cells() - 1);
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            match input.trim().parse::<usize>() {
                Ok(idx) => {
                    // Temporarily using index for input, converting to coordinate
                    // Ideally we'd ask for coordinates (x,y,z) but for now let's keep index input for simpler UI
                    // or implement a conversion.
                    // Since BoardState doesn't expose index conversion directly (it's infrastructure hidden),
                    // we need a way.
                    // But wait, Coordinate is generic. We need to construct it.
                    // We can construct it if we know dimensions.

                    // Actually, for HumanConsolePlayer, we might want to ask for Coordinates?
                    // Or keep index and convert.
                    // But `index_to_coords` is in `infrastructure::persistence`.
                    // We should probably rely on `Coordinate` constructor if we know the dimension.

                    // For now, let's assume we can map index to Coordinate if we knew how.
                    // BUT `BoardState` trait doesn't have `from_index`.

                    // We can compute it manually if we know board dimensions.
                    let dim = board.dimension();
                    let side = board.side();

                    // Re-implement index_to_coords here or make it a domain utility?
                    // It fits in Coordinate logic?
                    // Let's implement it here locally or move it to Coordinate.

                    let mut coords = vec![0; dim];
                    let mut temp = idx;
                    for i in 0..dim {
                        coords[i] = temp % side;
                        temp /= side;
                    }
                    let coord = Coordinate::new(coords);

                    if idx < board.total_cells() && board.get_cell(&coord).is_none() {
                        return Some(coord);
                    } else if idx >= board.total_cells() {
                        println!("Index out of bounds");
                    } else {
                        println!("Cell already occupied");
                    }
                }
                Err(_) => println!("Invalid number"),
            }
        }
    }
}
