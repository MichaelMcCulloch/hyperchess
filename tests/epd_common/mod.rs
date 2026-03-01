use hyperchess::config::AppConfig;
use hyperchess::domain::board::Board;
use hyperchess::domain::board::san::parse_san;
use hyperchess::domain::models::Player;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;

pub struct EpdPosition {
    pub fen: String,
    pub best_moves: Vec<String>,
    pub avoid_moves: Vec<String>,
    pub id: String,
}

/// Parse an EPD line into its components.
///
/// EPD format: `<fen_4_fields> [opcode operand;]...`
/// Supported opcodes: `bm` (best move), `am` (avoid move), `id`.
pub fn parse_epd(line: &str) -> Option<EpdPosition> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // First 4 space-separated tokens are FEN fields
    let mut parts = line.splitn(5, ' ');
    let pieces = parts.next()?;
    let side = parts.next()?;
    let castling = parts.next()?;
    let ep = parts.next()?;
    let rest = parts.next().unwrap_or("");

    let fen = format!("{pieces} {side} {castling} {ep}");

    // Parse operations from the remainder (semicolon-separated)
    let mut best_moves = Vec::new();
    let mut avoid_moves = Vec::new();
    let mut id = String::new();

    // The rest may start with extra FEN-like fields (hmvc, fmvn) before operations,
    // or may jump straight into operations. We split by ';' and parse each segment.
    for segment in rest.split(';') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }

        if let Some(val) = segment
            .strip_prefix("bm ")
            .or_else(|| segment.strip_prefix("bm\t"))
        {
            // Best moves: space or comma separated SAN
            for mv_str in val.split([',', ' ']) {
                let mv = mv_str.trim();
                if !mv.is_empty() {
                    best_moves.push(mv.to_string());
                }
            }
        } else if let Some(val) = segment
            .strip_prefix("am ")
            .or_else(|| segment.strip_prefix("am\t"))
        {
            for mv_str in val.split([',', ' ']) {
                let mv = mv_str.trim();
                if !mv.is_empty() {
                    avoid_moves.push(mv.to_string());
                }
            }
        } else if let Some(val) = segment
            .strip_prefix("id ")
            .or_else(|| segment.strip_prefix("id\t"))
        {
            // Strip quotes
            id = val.trim_matches('"').trim().to_string();
        }
        // Ignore other opcodes (acd, acs, ce, pv, etc.)
    }

    Some(EpdPosition {
        fen,
        best_moves,
        avoid_moves,
        id,
    })
}

/// Run a single EPD position. Returns (passed, id, engine_move_san).
pub fn run_epd_position(epd_line: &str) -> (bool, String, String) {
    let pos = match parse_epd(epd_line) {
        Some(p) => p,
        None => return (false, String::new(), String::new()),
    };

    // EPD has 4 fields; add "0 1" for halfmove/fullmove to make valid FEN
    let full_fen = format!("{} 0 1", pos.fen);
    let mut board = match Board::from_fen(&full_fen) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[ERROR] {} — FEN parse error: {}", pos.id, e);
            return (false, pos.id, format!("FEN error: {e}"));
        }
    };

    // Determine side to move
    let player = if pos.fen.split_whitespace().nth(1) == Some("w") {
        Player::White
    } else {
        Player::Black
    };

    // Configure engine: 30 threads, 4 GiB TT, 2 minutes, depth 10
    let mut config = AppConfig::default();
    config.minimax.depth = 10;
    config.compute.minutes = 2;
    config.compute.concurrency = 30;
    config.compute.memory = 4096;

    let mut bot = MinimaxBot::new(&config, 2, 8);
    let chosen = match bot.get_move(&board, player) {
        Some(mv) => mv,
        None => {
            eprintln!("[ERROR] {} — engine returned no move", pos.id);
            return (false, pos.id, "no move".to_string());
        }
    };

    let chosen_uci = format!(
        "{}{}{}{}",
        (b'a' + chosen.from.values[1]) as char,
        chosen.from.values[0] + 1,
        (b'a' + chosen.to.values[1]) as char,
        chosen.to.values[0] + 1,
    );

    // Check best moves (bm)
    if !pos.best_moves.is_empty() {
        for bm_san in &pos.best_moves {
            match parse_san(&mut board, player, bm_san) {
                Ok(expected) => {
                    if chosen.from == expected.from
                        && chosen.to == expected.to
                        && chosen.promotion == expected.promotion
                    {
                        return (true, pos.id, chosen_uci);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[WARN] {} — could not parse SAN '{}': {}",
                        pos.id, bm_san, e
                    );
                }
            }
        }
        return (false, pos.id, chosen_uci);
    }

    // Check avoid moves (am) — pass if engine does NOT play the avoided move
    if !pos.avoid_moves.is_empty() {
        for am_san in &pos.avoid_moves {
            match parse_san(&mut board, player, am_san) {
                Ok(avoided) => {
                    if chosen.from == avoided.from
                        && chosen.to == avoided.to
                        && chosen.promotion == avoided.promotion
                    {
                        return (false, pos.id, chosen_uci);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[WARN] {} — could not parse SAN '{}': {}",
                        pos.id, am_san, e
                    );
                }
            }
        }
        return (true, pos.id, chosen_uci);
    }

    // No bm or am — can't verify
    eprintln!("[WARN] {} — no bm or am to check", pos.id);
    (false, pos.id, chosen_uci)
}

/// Run an entire EPD suite. Returns (passed, total).
pub fn run_epd_suite(data: &str, suite_name: &str) -> (usize, usize) {
    let mut passed = 0usize;
    let mut total = 0usize;

    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        total += 1;
        let (ok, id, engine_move) = run_epd_position(line);
        if ok {
            passed += 1;
            println!("  [PASS] {id} — engine played {engine_move}");
        } else {
            println!("  [FAIL] {id} — engine played {engine_move}");
        }
    }

    println!();
    println!(
        "  {suite_name} Score: {passed}/{total} ({:.1}%)",
        100.0 * passed as f64 / total.max(1) as f64
    );

    (passed, total)
}
