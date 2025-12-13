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

    assert!(output.contains("11"));
    assert!(output.contains("12"));
    assert!(output.contains("13"));

    assert!(output.contains("A"));
    assert!(output.contains("1"));
}

#[test]
fn test_display_labels_4d() {
    let board = Board::new(4, 3);
    let output = render_board(&board);
    println!("{}", output);

    assert!(output.contains("AA"));
    assert!(output.contains("AB"));
    assert!(output.contains("AC"));
}
