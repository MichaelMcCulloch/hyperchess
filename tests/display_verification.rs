#[cfg(test)]
mod tests {
    use hyperchess::domain::board::Board;
    use hyperchess::infrastructure::display::render_board;

    #[test]
    fn test_label_rendering_4d() {
        let board = Board::new(4, 4);
        let output = render_board(&board);
        let expected = r###"      1         2        3        4   
    1 2 3 4                           
A A ♖ . . .| . . . .| . . . .| . . . .
  B ♕ . . .| . . . .| . . . .| . . . .
  C ♔ . . .| . . . .| . . . .| . . . .
  D ♖ . . .| . . . .| . . . .| . . . .
    ----------------------------------
B   . . . .| . ♙ . .| . . . .| . . . .
    . . . .| . ♙ . .| . . . .| . . . .
    . . . .| . ♙ . .| . . . .| . . . .
    . . . .| . ♙ . .| . . . .| . . . .
    ----------------------------------
C   . . . .| . . . .| . . ♟ .| . . . .
    . . . .| . . . .| . . ♟ .| . . . .
    . . . .| . . . .| . . ♟ .| . . . .
    . . . .| . . . .| . . ♟ .| . . . .
    ----------------------------------
D   . . . .| . . . .| . . . .| . . . ♜
    . . . .| . . . .| . . . .| . . . ♛
    . . . .| . . . .| . . . .| . . . ♚
    . . . .| . . . .| . . . .| . . . ♜
"###;

        let strip_ansi = |s: &str| -> String {
            let mut result = String::new();
            let mut in_escape = false;
            for c in s.chars() {
                if c == '\x1b' {
                    in_escape = true;
                }
                if !in_escape {
                    result.push(c);
                }
                if in_escape && c == 'm' {
                    in_escape = false;
                }
            }
            result
        };

        let output_clean = strip_ansi(&output);
        // println!("{}", expected);
        // println!("{}", output_clean);

        assert_eq!(expected, output_clean,);
    }
}
