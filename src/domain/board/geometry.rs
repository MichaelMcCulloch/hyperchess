use crate::domain::board::BitBoardLarge;
use crate::domain::board::board_representation::BoardRepresentation;
use crate::domain::board::cache::GenericBoardCache;

/// Immutable board topology. Created once, shared via Arc across all threads.
/// Combines dimension/side/total_cells with the precomputed cache data.
#[derive(Debug)]
pub struct BoardGeometry<R: BoardRepresentation> {
    pub dimension: usize,
    pub side: usize,
    pub total_cells: usize,
    pub cache: GenericBoardCache<R>,
}

pub type Geometry = BoardGeometry<BitBoardLarge>;

impl<R: BoardRepresentation> BoardGeometry<R> {
    pub fn new(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let cache = GenericBoardCache::new(dimension, side);
        Self {
            dimension,
            side,
            total_cells,
            cache,
        }
    }
}
