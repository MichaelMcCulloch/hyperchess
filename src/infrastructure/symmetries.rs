pub struct SymmetryHandler {
    pub maps: Vec<Vec<usize>>,
}

impl SymmetryHandler {
    pub fn new(dimension: usize, side: usize) -> Self {
        let total_cells = side.pow(dimension as u32);
        let mut maps = Vec::new();

        let mut axes: Vec<usize> = (0..dimension).collect();
        let permutations = permute(&mut axes);

        let num_reflections = 1 << dimension;

        for perm in &permutations {
            for ref_mask in 0..num_reflections {
                let mut map = vec![0; total_cells];

                for (i, map_entry) in map.iter_mut().enumerate().take(total_cells) {
                    let coords = index_to_coords(i, dimension, side);

                    let mut new_coords = vec![0; dimension];
                    for (dest_axis, &src_axis) in perm.iter().enumerate() {
                        new_coords[dest_axis] = coords[src_axis];
                    }

                    for (axis, val) in new_coords.iter_mut().enumerate() {
                        if (ref_mask >> axis) & 1 == 1 {
                            *val = side - 1 - *val;
                        }
                    }

                    *map_entry = coords_to_index(&new_coords, side);
                }
                maps.push(map);
            }
        }

        SymmetryHandler { maps }
    }
}

fn permute(arr: &mut [usize]) -> Vec<Vec<usize>> {
    let n = arr.len();
    let mut result = vec![arr.to_vec()];
    let mut c = vec![0usize; n];
    let mut i = 0;
    while i < n {
        if c[i] < i {
            if i % 2 == 0 {
                arr.swap(0, i);
            } else {
                arr.swap(c[i], i);
            }
            result.push(arr.to_vec());
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    result
}

fn index_to_coords(mut index: usize, dim: usize, side: usize) -> Vec<usize> {
    let mut coords = Vec::with_capacity(dim);
    for _ in 0..dim {
        coords.push(index % side);
        index /= side;
    }
    coords
}

fn coords_to_index(coords: &[usize], side: usize) -> usize {
    let mut idx = 0;
    let mut mul = 1;
    for &c in coords {
        idx += c * mul;
        mul *= side;
    }
    idx
}
