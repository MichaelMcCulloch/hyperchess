use crate::domain::board::Board;
use crate::domain::coordinate::Coordinate;
use crate::domain::models::{Move, PieceType, Player};

pub struct Rules;

impl Rules {
    pub fn generate_legal_moves(board: &Board, player: Player) -> Vec<Move> {
        let mut moves = Vec::new();
        let pseudo_legal = Self::generate_pseudo_legal_moves(board, player);

        for mv in pseudo_legal {
            if !Self::leaves_king_in_check(board, player, &mv) {
                moves.push(mv);
            }
        }
        moves
    }

    pub fn is_square_attacked(board: &Board, square: &Coordinate, by_player: Player) -> bool {
        // To check if a square is attacked by `by_player`, we can pretend there is a piece on `square`
        // and see if it can "capture" a piece of `by_player` using the movement rules of that piece.
        // E.g. if a Knight on `square` can jump to a square occupied by an enemy Knight, then `square` is attacked by that enemy Knight.
        // NOTE: We rely on board.get_index / coords_to_index which are now internal or public?
        // We made `coords_to_index` public in Board.

        let dimension = board.dimension;
        let side = board.side;
        // Accessing helper on board instance
        let _index = if let Some(idx) = board.coords_to_index(&square.values) {
            idx
        } else {
            return false;
        };

        let enemy_occupancy = match by_player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        // 1. Check Leapers (Knights, Kings)
        // Check Knight attacks
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

        // Check King attacks (useful for validation, though kings can't really attack to checkmate)
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

        // 2. Check Rays (Rook, Bishop, Queen)
        // Reverse raycast: Look outwards from `square`. If first piece hit is enemy slider of relevant type, then attacked.

        // Rook vectors
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

        // Bishop vectors
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

        // 3. Check Pawns
        // Pawns attack "Forward" + "Sideways".
        // Inverse: Check if there is an enemy pawn that can capture `square`.
        // Enemy pawn moves "Forward" (relative to enemy).
        // So we look "Backward" relative to enemy from `square`.

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

    fn scan_ray_for_threat(
        board: &Board,
        origin_vals: &[usize],
        direction: &[isize],
        attacker: Player,
        threat_types: &[PieceType],
    ) -> bool {
        // We are at `origin_vals` (which is empty or the target square).
        // We look OUTWARD in `direction`.
        // If we hit an enemy piece of `threat_types`, return true.
        // If we hit any other piece (own or enemy non-threat), return false (blocked).

        let mut current = origin_vals.to_vec();
        let enemy_occupancy = match attacker {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        // Own occupancy relative to the square being attacked?
        // No, 'own' relative to the attacker is 'enemy' relative to the square?
        // Wait. `by_player` is the ATTACKER.
        // So `enemy_occupancy` is the ATTACKER's pieces.
        // `own_occupancy` is the DEFENDER's pieces (or Empty).
        // Any piece blocks the ray.

        // Actually simpler: Just check ALL occupancy.
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        loop {
            if let Some(next) = Self::apply_offset(&current, direction, board.side) {
                if let Some(idx) = board.coords_to_index(&next) {
                    if all_occupancy.get_bit(idx) {
                        // Hit a piece. Is it an enemy threat?
                        if enemy_occupancy.get_bit(idx) {
                            // It is an enemy piece. Check type.
                            // We need to check if it is one of the threat_types.

                            // Optimization: The caller passes specific threat types (e.g. Rook+Queen).
                            // But board bitboards are separated.
                            // Let's iterate threat types passed.
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
                            // If we hit an enemy piece but it's not in the threat list (e.g. a pawn blocking a rook),
                            // then it blocks the view.
                            return false;
                        } else {
                            // Hit own piece (Defender's piece), blocks view.
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

    // --- Internal Helpers ---

    fn generate_pseudo_legal_moves(board: &Board, player: Player) -> Vec<Move> {
        let mut moves = Vec::new();
        // Iterate all cells, find pieces owned by player
        for i in 0..board.total_cells {
            let occupancy = match player {
                Player::White => &board.white_occupancy,
                Player::Black => &board.black_occupancy,
            };

            if occupancy.get_bit(i) {
                let coord_vals = board.index_to_coords(i);
                let coord = Coordinate::new(coord_vals.clone());

                // Identify piece type
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
                    continue; // Error?
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
                    PieceType::Rook => Self::generate_slider_moves(
                        board,
                        &coord,
                        player,
                        &Self::get_rook_directions(board.dimension),
                        &mut moves,
                    ),
                    PieceType::Bishop => Self::generate_slider_moves(
                        board,
                        &coord,
                        player,
                        &Self::get_bishop_directions(board.dimension),
                        &mut moves,
                    ),
                    PieceType::Queen => {
                        Self::generate_slider_moves(
                            board,
                            &coord,
                            player,
                            &Self::get_rook_directions(board.dimension),
                            &mut moves,
                        );
                        Self::generate_slider_moves(
                            board,
                            &coord,
                            player,
                            &Self::get_bishop_directions(board.dimension),
                            &mut moves,
                        );
                    }
                }
            }
        }
        moves
    }

    fn leaves_king_in_check(board: &Board, player: Player, mv: &Move) -> bool {
        // Clone board, apply move, check if king is attacked
        // This is expensive. Future optimization: Incremental update or specialized check.
        let mut temp_board = board.clone();
        if let Err(_) = temp_board.apply_move(mv) {
            return true; // Illegal move invocation
        }

        if let Some(king_pos) = temp_board.get_king_coordinate(player) {
            Self::is_square_attacked(&temp_board, &king_pos, player.opponent())
        } else {
            // No king? For testing (sandbox), assume safe.
            false
        }
    }

    // Geometry Generators

    fn get_rook_directions(dimension: usize) -> Vec<Vec<isize>> {
        let mut dirs = Vec::new();
        // Just one non-zero component, +/- 1
        for i in 0..dimension {
            let mut v = vec![0; dimension];
            v[i] = 1;
            dirs.push(v.clone());
            v[i] = -1;
            dirs.push(v);
        }
        dirs
    }

    fn get_bishop_directions(dimension: usize) -> Vec<Vec<isize>> {
        // Even number of non-zero elements (user spec).
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

    fn get_knight_offsets(dimension: usize) -> Vec<Vec<isize>> {
        // Permutations of (+/- 2, +/- 1, 0...)
        // We need exactly one '2' and one '1', rest 0.
        let mut offsets = Vec::new();

        // This is a bit tricky to generate generically for N dimensions.
        // Iterate all pairs of axes.
        for i in 0..dimension {
            for j in 0..dimension {
                if i == j {
                    continue;
                }

                // +/- 2 on axis i, +/- 1 on axis j
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

    fn get_king_offsets(dimension: usize) -> Vec<Vec<isize>> {
        // Chebyshev 1. All 3^N - 1 neighbors.
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

    fn get_pawn_capture_offsets_for_target(dimension: usize, attacker: Player) -> Vec<Vec<isize>> {
        // If 'attacker' is White, they move +1 on axis 0 (forward).
        // Captures are +1 on axis 0 AND +/- 1 on exactly ONE other axis.
        // So we want to find where an Attacker could be relative to the Target.
        // Target = Attacker + Move.
        // Attacker = Target - Move.

        let direction = match attacker {
            Player::White => -1, // Look back
            Player::Black => 1,
        };

        let mut offsets = Vec::new();
        // Axis 0 is forward.
        // For each other dimension, allow +/- 1.
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

    // Move Generation Logic implementation

    fn apply_offset(coords: &[usize], offset: &[isize], side: usize) -> Option<Vec<usize>> {
        let mut new_coords = Vec::with_capacity(coords.len());
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
        moves: &mut Vec<Move>,
    ) {
        let same_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for offset in offsets {
            if let Some(target_coords) = Self::apply_offset(&origin.values, offset, board.side) {
                if let Some(target_idx) = board.coords_to_index(&target_coords) {
                    if !same_occupancy.get_bit(target_idx) {
                        // Empty or Enemy -> Legal
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

    fn generate_slider_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        directions: &[Vec<isize>],
        moves: &mut Vec<Move>,
    ) {
        let own_occupancy = match player {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        let enemy_occupancy = match player.opponent() {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };

        for dir in directions {
            let mut current = origin.values.clone();
            loop {
                if let Some(next) = Self::apply_offset(&current, dir, board.side) {
                    if let Some(idx) = board.coords_to_index(&next) {
                        if own_occupancy.get_bit(idx) {
                            break; // Blocked by own piece
                        }

                        moves.push(Move {
                            from: origin.clone(),
                            to: Coordinate::new(next.clone()),
                            promotion: None,
                        });

                        if enemy_occupancy.get_bit(idx) {
                            break; // Capture, then stop
                        }

                        current = next;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    fn generate_pawn_moves(
        board: &Board,
        origin: &Coordinate,
        player: Player,
        moves: &mut Vec<Move>,
    ) {
        let forward_dir = match player {
            Player::White => 1,
            Player::Black => -1,
        };

        let enemy_occupancy = match player.opponent() {
            Player::White => &board.white_occupancy,
            Player::Black => &board.black_occupancy,
        };
        // Just checking occupancy generically
        let all_occupancy = board
            .white_occupancy
            .clone()
            .or_with(&board.black_occupancy);

        // 1. One step forward
        let mut forward_step = vec![0; board.dimension];
        forward_step[0] = forward_dir;

        if let Some(target) = Self::apply_offset(&origin.values, &forward_step, board.side) {
            if let Some(idx) = board.coords_to_index(&target) {
                if !all_occupancy.get_bit(idx) {
                    // Must be empty
                    Self::add_pawn_move(
                        origin,
                        &target,
                        board.dimension,
                        board.side,
                        player,
                        moves,
                    );

                    // 2. Double step?
                    let is_start_rank = match player {
                        Player::White => origin.values[0] == 1,
                        Player::Black => origin.values[0] == board.side - 2,
                    };

                    if is_start_rank {
                        if let Some(target2) =
                            Self::apply_offset(&target, &forward_step, board.side)
                        {
                            if let Some(idx2) = board.coords_to_index(&target2) {
                                if !all_occupancy.get_bit(idx2) {
                                    Self::add_pawn_move(
                                        origin,
                                        &target2,
                                        board.dimension,
                                        board.side,
                                        player,
                                        moves,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Captures
        // +/- 1 on any other axis combined with forward step
        for i in 1..board.dimension {
            for s in [-1, 1] {
                let mut cap_step = forward_step.clone();
                cap_step[i] = s;
                if let Some(target) = Self::apply_offset(&origin.values, &cap_step, board.side) {
                    if let Some(idx) = board.coords_to_index(&target) {
                        if enemy_occupancy.get_bit(idx) {
                            Self::add_pawn_move(
                                origin,
                                &target,
                                board.dimension,
                                board.side,
                                player,
                                moves,
                            );
                        }
                    }
                }
            }
        }
    }

    fn add_pawn_move(
        from: &Coordinate,
        to_vals: &[usize],
        _dimension: usize,
        side: usize,
        player: Player,
        moves: &mut Vec<Move>,
    ) {
        let is_promotion = match player {
            Player::White => to_vals[0] == side - 1,
            Player::Black => to_vals[0] == 0,
        };

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
