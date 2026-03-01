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

    draw_board(board, dim, &mut canvas, 0, 0, 0, true, true);

    canvas.to_string()
}

/// Bottom-up DP computation of metrics for each dimension and (is_top, is_left) combo.
/// Returns a table indexed as `table[dim][flag_index]` where flag_index encodes (is_top, is_left):
///   0 = (false, false), 1 = (false, true), 2 = (true, false), 3 = (true, true)
/// Each entry is (width, height, offset_x, offset_y).
fn build_metrics_table(max_dim: usize, side: usize) -> Vec<[(usize, usize, usize, usize); 4]> {
    let mut result = Vec::with_capacity(max_dim + 1);

    // dim 0: always (1, 1, 0, 0)
    let dim0 = [(1, 1, 0, 0); 4];
    result.push(dim0);

    if max_dim == 0 {
        return result;
    }

    // dim 1: always (side, 1, 0, 0)
    let dim1 = [(side, 1, 0, 0); 4];
    result.push(dim1);

    if max_dim == 1 {
        return result;
    }

    // dim 2
    let mut dim2 = [(0, 0, 0, 0); 4];
    for flag_idx in 0..4 {
        let is_top = flag_idx & 2 != 0;
        let is_left = flag_idx & 1 != 0;

        let body_w = side * 2 - 1;
        let body_h = side;
        let label_w = if is_left { 2 } else { 0 };
        let label_h = if is_top { 1 } else { 0 };

        dim2[flag_idx] = (body_w + label_w, body_h + label_h, label_w, label_h);
    }
    result.push(dim2);

    // dim 3..max_dim
    for d in 3..=max_dim {
        let mut entry = [(0, 0, 0, 0); 4];
        let prev = &result[d - 1];

        for flag_idx in 0..4 {
            let is_top = flag_idx & 2 != 0;
            let is_left = flag_idx & 1 != 0;

            // child 0 inherits is_top, is_left; other children get (false, false)
            let c0_flag = flag_idx; // same (is_top, is_left)
            let other_flag = 0; // (false, false)

            let (c0_w, c0_h, c0_off_x, c0_off_y) = prev[c0_flag];
            let (other_w, other_h, _, _) = prev[other_flag];

            if d % 2 != 0 {
                // Odd dim: horizontal layout
                let label_h = if is_top { 1 } else { 0 };
                let gap = 2;

                let total_w = c0_w + (side - 1) * (other_w + gap);
                let total_h = c0_h + label_h;
                let content_off_x = c0_off_x;
                let content_off_y = label_h + c0_off_y;

                entry[flag_idx] = (total_w, total_h, content_off_x, content_off_y);
            } else {
                // Even dim: vertical layout
                let label_w = if is_left { 2 } else { 0 };
                let actual_gap = 1;

                let max_child_w = std::cmp::max(c0_w, other_w);
                let total_w = max_child_w + label_w;
                let total_h = c0_h + (side - 1) * (other_h + actual_gap);
                let content_off_x = label_w + c0_off_x;
                let content_off_y = c0_off_y;

                entry[flag_idx] = (total_w, total_h, content_off_x, content_off_y);
            }
        }
        result.push(entry);
    }

    result
}

fn flag_index(is_top: bool, is_left: bool) -> usize {
    (if is_top { 2 } else { 0 }) | (if is_left { 1 } else { 0 })
}

fn calculate_metrics(
    dim: usize,
    side: usize,
    is_top: bool,
    is_left: bool,
) -> (usize, usize, usize, usize) {
    let table = build_metrics_table(dim, side);
    table[dim][flag_index(is_top, is_left)]
}

/// Draws a 2D sub-board directly onto the canvas (base case).
fn draw_2d(
    board: &Board,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
    is_top: bool,
    is_left: bool,
) {
    let side = board.side();
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
}

struct DrawFrame {
    current_dim: usize,
    x: usize,
    y: usize,
    base_index: usize,
    is_top: bool,
    is_left: bool,
    child_idx: usize,
    current_pos: usize, // current_x for odd dims, current_y for even dims
}

/// Iterative board drawing using an explicit stack instead of recursion.
fn draw_board(
    board: &Board,
    dim: usize,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    base_index: usize,
    is_top: bool,
    is_left: bool,
) {
    let side = board.side();
    let metrics = build_metrics_table(dim, side);

    let mut stack: Vec<DrawFrame> = Vec::with_capacity(dim);

    stack.push(DrawFrame {
        current_dim: dim,
        x,
        y,
        base_index,
        is_top,
        is_left,
        child_idx: 0,
        current_pos: 0, // will be set on first iteration
    });

    while let Some(frame) = stack.last_mut() {
        let current_dim = frame.current_dim;

        // Base case: draw 2D grid directly
        if current_dim == 2 {
            draw_2d(
                board,
                canvas,
                frame.x,
                frame.y,
                frame.base_index,
                frame.is_top,
                frame.is_left,
            );
            stack.pop();
            continue;
        }

        let stride = side.pow((current_dim - 1) as u32);
        let i = frame.child_idx;

        // Initialize current_pos on first child
        if i == 0 {
            if current_dim % 2 != 0 {
                frame.current_pos = frame.x;
            } else {
                frame.current_pos = frame.y;
            }
        }

        // All children processed — pop this frame
        if i >= side {
            stack.pop();
            continue;
        }

        let child_is_top = frame.is_top && (i == 0);
        let child_is_left = frame.is_left && (i == 0);
        let next_base = frame.base_index + i * stride;

        let child_flag = flag_index(child_is_top, child_is_left);
        let (child_w, child_h, child_off_x, child_off_y) = metrics[current_dim - 1][child_flag];

        if current_dim % 2 != 0 {
            // Odd dim: horizontal layout
            let has_labels = frame.is_top;
            let label_h = if has_labels { 1 } else { 0 };
            let gap = 2;

            let c0_flag = flag_index(frame.is_top, frame.is_left);
            let (_, _, _, c0_off_y) = metrics[current_dim - 1][c0_flag];

            let align_y = c0_off_y.saturating_sub(child_off_y);

            // Draw label for this child
            if has_labels {
                let label = format!("{}", i + 1);
                let label_len = label.len();
                let center_offset = if child_w > label_len {
                    (child_w - label_len) / 2
                } else {
                    0
                };
                canvas.put(frame.current_pos + center_offset, frame.y, &label);
            }

            let child_x = frame.current_pos;
            let child_y = frame.y + label_h + align_y;

            // Draw separator after this child (before advancing)
            if i < side - 1 {
                let sep_x = frame.current_pos + child_w + gap / 2 - 1;
                for k in child_off_y..child_h {
                    canvas.put(
                        sep_x,
                        frame.y + label_h + align_y + k,
                        &format!("{}|{}", COLOR_DIM, COLOR_RESET),
                    );
                }
                frame.current_pos += child_w + gap;
            }

            frame.child_idx += 1;

            // Push child frame
            stack.push(DrawFrame {
                current_dim: current_dim - 1,
                x: child_x,
                y: child_y,
                base_index: next_base,
                is_top: child_is_top,
                is_left: child_is_left,
                child_idx: 0,
                current_pos: 0,
            });
        } else {
            // Even dim: vertical layout
            let has_labels = frame.is_left;
            let label_w = if has_labels { 2 } else { 0 };
            let gap = 1;

            let c0_flag = flag_index(frame.is_top, frame.is_left);
            let (_, _, c0_off_x, _) = metrics[current_dim - 1][c0_flag];

            let align_x = c0_off_x.saturating_sub(child_off_x);

            // Draw label for this child
            if has_labels {
                let suffix_char = (b'A' + i as u8) as char;
                let label = format!("{}", suffix_char);
                canvas.put(frame.x, frame.current_pos + child_off_y, &label);
            }

            let child_x = frame.x + label_w + align_x;
            let child_y = frame.current_pos;

            // Draw separator after this child (before advancing)
            if i < side - 1 {
                let sep_y = frame.current_pos + child_h;
                for k in child_off_x..child_w {
                    canvas.put(
                        frame.x + label_w + align_x + k,
                        sep_y,
                        &format!("{}-{}", COLOR_DIM, COLOR_RESET),
                    );
                }
                frame.current_pos += child_h + gap;
            }

            frame.child_idx += 1;

            // Push child frame
            stack.push(DrawFrame {
                current_dim: current_dim - 1,
                x: child_x,
                y: child_y,
                base_index: next_base,
                is_top: child_is_top,
                is_left: child_is_left,
                child_idx: 0,
                current_pos: 0,
            });
        }
    }
}
