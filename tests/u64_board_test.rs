use hyperchess::domain::board::{BitBoard64, BoardRepresentation, GenericBoard};

#[test]
fn test_u64_board_initialization() {
    let mut board = GenericBoard::<BitBoard64>::new_empty(2, 8);

    assert_eq!(board.dimension, 2);
    assert_eq!(board.side, 8);
    assert_eq!(board.total_cells, 64);

    board.white_occupancy.set_bit(0);
    assert!(board.white_occupancy.get_bit(0));
    assert_eq!(board.white_occupancy.count_ones(), 1);

    board.white_occupancy.clear_bit(0);
    assert!(!board.white_occupancy.get_bit(0));

    assert_eq!(board.cache.index_to_coords.len(), 64);

    board.white_occupancy.set_bit(10);
    board.black_occupancy.set_bit(20);

    let combined = board.white_occupancy.clone() | &board.black_occupancy;
    assert!(combined.get_bit(10));
    assert!(combined.get_bit(20));
    assert_eq!(combined.count_ones(), 2);
}
