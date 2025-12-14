use crate::domain::board::{BitBoard, Board};
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType, Player};
use smallvec::SmallVec;

pub type MoveList = SmallVec<[Move; 64]>;

pub struct Rules;

impl Rules {
    pub fn generate_legal_moves(board: &mut Board, player: Player) -> MoveList {
        let mut moves = MoveList::new();
        let pseudo_legal = Self::generate_pseudo_legal_moves(board, player);

        for mv in pseudo_legal {
            if !Self::leaves_king_in_check(board, player, &mv) {
                moves.push(mv);
            }
        }

        Self::generate_castling_moves(board, player, &mut moves);
        moves
    }

    pub fn count_piece_mobility(board: &Board, index: usize, piece_type: PieceType) -> i32 {
        let coords = board.index_to_coords(index);
        let coord = Coordinate::new(coords.clone());
        let player = board
            .get_piece_at_index(index)
            .map(|p| p.owner)
            .unwrap_or(Player::White);

        let mut count = 0;

        match piece_type {
            PieceType::Pawn => {
                return 0;
            }
            PieceType::Knight => {
                let offsets = Self::get_knight_offsets(board.dimension);
                return Self::count_leaper_moves(board, &coord, player, &offsets);
            }
            PieceType::King => {
                let offsets = Self::get_king_offsets(board.dimension);
                return Self::count_leaper_moves(board, &coord, player, &offsets);
            }
            PieceType::Rook => {
                let dirs = Self::get_rook_directions(board.dimension);
                return Self::count_slider_moves(board, &coord, player, &dirs);
            }
            PieceType::Bishop => {
                let dirs = Self::get_bishop_directions(board.dimension);
                return Self::count_slider_moves(board, &coord, player, &dirs);
            }
            PieceType::Queen => {
                let r_dirs = Self::get_rook_directions(board.dimension);
                let b_dirs = Self::get_bishop_directions(board.dimension);
                count += Self::count_slider_moves(board, &coord, player, &r_dirs);
                count += Self::count_slider_moves(board, &coord, player, &b_dirs);
                return count;
            }
        }
    }

    fn count_leaper_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        offsets: &[Vec<isize>],
    ) -> i32 {
        let mut count = 0;
        let same_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        for offset in offsets {
            if let Some(target) = Self::apply_offset(&origin.values, offset, board.side) {
                if let Some(idx) = board.coords_to_index(&target) {
                    if !same_occupancy.get_bit(idx) {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn count_slider_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        directions: &[Vec<isize>],
    ) -> i32 {
        let origin_idx = board.coords_to_index(&origin.values).unwrap();
        let mut count = 0;

        let mut generator = board.white_occupancy.zero_like();
        generator.set_bit(origin_idx);

        let all_occupancy = &board.white_occupancy | &board.black_occupancy;
        let own_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        let empty = match all_occupancy {
            BitBoard::Small(b) => {
                let mask =
                    (1u32.checked_shl(board.total_cells as u32).unwrap_or(0)).wrapping_sub(1);
                BitBoard::Small((!b) & mask)
            }
            BitBoard::Medium(b) => {
                let mask =
                    (1u128.checked_shl(board.total_cells as u32).unwrap_or(0)).wrapping_sub(1);
                BitBoard::Medium((!b) & mask)
            }
            BitBoard::Large { data } => {
                let mut new_data = Vec::with_capacity(data.len());
                let mut remaining = board.total_cells;
                for val in data {
                    let limit = std::cmp::min(64, remaining);
                    let mask = if limit == 64 {
                        !0u64
                    } else {
                        (1u64 << limit) - 1
                    };
                    new_data.push((!val) & mask);
                    remaining = remaining.saturating_sub(64);
                }
                BitBoard::Large { data: new_data }
            }
        };

        for dir in directions {
            let stride = Self::calculate_stride(board, dir);
            if stride == 0 {
                continue;
            }
            let attacks = Self::kogge_stone_fill(&generator, &empty, stride, board, dir);
            let valid_moves = &attacks & &(!own_occupancy);
            count += valid_moves.count_ones() as i32;

            if valid_moves.get_bit(origin_idx) {
                count -= 1;
            }
        }
        count
    }

    pub fn generate_loud_moves(board: &mut Board, player: Player) -> MoveList {
        let mut moves = MoveList::new();
        let pseudo_legal = Self::generate_pseudo_legal_moves(board, player);

        for mv in pseudo_legal {
            let is_loud = {
                let enemy_occupancy = match player {
                    Player::White => &board.black_occupancy,
                    Player::Black => &board.white_occupancy,
                };

                let to_idx = board.coords_to_index(&mv.to.values);

                let is_capture = if let Some(idx) = to_idx {
                    enemy_occupancy.get_bit(idx)
                } else {
                    false
                };

                let is_ep_capture = if let Some((ep_idx, _)) = board.en_passant_target {
                    if let Some(idx) = to_idx {
                        idx == ep_idx
                            && board.pawns.get_bit(
                                board.coords_to_index(&mv.from.values).unwrap_or(usize::MAX),
                            )
                    } else {
                        false
                    }
                } else {
                    false
                };

                let is_promotion = mv.promotion.is_some();
                is_capture || is_promotion || is_ep_capture
            };

            if is_loud {
                if !Self::leaves_king_in_check(board, player, &mv) {
                    moves.push(mv);
                }
            }
        }
        moves
    }

    pub fn is_square_attacked(board: &Board, square: &Coordinate, by_player: Player) -> bool {
        let _index = if let Some(idx) = board.coords_to_index(&square.values) {
            idx
        } else {
            return false;
        };

        let enemy_occupancy = match by_player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        let dimension = board.dimension;
        let side = board.side;

        let knight_offsets = Self::get_knight_offsets(dimension);
        for offset in &knight_offsets {
            if let Some(target_coord) = Self::apply_offset(&square.values, offset, side) {
                if let Some(target_idx) = board.coords_to_index(&target_coord) {
                    if enemy_occupancy.get_bit(target_idx) && board.knights.get_bit(target_idx) {
                        return true;
                    }
                }
            }
        }

        let king_offsets = Self::get_king_offsets(dimension);
        for offset in &king_offsets {
            if let Some(target_coord) = Self::apply_offset(&square.values, offset, side) {
                if let Some(target_idx) = board.coords_to_index(&target_coord) {
                    if enemy_occupancy.get_bit(target_idx) && board.kings.get_bit(target_idx) {
                        return true;
                    }
                }
            }
        }

        let rook_dirs = Self::get_rook_directions(dimension);
        for dir in &rook_dirs {
            if Self::scan_ray_for_threat(
                board,
                &square.values,
                dir,
                by_player,
                &[PieceType::Rook, PieceType::Queen],
            ) {
                return true;
            }
        }

        let bishop_dirs = Self::get_bishop_directions(dimension);
        for dir in &bishop_dirs {
            if Self::scan_ray_for_threat(
                board,
                &square.values,
                dir,
                by_player,
                &[PieceType::Bishop, PieceType::Queen],
            ) {
                return true;
            }
        }

        let pawn_attack_offsets = Self::get_pawn_capture_offsets_for_target(dimension, by_player);
        for offset in &pawn_attack_offsets {
            if let Some(target_coord) = Self::apply_offset(&square.values, offset, side) {
                if let Some(target_idx) = board.coords_to_index(&target_coord) {
                    if enemy_occupancy.get_bit(target_idx) && board.pawns.get_bit(target_idx) {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn scan_ray_for_threat(
        board: &Board,
        origin_vals: &[usize],
        direction: &[isize],
        attacker: Player,
        threat_types: &[PieceType],
    ) -> bool {
        let mut current: SmallVec<[usize; 4]> = SmallVec::from_slice(origin_vals);
        let enemy_occupancy = match attacker {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        loop {
            if let Some(next) = Self::apply_offset(&current, direction, board.side) {
                if let Some(idx) = board.coords_to_index(&next) {
                    if all_occupancy.get_bit(idx) {
                        if enemy_occupancy.get_bit(idx) {
                            for &t in threat_types {
                                let match_found = match t {
                                    PieceType::Rook => board.rooks.get_bit(idx),
                                    PieceType::Bishop => board.bishops.get_bit(idx),
                                    PieceType::Queen => board.queens.get_bit(idx),
                                    _ => false,
                                };
                                if match_found {
                                    return true;
                                }
                            }
                            return false;
                        } else {
                            return false;
                        }
                    }
                    current = next;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        false
    }

    fn generate_pseudo_legal_moves(board: &Board, player: Player) -> MoveList {
        let mut moves = MoveList::new();
        let occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for i in occupancy.iter_indices() {
            let coord_vals = board.index_to_coords(i);
            let coord = Coordinate::new(coord_vals.clone());

            let piece_type = if board.pawns.get_bit(i) {
                PieceType::Pawn
            } else if board.knights.get_bit(i) {
                PieceType::Knight
            } else if board.bishops.get_bit(i) {
                PieceType::Bishop
            } else if board.rooks.get_bit(i) {
                PieceType::Rook
            } else if board.queens.get_bit(i) {
                PieceType::Queen
            } else if board.kings.get_bit(i) {
                PieceType::King
            } else {
                continue;
            };

            match piece_type {
                PieceType::Pawn => Self::generate_pawn_moves(board, &coord, player, &mut moves),
                PieceType::Knight => Self::generate_leaper_moves(
                    board,
                    &coord,
                    player,
                    &Self::get_knight_offsets(board.dimension),
                    &mut moves,
                ),
                PieceType::King => Self::generate_leaper_moves(
                    board,
                    &coord,
                    player,
                    &Self::get_king_offsets(board.dimension),
                    &mut moves,
                ),
                PieceType::Rook => Self::generate_slider_moves_bitwise(
                    board,
                    &coord,
                    player,
                    &Self::get_rook_directions(board.dimension),
                    &mut moves,
                ),
                PieceType::Bishop => Self::generate_slider_moves_bitwise(
                    board,
                    &coord,
                    player,
                    &Self::get_bishop_directions(board.dimension),
                    &mut moves,
                ),
                PieceType::Queen => {
                    Self::generate_slider_moves_bitwise(
                        board,
                        &coord,
                        player,
                        &Self::get_rook_directions(board.dimension),
                        &mut moves,
                    );
                    Self::generate_slider_moves_bitwise(
                        board,
                        &coord,
                        player,
                        &Self::get_bishop_directions(board.dimension),
                        &mut moves,
                    );
                }
            }
        }
        moves
    }

    fn generate_slider_moves_bitwise(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        directions: &[Vec<isize>],
        moves: &mut MoveList,
    ) {
        let origin_idx = board.coords_to_index(&origin.values).unwrap();

        let mut generator = board.white_occupancy.zero_like();
        generator.set_bit(origin_idx);

        let all_occupancy = &board.white_occupancy | &board.black_occupancy;

        let empty = match all_occupancy {
            BitBoard::Small(b) => {
                let mask =
                    (1u32.checked_shl(board.total_cells as u32).unwrap_or(0)).wrapping_sub(1);
                BitBoard::Small((!b) & mask)
            }
            BitBoard::Medium(b) => {
                let mask =
                    (1u128.checked_shl(board.total_cells as u32).unwrap_or(0)).wrapping_sub(1);
                BitBoard::Medium((!b) & mask)
            }
            BitBoard::Large { data } => {
                let mut new_data = Vec::with_capacity(data.len());
                let mut remaining = board.total_cells;
                for val in data {
                    let limit = std::cmp::min(64, remaining);
                    let mask = if limit == 64 {
                        !0u64
                    } else {
                        (1u64 << limit) - 1
                    };
                    new_data.push((!val) & mask);
                    remaining = remaining.saturating_sub(64);
                }
                BitBoard::Large { data: new_data }
            }
        };

        let own_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for dir in directions {
            let stride = Self::calculate_stride(board, dir);
            if stride == 0 {
                continue;
            }

            let attacks = Self::kogge_stone_fill(&generator, &empty, stride, board, dir);

            let valid_moves_bb = &attacks & &(!own_occupancy);

            for to_idx in valid_moves_bb.iter_indices() {
                if to_idx == origin_idx {
                    continue;
                }

                let to_coords = board.index_to_coords(to_idx);
                moves.push(Move {
                    from: origin.clone(),
                    to: Coordinate::new(to_coords),
                    promotion: None,
                });
            }
        }
    }

    fn kogge_stone_fill(
        generator: &BitBoard,
        prop: &BitBoard,
        stride: isize,
        board: &Board,
        direction: &[isize],
    ) -> BitBoard {
        let mut g = generator.clone();
        let mut p = prop.clone();

        let steps = 7;

        let mut shift_amt = 1;
        for _ in 0..steps {
            let mask = Self::get_validity_mask(board, direction, shift_amt);

            let g_masked = &g & &mask;
            let shifted_g = if stride > 0 {
                &g_masked << (stride.abs() as usize * shift_amt)
            } else {
                &g_masked >> (stride.abs() as usize * shift_amt)
            };

            let p_masked = &p & &mask;
            let shifted_p = if stride > 0 {
                &p_masked << (stride.abs() as usize * shift_amt)
            } else {
                &p_masked >> (stride.abs() as usize * shift_amt)
            };

            g = &g | &(&shifted_g & &p);
            p = &p & &shifted_p;

            shift_amt *= 2;
        }

        let mask = Self::get_validity_mask(board, direction, 1);
        let g_masked = &g & &mask;

        let attacks = if stride > 0 {
            &g_masked << stride.abs() as usize
        } else {
            &g_masked >> stride.abs() as usize
        };

        attacks
    }

    fn calculate_stride(board: &Board, dir: &[isize]) -> isize {
        let mut stride = 0;
        let mut multiplier = 1;
        for i in 0..board.dimension {
            stride += dir[i] * multiplier as isize;
            multiplier *= board.side;
        }
        stride
    }

    fn get_validity_mask(board: &Board, dir: &[isize], steps: usize) -> BitBoard {
        let mut mask = board.white_occupancy.zero_like();

        for i in 0..board.total_cells() {
            let coords = board.index_to_coords(i);

            let mut valid = true;
            for (c, &d) in coords.iter().zip(dir.iter()) {
                let res = *c as isize + (d * steps as isize);
                if res < 0 || res >= board.side as isize {
                    valid = false;
                    break;
                }
            }

            if valid {
                mask.set_bit(i);
            }
        }
        mask
    }

    fn leaves_king_in_check(board: &mut Board, player: Player, mv: &Move) -> bool {
        let info = match board.apply_move(mv) {
            Ok(i) => i,
            Err(_) => return true,
        };

        let in_check = if let Some(king_pos) = board.get_king_coordinate(player) {
            Self::is_square_attacked(board, &king_pos, player.opponent())
        } else {
            false
        };

        board.unmake_move(mv, info);
        in_check
    }

    fn generate_castling_moves(board: &Board, player: Player, moves: &mut MoveList) {
        if board.side != 8 {
            return;
        }

        let (rights_mask, rank) = match player {
            Player::White => (0x3, 0),
            Player::Black => (0xC, board.side - 1),
        };

        let my_rights = board.castling_rights & rights_mask;
        if my_rights == 0 {
            return;
        }

        let king_file = 4;
        let mut king_coords = vec![rank; board.dimension];
        king_coords[1] = king_file;
        let king_coord = Coordinate::new(king_coords.clone());

        if Self::is_square_attacked(board, &king_coord, player.opponent()) {
            return;
        }

        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        let ks_mask = match player {
            Player::White => 0x1,
            Player::Black => 0x4,
        };
        if (my_rights & ks_mask) != 0 {
            let f_file = 5;
            let g_file = 6;

            let mut f_coords = king_coords.clone();
            f_coords[1] = f_file;
            let mut g_coords = king_coords.clone();
            g_coords[1] = g_file;

            let f_idx = board.coords_to_index(&f_coords);
            let g_idx = board.coords_to_index(&g_coords);

            let mut blocked = true;
            if let (Some(fi), Some(gi)) = (f_idx, g_idx) {
                if !all_occupancy.get_bit(fi) && !all_occupancy.get_bit(gi) {
                    blocked = false;
                }
            }

            if !blocked {
                if !Self::is_square_attacked(board, &Coordinate::new(f_coords), player.opponent())
                    && !Self::is_square_attacked(
                        board,
                        &Coordinate::new(g_coords.clone()),
                        player.opponent(),
                    )
                {
                    moves.push(Move {
                        from: king_coord.clone(),
                        to: Coordinate::new(g_coords),
                        promotion: None,
                    });
                }
            }
        }

        let qs_mask = match player {
            Player::White => 0x2,
            Player::Black => 0x8,
        };
        if (my_rights & qs_mask) != 0 {
            let b_file = 1;
            let c_file = 2;
            let d_file = 3;

            let mut b_coords = king_coords.clone();
            b_coords[1] = b_file;
            let mut c_coords = king_coords.clone();
            c_coords[1] = c_file;
            let mut d_coords = king_coords.clone();
            d_coords[1] = d_file;

            let b_idx = board.coords_to_index(&b_coords);
            let c_idx = board.coords_to_index(&c_coords);
            let d_idx = board.coords_to_index(&d_coords);

            let mut blocked = true;
            if let (Some(bi), Some(ci), Some(di)) = (b_idx, c_idx, d_idx) {
                if !all_occupancy.get_bit(bi)
                    && !all_occupancy.get_bit(ci)
                    && !all_occupancy.get_bit(di)
                {
                    blocked = false;
                }
            }

            if !blocked {
                if !Self::is_square_attacked(board, &Coordinate::new(d_coords), player.opponent())
                    && !Self::is_square_attacked(
                        board,
                        &Coordinate::new(c_coords.clone()),
                        player.opponent(),
                    )
                {
                    moves.push(Move {
                        from: king_coord.clone(),
                        to: Coordinate::new(c_coords),
                        promotion: None,
                    });
                }
            }
        }
    }

    pub fn get_rook_directions(dimension: usize) -> Vec<Vec<isize>> {
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

    pub fn get_bishop_directions(dimension: usize) -> Vec<Vec<isize>> {
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

    pub fn get_knight_offsets(dimension: usize) -> Vec<Vec<isize>> {
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

    pub fn get_king_offsets(dimension: usize) -> Vec<Vec<isize>> {
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

    pub fn get_pawn_capture_offsets_for_target(
        dimension: usize,
        attacker: Player,
    ) -> Vec<Vec<isize>> {
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

    pub fn apply_offset(
        coords: &[usize],
        offset: &[isize],
        side: usize,
    ) -> Option<SmallVec<[usize; 4]>> {
        let mut new_coords = SmallVec::with_capacity(coords.len());
        for (c, &o) in coords.iter().zip(offset.iter()) {
            let val = *c as isize + o;
            if val < 0 || val >= side as isize {
                return None;
            }
            new_coords.push(val as usize);
        }
        Some(new_coords)
    }

    fn generate_leaper_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        offsets: &[Vec<isize>],
        moves: &mut MoveList,
    ) {
        let same_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for offset in offsets {
            if let Some(target_coords) = Self::apply_offset(&origin.values, offset, board.side) {
                if let Some(target_idx) = board.coords_to_index(&target_coords) {
                    if !same_occupancy.get_bit(target_idx) {
                        moves.push(Move {
                            from: origin.clone(),
                            to: Coordinate::new(target_coords),
                            promotion: None,
                        });
                    }
                }
            }
        }
    }

    fn generate_pawn_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        moves: &mut MoveList,
    ) {
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        let enemy_occupancy = match player.opponent() {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for movement_axis in 0..board.dimension {
            if movement_axis == 1 {
                continue;
            }

            let forward_dir = match player {
                Player::White => 1,
                Player::Black => -1,
            };

            let mut forward_step = vec![0; board.dimension];
            forward_step[movement_axis] = forward_dir;

            if let Some(target) = Self::apply_offset(&origin.values, &forward_step, board.side) {
                if let Some(idx) = board.coords_to_index(&target) {
                    if !all_occupancy.get_bit(idx) {
                        Self::add_pawn_move(origin, &target, board.side, player, moves);

                        let is_start_rank = match player {
                            Player::White => origin.values[movement_axis] == 1,
                            Player::Black => origin.values[movement_axis] == board.side - 2,
                        };

                        if is_start_rank {
                            if let Some(target2) =
                                Self::apply_offset(&target, &forward_step, board.side)
                            {
                                if let Some(idx2) = board.coords_to_index(&target2) {
                                    if !all_occupancy.get_bit(idx2) {
                                        Self::add_pawn_move(
                                            origin, &target2, board.side, player, moves,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            for capture_axis in 0..board.dimension {
                if capture_axis == movement_axis {
                    continue;
                }
                for s in [-1, 1] {
                    let mut cap_step = forward_step.clone();
                    cap_step[capture_axis] = s;

                    if let Some(target) = Self::apply_offset(&origin.values, &cap_step, board.side)
                    {
                        if let Some(idx) = board.coords_to_index(&target) {
                            if enemy_occupancy.get_bit(idx) {
                                Self::add_pawn_move(origin, &target, board.side, player, moves);
                            } else if let Some((ep_target, _)) = board.en_passant_target {
                                if idx == ep_target {
                                    moves.push(Move {
                                        from: origin.clone(),
                                        to: Coordinate::new(target),
                                        promotion: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn add_pawn_move(
        from: &Coordinate,
        to_vals: &[usize],
        side: usize,
        player: Player,
        moves: &mut MoveList,
    ) {
        let is_promotion = (0..to_vals.len()).all(|i| {
            if i == 1 {
                true
            } else {
                match player {
                    Player::White => to_vals[i] == side - 1,
                    Player::Black => to_vals[i] == 0,
                }
            }
        });

        let to = Coordinate::new(to_vals.to_vec());

        if is_promotion {
            for t in [
                PieceType::Queen,
                PieceType::Rook,
                PieceType::Bishop,
                PieceType::Knight,
            ] {
                moves.push(Move {
                    from: from.clone(),
                    to: to.clone(),
                    promotion: Some(t),
                });
            }
        } else {
            moves.push(Move {
                from: from.clone(),
                to,
                promotion: None,
            });
        }
    }
}
