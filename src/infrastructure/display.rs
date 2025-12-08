use crate::domain::models::{BoardState, Player};
use std::fmt;

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_X: &str = "\x1b[31m";
const COLOR_O: &str = "\x1b[36m";
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

pub fn render_board<S: BoardState>(board: &S) -> String {
    let dim = board.dimension();
    let (w, h) = calculate_size(dim);
    let mut canvas = Canvas::new(w, h);

    draw_recursive(board, dim, &mut canvas, 0, 0, 0);

    canvas.to_string()
}

fn calculate_size(dim: usize) -> (usize, usize) {
    if dim == 0 {
        return (1, 1);
    }
    if dim == 1 {
        return (3, 1);
    }

    if dim == 2 {
        return (5, 3);
    }

    let (child_w, child_h) = calculate_size(dim - 1);

    if dim % 2 != 0 {
        let gap = 2;
        (child_w * 3 + gap * 2, child_h)
    } else {
        let gap = 1;
        (child_w, child_h * 3 + gap * 2)
    }
}

fn draw_recursive<S: BoardState>(
    board: &S,
    current_dim: usize,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
) {
    let side = 3;

    if current_dim == 2 {
        for dy in 0..3 {
            for dx in 0..3 {
                let cell_idx = base_index + dx + dy * side;
                let coord_vals = crate::infrastructure::persistence::index_to_coords(
                    cell_idx,
                    board.dimension(),
                    board.side(),
                );
                let coord = crate::domain::coordinate::Coordinate::new(coord_vals);

                let s = match board.get_cell(&coord) {
                    Some(Player::X) => format!("{}X{}", COLOR_X, COLOR_RESET),
                    Some(Player::O) => format!("{}O{}", COLOR_O, COLOR_RESET),
                    None => format!("{}.{}", COLOR_DIM, COLOR_RESET),
                };
                canvas.put(x + dx * 2, y + dy, &s);
            }
        }
        return;
    }

    let (child_w, child_h) = calculate_size(current_dim - 1);
    let stride = side.pow((current_dim - 1) as u32);

    if current_dim % 2 != 0 {
        let gap = 2;
        for i in 0..3 {
            let next_x = x + i * (child_w + gap);
            let next_y = y;
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < 2 {
                let sep_x = next_x + child_w + gap / 2 - 1;
                for k in 0..child_h {
                    canvas.put(sep_x, next_y + k, &format!("{}|{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    } else {
        let gap = 1;
        for i in 0..3 {
            let next_x = x;
            let next_y = y + i * (child_h + gap);
            let next_base = base_index + i * stride;
            draw_recursive(board, current_dim - 1, canvas, next_x, next_y, next_base);

            if i < 2 {
                let sep_y = next_y + child_h;
                for k in 0..child_w {
                    canvas.put(next_x + k, sep_y, &format!("{}-{}", COLOR_DIM, COLOR_RESET));
                }
            }
        }
    }
}
