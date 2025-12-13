#[cfg(test)]
mod tests {
    use hyperchess::domain::board::Board;
    use hyperchess::infrastructure::display::render_board;

    #[test]
    fn test_label_rendering_4d() {
        // Create a 4D board with small side length to keep output manageable
        // Dim 4, Side 2
        // Structure:
        // Vertical (Dim 4): AA, AB
        //   Horizontal (Dim 3): 11, 12
        //     Board (Dim 2)

        // We expect:
        // AA (Top):
        //   11 (Left): Should have Top Labels (1,2) and Left Labels (A,B)
        //   12 (Right): Should have Top Labels (1,2) but NO Left Labels
        // AB (Bottom):
        //   11 (Left): Should have NO Top Labels but HAVE Left Labels (A,B)
        //   12 (Right): Should have NO Top Labels and NO Left Labels

        let board = Board::new(4, 2);
        let output = render_board(&board);
        let expected = r###"     11    12 
      1 2     
AA  A ♕ ♙| . .
    B ♔ ♙| . .
      --------
AB    . .| . ♛
      . .| . ♚
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
        println!("{}", output_clean);
        println!("{}", expected);

        assert_eq!(expected, output_clean,);
    }
}
