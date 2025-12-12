use crate::domain::board::Board;
use crate::domain::models::{PieceType, Player};
use std::fmt;

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_WHITE: &str = "\x1b[37m";
const COLOR_BLACK: &str = "\x1b[31m";
const COLOR_DIM: &str = "\x1b[90m";

struct Canvas {
    width: usize,
    height: usize,
    buffer: Vec<String>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Canvas {
            width,
            height,
            buffer: vec![" ".to_string(); width * height],
        }
    }

    fn put(&mut self, x: usize, y: usize, s: &str) {
        if x < self.width && y < self.height {
            self.buffer[y * self.width + x] = s.to_string();
        }
    }
}

impl fmt::Display for Canvas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for y in 0..self.height {
            for x in 0..self.width {
                write!(f, "{}", self.buffer[y * self.width + x])?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

pub fn render_board(board: &Board) -> String {
    let dim = board.dimension();
    let side = board.side();
    let (w, h) = calculate_size(dim, side);
    let mut canvas = Canvas::new(w, h);

    draw_recursive(board, dim, &mut canvas, 0, 0, 0);

    canvas.to_string()
}

fn calculate_size(dim: usize, side: usize) -> (usize, usize) {
    if dim == 0 {
        return (1, 1);
    }
    if dim == 1 {
        return (side, 1);
    }
    if dim == 2 {
        return (side * 2 - 1, side);
    }

    let (child_w, child_h) = calculate_size(dim - 1, side);

    if dim % 2 != 0 {
        let gap = 2;
        (child_w * side + gap * (side - 1), child_h)
    } else {
        let gap = 1;
        (child_w, child_h * side + gap * (side - 1))
    }
}

fn draw_recursive(
    board: &Board,
    current_dim: usize,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
) {
    let side = board.side();

    if current_dim == 2 {
        for dy in 0..side {
            for dx in 0..side {
                let cell_idx = base_index + dx + dy * side;
                let coord_vals = board.index_to_coords(cell_idx);
                let coord = crate::domain::coordinate::Coordinate::new(coord_vals);

                let s = match board.get_piece(&coord) {
                    Some(piece) => {
                        let symbol = match piece.owner {
                            Player::White => match piece.piece_type {
                                PieceType::Pawn => "♙",
                                PieceType::Knight => "♘",
                                PieceType::Bishop => "♗",
                                PieceType::Rook => "♖",
                                PieceType::Queen => "♕",
                                PieceType::King => "♔",
                            },
                            Player::Black => match piece.piece_type {
                                PieceType::Pawn => "♟",
                                PieceType::Knight => "♞",
                                PieceType::Bishop => "♝",
                                PieceType::Rook => "♜",
                                PieceType::Queen => "♛",
                                PieceType::King => "♚",
                            },
                        };

                        let color = match piece.owner {
                            Player::White => COLOR_WHITE,
                            Player::Black => COLOR_BLACK,
                        };
                        format!("{}{}{}", color, symbol, COLOR_RESET)
                    }
                    None => format!("{}.{}", COLOR_DIM, COLOR_RESET),
                };
                canvas.put(x + dx * 2, y + dy, &s);
            }
        }
        return;
    }

    let (child_w, child_h) = calculate_size(current_dim - 1, side);
    let stride = side.pow((current_dim - 1) as u32);

    if current_dim % 2 != 0 {
        let gap = 2;
        for i in 0..side {
            let next_x = x + i * (child_w + gap);
            let next_y = y;
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < side - 1 {
                let sep_x = next_x + child_w + gap / 2 - 1;
                for k in 0..child_h {
                    canvas.put(sep_x, next_y + k, &format!("{}|{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    } else {
        let gap = 1;
        for i in 0..side {
            let next_x = x;
            let next_y = y + i * (child_h + gap);
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < side - 1 {
                let sep_y = next_y + child_h;
                for k in 0..child_w {
                    canvas.put(next_x + k, sep_y, &format!("{}-{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    }
}
