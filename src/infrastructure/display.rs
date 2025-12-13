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
        if s.contains('\x1b') {
            if x < self.width && y < self.height {
                self.buffer[y * self.width + x] = s.to_string();
            }
        } else {
            for (i, c) in s.chars().enumerate() {
                let curr_x = x + i;
                if curr_x < self.width && y < self.height {
                    self.buffer[y * self.width + curr_x] = c.to_string();
                }
            }
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
    let (w, h, _, _) = calculate_metrics(dim, side, true, true);
    let mut canvas = Canvas::new(w, h);

    draw_recursive(board, dim, &mut canvas, 0, 0, 0, true, true);

    canvas.to_string()
}

fn calculate_metrics(
    dim: usize,
    side: usize,
    is_top: bool,
    is_left: bool,
) -> (usize, usize, usize, usize) {
    let res = if dim == 0 {
        (1, 1, 0, 0)
    } else if dim == 1 {
        (side, 1, 0, 0)
    } else if dim == 2 {
        let has_col_labels = is_top;
        let has_row_labels = is_left;

        let body_w = side * 2 - 1;
        let body_h = side;

        let label_w = if has_row_labels { 2 } else { 0 };
        let label_h = if has_col_labels { 1 } else { 0 };

        (body_w + label_w, body_h + label_h, label_w, label_h)
    } else if dim % 2 != 0 {
        let has_labels = is_top;
        let label_h = if has_labels { 1 } else { 0 };
        let gap = 2;

        let (c0_w, c0_h, c0_off_x, c0_off_y) = calculate_metrics(dim - 1, side, is_top, is_left);
        let (other_w, _, _, _) =
            calculate_metrics(dim - 1, side, is_top && false, is_left && false);

        let total_w = c0_w + (side - 1) * (other_w + gap);
        let total_h = c0_h + label_h;

        let content_off_y = label_h + c0_off_y;
        let content_off_x = c0_off_x;

        (total_w, total_h, content_off_x, content_off_y)
    } else {
        let has_labels = is_left;

        let label_w = if has_labels { 2 } else { 0 };
        let actual_gap = 1;

        let (c0_w, c0_h, c0_off_x, c0_off_y) = calculate_metrics(dim - 1, side, is_top, is_left);
        let (other_w, other_h, _, _) =
            calculate_metrics(dim - 1, side, is_top && false, is_left && false);

        let max_child_w = std::cmp::max(c0_w, other_w);
        let total_w = max_child_w + label_w;

        let total_h = c0_h + (side - 1) * (other_h + actual_gap);

        let content_off_x = label_w + c0_off_x;
        let content_off_y = c0_off_y;

        (total_w, total_h, content_off_x, content_off_y)
    };
    res
}

fn draw_recursive(
    board: &Board,
    current_dim: usize,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
    is_top: bool,
    is_left: bool,
) {
    let side = board.side();

    if current_dim == 2 {
        let has_col_labels = is_top;
        let has_row_labels = is_left;

        let col_label_h = if has_col_labels { 1 } else { 0 };
        let row_label_w = if has_row_labels { 2 } else { 0 };

        if has_col_labels {
            for dx in 0..side {
                let label = format!("{}", dx + 1);
                let label_x = x + row_label_w + dx * 2;
                canvas.put(label_x, y, &label);
            }
        }

        for dy in 0..side {
            if has_row_labels {
                let row_char = (b'A' + dy as u8) as char;
                let label_str = format!("{}", row_char);
                canvas.put(x, y + col_label_h + dy, &label_str);
            }

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
                canvas.put(x + row_label_w + dx * 2, y + col_label_h + dy, &s);
            }
        }
        return;
    }

    let stride = side.pow((current_dim - 1) as u32);

    if current_dim % 2 != 0 {
        let has_labels = is_top;
        let label_h = if has_labels { 1 } else { 0 };
        let gap = 2;

        let mut current_x = x;
        let (_, _, _, c0_off_y) = calculate_metrics(current_dim - 1, side, is_top, is_left);

        for i in 0..side {
            let child_is_top = is_top && (i == 0);
            let child_is_left = is_left && (i == 0);
            let next_base = base_index + i * stride;

            let (child_w, child_h, _, this_off_y) =
                calculate_metrics(current_dim - 1, side, child_is_top, child_is_left);

            let align_y = if c0_off_y > this_off_y {
                c0_off_y - this_off_y
            } else {
                0
            };

            if has_labels {
                let label = format!("{}", i + 1);
                let label_len = label.len();
                let center_offset = if child_w > label_len {
                    (child_w - label_len) / 2
                } else {
                    0
                };
                canvas.put(current_x + center_offset, y, &label);
            }

            draw_recursive(
                board,
                current_dim - 1,
                canvas,
                current_x,
                y + label_h + align_y,
                next_base,
                child_is_top,
                child_is_left,
            );

            if i < side - 1 {
                let sep_x = current_x + child_w + gap / 2 - 1;
                for k in this_off_y..child_h {
                    canvas.put(
                        sep_x,
                        y + label_h + align_y + k,
                        &format!("{}|{}", COLOR_DIM, COLOR_RESET),
                    );
                }
                current_x += child_w + gap;
            }
        }
    } else {
        let has_labels = is_left;
        let label_w = if has_labels { 2 } else { 0 };
        let gap = 1;

        let mut current_y = y;
        let (_, _, c0_off_x, _) = calculate_metrics(current_dim - 1, side, is_top, is_left);

        for i in 0..side {
            let child_is_top = is_top && (i == 0);
            let child_is_left = is_left && (i == 0);
            let next_base = base_index + i * stride;

            let (child_w, child_h, this_off_x, this_off_y) =
                calculate_metrics(current_dim - 1, side, child_is_top, child_is_left);

            let align_x = if c0_off_x > this_off_x {
                c0_off_x - this_off_x
            } else {
                0
            };

            if has_labels {
                let suffix_char = (b'A' + i as u8) as char;
                let label = format!("{}", suffix_char);
                canvas.put(x, current_y + this_off_y, &label);
            }

            draw_recursive(
                board,
                current_dim - 1,
                canvas,
                x + label_w + align_x,
                current_y,
                next_base,
                child_is_top,
                child_is_left,
            );

            if i < side - 1 {
                let sep_y = current_y + child_h;

                for k in this_off_x..child_w {
                    canvas.put(
                        x + label_w + align_x + k,
                        sep_y,
                        &format!("{}-{}", COLOR_DIM, COLOR_RESET),
                    );
                }
                current_y += child_h + gap;
            }
        }
    }
}
