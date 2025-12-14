use crate::domain::models::Player;

pub fn get_rook_directions_calc(dimension: usize) -> Vec<Vec<isize>> {
    let mut dirs = Vec::new();
    for i in 0..dimension {
        let mut v = vec![0; dimension];
        v[i] = 1;
        dirs.push(v.clone());
        v[i] = -1;
        dirs.push(v);
    }
    dirs
}

pub fn get_bishop_directions_calc(dimension: usize) -> Vec<Vec<isize>> {
    let mut dirs = Vec::new();
    let num_dirs = 3_usize.pow(dimension as u32);
    for i in 0..num_dirs {
        let mut dir = Vec::with_capacity(dimension);
        let mut temp = i;
        let mut nonzero_count = 0;
        for _ in 0..dimension {
            let val = match temp % 3 {
                0 => 0,
                1 => {
                    nonzero_count += 1;
                    1
                }
                2 => {
                    nonzero_count += 1;
                    -1
                }
                _ => unreachable!(),
            };
            dir.push(val);
            temp /= 3;
        }
        if nonzero_count > 0 && nonzero_count % 2 == 0 {
            dirs.push(dir);
        }
    }
    dirs
}

pub fn get_knight_offsets_calc(dimension: usize) -> Vec<Vec<isize>> {
    let mut offsets = Vec::new();
    for i in 0..dimension {
        for j in 0..dimension {
            if i == j {
                continue;
            }
            for s1 in [-1, 1] {
                for s2 in [-1, 1] {
                    let mut v = vec![0; dimension];
                    v[i] = 2 * s1;
                    v[j] = 1 * s2;
                    offsets.push(v);
                }
            }
        }
    }
    offsets
}

pub fn get_king_offsets_calc(dimension: usize) -> Vec<Vec<isize>> {
    let mut offsets = Vec::new();
    let num_dirs = 3_usize.pow(dimension as u32);
    for i in 0..num_dirs {
        let mut dir = Vec::with_capacity(dimension);
        let mut temp = i;
        let mut all_zero = true;
        for _ in 0..dimension {
            let val = match temp % 3 {
                0 => 0,
                1 => 1,
                2 => -1,
                _ => unreachable!(),
            };
            if val != 0 {
                all_zero = false;
            }
            dir.push(val);
            temp /= 3;
        }
        if !all_zero {
            offsets.push(dir);
        }
    }
    offsets
}

pub fn get_pawn_capture_offsets_calc(dimension: usize, attacker: Player) -> Vec<Vec<isize>> {
    let direction = match attacker {
        Player::White => -1,
        Player::Black => 1,
    };
    let mut offsets = Vec::new();
    for i in 1..dimension {
        for s in [-1, 1] {
            let mut v = vec![0; dimension];
            v[0] = direction;
            v[i] = s;
            offsets.push(v);
        }
    }
    offsets
}
