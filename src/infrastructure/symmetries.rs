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

                for i in 0..total_cells {
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

                    map[i] = coords_to_index(&new_coords, side);
                }
                maps.push(map);
            }
        }

        SymmetryHandler { maps }
    }
}

fn permute(arr: &mut [usize]) -> Vec<Vec<usize>> {
    let mut res = Vec::new();
    heap_permute(arr.len(), arr, &mut res);
    res
}

fn heap_permute(k: usize, arr: &mut [usize], res: &mut Vec<Vec<usize>>) {
    if k == 1 {
        res.push(arr.to_vec());
    } else {
        heap_permute(k - 1, arr, res);
        for i in 0..k - 1 {
            if k % 2 == 0 {
                arr.swap(i, k - 1);
            } else {
                arr.swap(0, k - 1);
            }
            heap_permute(k - 1, arr, res);
        }
    }
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
