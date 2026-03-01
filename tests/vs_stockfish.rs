use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use hyperchess::config::AppConfig;
use hyperchess::domain::board::Board;
use hyperchess::domain::coordinate::Coordinate;
use hyperchess::domain::models::{Move, PieceType, Player};
use hyperchess::domain::rules::Rules;
use hyperchess::domain::services::PlayerStrategy;
use hyperchess::infrastructure::ai::MinimaxBot;
use hyperchess::infrastructure::ai::eval::Evaluator;
use hyperchess::infrastructure::display::render_board;

/// Convert an internal Coordinate (2D, 0-indexed [rank, file]) to UCI square string.
/// rank=0 file=0 → "a1", rank=1 file=4 → "e2", etc.
fn coord_to_uci(c: &Coordinate) -> String {
    let file = c.values[1]; // 0=a, 1=b, ..., 7=h
    let rank = c.values[0]; // 0-indexed rank
    format!("{}{}", (b'a' + file) as char, rank + 1)
}

/// Convert an internal Move to UCI notation (e.g. "e2e4", "e7e8q").
fn move_to_uci(mv: &Move) -> String {
    let mut s = format!("{}{}", coord_to_uci(&mv.from), coord_to_uci(&mv.to));
    if let Some(promo) = mv.promotion {
        s.push(match promo {
            PieceType::Queen => 'q',
            PieceType::Rook => 'r',
            PieceType::Bishop => 'b',
            PieceType::Knight => 'n',
            _ => unreachable!(),
        });
    }
    s
}

/// Parse a UCI square string to a Coordinate.
fn uci_square_to_coord(s: &str) -> Coordinate {
    let bytes = s.as_bytes();
    let file = bytes[0] - b'a';
    let rank = bytes[1] - b'1';
    Coordinate::new(vec![rank, file])
}

/// Parse a UCI move string (e.g. "e2e4", "e7e8q") to an internal Move.
fn uci_to_move(s: &str) -> Move {
    let s = s.trim();
    let from = uci_square_to_coord(&s[0..2]);
    let to = uci_square_to_coord(&s[2..4]);
    let promotion = if s.len() > 4 {
        Some(match s.as_bytes()[4] {
            b'q' => PieceType::Queen,
            b'r' => PieceType::Rook,
            b'b' => PieceType::Bishop,
            b'n' => PieceType::Knight,
            _ => PieceType::Queen,
        })
    } else {
        None
    };
    Move { from, to, promotion }
}

struct Stockfish {
    child: std::process::Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl Stockfish {
    fn new(skill_level: u8) -> Self {
        let mut child = Command::new("stockfish")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to launch stockfish — is it installed?");

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        let mut sf = Stockfish { child, reader };

        sf.send("uci");
        sf.wait_for("uciok");

        // Max difficulty: skill level 20, max hash, max threads
        sf.send(&format!("setoption name Skill Level value {}", skill_level));
        sf.send("setoption name Hash value 256");
        sf.send("setoption name Threads value 4");
        sf.send("isready");
        sf.wait_for("readyok");
        sf.send("ucinewgame");
        sf.send("isready");
        sf.wait_for("readyok");

        sf
    }

    fn send(&mut self, cmd: &str) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}", cmd).unwrap();
        stdin.flush().unwrap();
    }

    fn wait_for(&mut self, token: &str) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            self.reader.read_line(&mut line).unwrap();
            if line.trim().starts_with(token) || line.contains(token) {
                return line;
            }
        }
    }

    /// Ask Stockfish for its best move given the move history.
    fn best_move(&mut self, moves: &[String], move_time_ms: u64) -> String {
        if moves.is_empty() {
            self.send("position startpos");
        } else {
            self.send(&format!(
                "position startpos moves {}",
                moves.join(" ")
            ));
        }
        self.send(&format!("go movetime {}", move_time_ms));

        // Read lines until we get "bestmove ..."
        loop {
            let line = self.read_line();
            if line.starts_with("bestmove") {
                let best = line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("(none)")
                    .to_string();
                return best;
            }
        }
    }

    fn read_line(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        line.trim().to_string()
    }
}

impl Drop for Stockfish {
    fn drop(&mut self) {
        let _ = self.send_quiet("quit");
        let _ = self.child.wait();
    }
}

impl Stockfish {
    fn send_quiet(&mut self, cmd: &str) -> std::io::Result<()> {
        if let Some(stdin) = self.child.stdin.as_mut() {
            writeln!(stdin, "{}", cmd)?;
            stdin.flush()?;
        }
        Ok(())
    }
}

/// Format eval score in pawns (positive = white advantage).
fn format_eval(cp: i32) -> String {
    let pawns = cp as f64 / 100.0;
    if pawns >= 0.0 {
        format!("+{:.2}", pawns)
    } else {
        format!("{:.2}", pawns)
    }
}

/// Print the board, the move list so far, and the eval bar.
fn print_position(board: &Board, move_number: usize, last_white: &str, last_black: &str) {
    // Clear screen for a clean redraw
    print!("\x1b[2J\x1b[H");

    println!("\x1b[1;36m╔══════════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;36m║  HyperChess (White)  vs  Stockfish 20 (Black)   ║\x1b[0m");
    println!("\x1b[1;36m╚══════════════════════════════════════════════════╝\x1b[0m");
    println!();

    // Render the board
    let rendered = render_board(board);
    println!("{}", rendered);

    // Eval
    let eval = Evaluator::evaluate(board);
    let bar = eval_bar(eval);
    println!("  Eval: {} {}",  format_eval(eval), bar);
    println!();

    // Last move info
    println!(
        "  Move {}: \x1b[1;37m{}\x1b[0m  \x1b[1;31m{}\x1b[0m",
        move_number, last_white, last_black
    );
    println!();
}

/// Render an ASCII eval bar. ±5 pawn scale so small edges are visible.
/// Full white = White winning, full black = Black winning.
fn eval_bar(cp: i32) -> String {
    let pawns = (cp as f64 / 100.0).clamp(-5.0, 5.0);
    // Map [-5, +5] to [0, bar_width]. Center = equal.
    let bar_width = 30;
    let fill = ((pawns + 5.0) / 10.0 * bar_width as f64) as usize;

    let mut bar = String::with_capacity(bar_width + 20);
    // White portion (filled) then black portion (empty)
    for i in 0..bar_width {
        if i < fill {
            bar.push('█');
        } else {
            bar.push('░');
        }
    }

    let verdict = if pawns.abs() < 0.3 {
        "\x1b[90m= Equal\x1b[0m"
    } else if pawns > 0.0 && pawns < 1.0 {
        "\x1b[37mWhite slightly better\x1b[0m"
    } else if pawns < 0.0 && pawns > -1.0 {
        "\x1b[31mBlack slightly better\x1b[0m"
    } else if pawns >= 1.0 && pawns < 3.0 {
        "\x1b[1;37mWhite is winning\x1b[0m"
    } else if pawns <= -1.0 && pawns > -3.0 {
        "\x1b[1;31mBlack is winning\x1b[0m"
    } else if pawns >= 3.0 {
        "\x1b[1;32mWhite is crushing\x1b[0m"
    } else {
        "\x1b[1;31mBlack is crushing\x1b[0m"
    };

    format!(
        "\x1b[1;37mW\x1b[0m [{bar}] \x1b[1;31mB\x1b[0m  {verdict}",
    )
}

/// Validate that a UCI move from Stockfish is legal on our board.
fn validate_and_apply_sf_move(board: &mut Board, uci_mv: &str, player: Player) -> Move {
    let parsed = uci_to_move(uci_mv);
    let legal_moves = Rules::generate_legal_moves(board, player);

    // Find the matching legal move (handles promotion matching).
    let matched = legal_moves
        .iter()
        .find(|m| {
            m.from == parsed.from
                && m.to == parsed.to
                && m.promotion == parsed.promotion
        })
        .unwrap_or_else(|| {
            panic!(
                "Stockfish move {} is not legal on our board!\nLegal moves: {:?}",
                uci_mv,
                legal_moves.iter().map(|m| move_to_uci(m)).collect::<Vec<_>>()
            );
        })
        .clone();

    board.apply_move(&matched).unwrap();
    matched
}

#[test]
#[ignore] // Run with: cargo test vs_stockfish -- --ignored --nocapture
fn vs_stockfish_full_game() {
    // ── Config: crank HyperChess to its strongest ──
    let mut config = AppConfig::default();
    config.minimax.depth = 10;
    config.compute.minutes = 2;
    config.compute.concurrency = 4;
    config.compute.memory = 256;

    let mut board = Board::new(2, 8);
    let mut bot = MinimaxBot::new(&config, 2, 8);
    let mut sf = Stockfish::new(20); // Skill Level 20 = max

    let mut uci_history: Vec<String> = Vec::new();
    let mut pgn_moves: Vec<String> = Vec::new();
    let mut move_number = 1;
    let mut last_white = String::from("...");
    let mut last_black = String::from("...");
    let max_moves = 300; // Safety valve

    // Show initial position
    print_position(&board, 0, "—", "—");
    println!("  Game starting...\n");
    std::thread::sleep(std::time::Duration::from_secs(2));

    for half_move in 0..(max_moves * 2) {
        let player = if half_move % 2 == 0 {
            Player::White
        } else {
            Player::Black
        };

        // Check for draw by repetition
        if board.is_repetition() {
            print_position(&board, move_number, &last_white, &last_black);
            println!("  \x1b[1;33m½-½ Draw by repetition after {} moves.\x1b[0m", move_number);
            print_pgn(&pgn_moves);
            return;
        }

        // Check for no legal moves (checkmate or stalemate)
        let legal_moves = Rules::generate_legal_moves(&mut board, player);
        if legal_moves.is_empty() {
            let king_coord = board.get_king_coordinate(player);
            let in_check = king_coord
                .map(|k| Rules::is_square_attacked(&board, &k, player.opponent()))
                .unwrap_or(false);

            print_position(&board, move_number, &last_white, &last_black);

            if in_check {
                let winner = player.opponent();
                match winner {
                    Player::White => {
                        println!("  \x1b[1;32m1-0 CHECKMATE! HyperChess WINS after {} moves!\x1b[0m", move_number);
                    }
                    Player::Black => {
                        println!("  \x1b[1;31m0-1 CHECKMATE. Stockfish wins after {} moves.\x1b[0m", move_number);
                    }
                }
            } else {
                println!("  \x1b[1;33m½-½ STALEMATE after {} moves.\x1b[0m", move_number);
            }
            print_pgn(&pgn_moves);
            return;
        }

        // 50-move rule approximation
        if board.state.history.len() > 200 {
            print_position(&board, move_number, &last_white, &last_black);
            println!("  \x1b[1;33m½-½ Draw by 50-move rule ({} half-moves).\x1b[0m", board.state.history.len());
            print_pgn(&pgn_moves);
            return;
        }

        match player {
            Player::White => {
                let mv = bot
                    .get_move(&board, Player::White)
                    .expect("HyperChess has no move but legal_moves was non-empty");
                let uci = move_to_uci(&mv);
                last_white = uci.clone();

                board.apply_move(&mv).unwrap();
                uci_history.push(uci.clone());
                pgn_moves.push(format!("{}. {}", move_number, uci));
            }
            Player::Black => {
                let sf_uci = sf.best_move(&uci_history, 5000);
                if sf_uci == "(none)" {
                    print_position(&board, move_number, &last_white, &last_black);
                    println!("  \x1b[1;32m1-0 Stockfish has no move. HyperChess WINS!\x1b[0m");
                    print_pgn(&pgn_moves);
                    return;
                }
                last_black = sf_uci.clone();

                validate_and_apply_sf_move(&mut board, &sf_uci, Player::Black);
                uci_history.push(sf_uci.clone());
                pgn_moves.push(sf_uci);

                // Render after each full move
                print_position(&board, move_number, &last_white, &last_black);

                // Print compact move log
                let start = if pgn_moves.len() > 20 { pgn_moves.len() - 20 } else { 0 };
                print!("  Moves: ");
                for token in &pgn_moves[start..] {
                    print!("{} ", token);
                }
                println!();

                move_number += 1;
            }
        }
    }

    print_position(&board, move_number, &last_white, &last_black);
    println!("  \x1b[1;33m½-½ Draw by adjudication ({} moves).\x1b[0m", max_moves);
    print_pgn(&pgn_moves);
}

fn print_pgn(moves: &[String]) {
    println!("\n  \x1b[1;36m── PGN ──\x1b[0m");
    let mut line = String::from("  ");
    for token in moves {
        if line.len() + token.len() + 1 > 78 {
            println!("{}", line);
            line = String::from("  ");
        }
        line.push_str(token);
        line.push(' ');
    }
    if !line.trim().is_empty() {
        println!("{}", line);
    }
    println!();
}
