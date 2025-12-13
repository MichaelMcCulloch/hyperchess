use hyperchess::domain::board::Board;
use hyperchess::infrastructure::display::render_board;

#[test]
fn test_display_labels_2d() {
    let board = Board::new(2, 3);
    let output = render_board(&board);
    println!("{}", output);

    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));

    assert!(output.contains("A"));
    assert!(output.contains("B"));
    assert!(output.contains("C"));
}

#[test]
fn test_display_labels_3d() {
    let board = Board::new(3, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check for presence of column labels (1, 2, 3)
    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));

    // Nested labels (only on first column)
    assert!(output.contains("1 2 3"));

    // Check for row labels
    assert!(output.contains("A"));
    assert!(output.contains("B"));
    assert!(output.contains("C"));
}

#[test]
fn test_display_labels_4d() {
    let board = Board::new(4, 3);
    let output = render_board(&board);
    println!("{}", output);

    // Check for presence of column labels
    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));

    // Row labels
    assert!(output.contains("A"));
    assert!(output.contains("B"));
    assert!(output.contains("C"));
}
