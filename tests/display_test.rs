use hyperchess::domain::board::Board;
use hyperchess::infrastructure::display::render_board;

#[test]
fn test_display_labels_2d() {
    let board = Board::new(2, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check Column Labels
    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));

    // Check Row Labels
    assert!(output.contains("A"));
    assert!(output.contains("B"));
    assert!(output.contains("C"));
}

#[test]
fn test_display_labels_3d() {
    let board = Board::new(3, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check Dimension Labels (Horizontal: 11, 12, 13)
    // "1" prefix + index 1..3
    assert!(output.contains("11"));
    assert!(output.contains("12"));
    assert!(output.contains("13"));

    // Check internal 2D labels
    assert!(output.contains("A"));
    assert!(output.contains("1"));
}

#[test]
fn test_display_labels_4d() {
    let board = Board::new(4, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check Dimension Labels (Vertical: AA, AB, AC)
    // "A" prefix + char A..C
    assert!(output.contains("AA"));
    assert!(output.contains("AB"));
    assert!(output.contains("AC"));
}
