use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{AppConfig, MctsConfig};
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::domain::services::PlayerStrategy;
use crate::infrastructure::ai::mcts::MCTS;
use crate::infrastructure::ai::transposition::LockFreeTT;

pub struct MctsBot {
    config: MctsConfig,
    time_limit: Duration,
    tt: Arc<LockFreeTT>,
    stop_flag: Arc<AtomicBool>,
    nodes_searched: Arc<AtomicUsize>,
    num_threads: usize,
}

impl MctsBot {
    pub fn new(config: &AppConfig, time_limit_ms: u64) -> Self {
        let mcts_config = config.mcts.clone().unwrap_or_else(|| MctsConfig {
            iterations: 1000,
            depth: 50,
            iter_per_thread: 10.0,
            prior_weight: 1.414,
            rollout_depth: 0,
        });

        Self {
            config: mcts_config,
            time_limit: Duration::from_millis(time_limit_ms),
            tt: Arc::new(LockFreeTT::new(config.compute.memory)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            nodes_searched: Arc::new(AtomicUsize::new(0)),
            num_threads: std::thread::available_parallelism()
                .map(|n| n.get().saturating_sub(2).max(1))
                .unwrap_or(1),
        }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.num_threads = concurrency;
        self
    }
}

impl PlayerStrategy for MctsBot {
    fn get_move(&mut self, board: &Board, player: Player) -> Option<Move> {
        self.nodes_searched.store(0, Ordering::Relaxed);
        self.stop_flag.store(false, Ordering::Relaxed);

        let start_time = Instant::now();
        let stop_flag = self.stop_flag.clone();
        let nodes_counter = self.nodes_searched.clone();
        let time_limit = self.time_limit;

        let search_active = Arc::new(AtomicBool::new(true));
        let search_active_clone = search_active.clone();

        thread::spawn(move || {
            let mut last_nodes = 0;
            let mut last_time = Instant::now();

            while search_active_clone.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(50));

                if start_time.elapsed() > time_limit {
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }

                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                let current_nodes = nodes_counter.load(Ordering::Relaxed);
                let now = Instant::now();
                let duration = now.duration_since(last_time).as_secs_f64();

                if duration > 1.0 {
                    let nps = (current_nodes - last_nodes) as f64 / duration;
                    let nps_fmt = if nps > 1_000_000.0 {
                        format!("{:.2} MN/s", nps / 1_000_000.0)
                    } else {
                        format!("{:.2} kN/s", nps / 1_000.0)
                    };

                    print!(
                        "\rinfo nodes {} nps {} time {:.1}s  ",
                        current_nodes,
                        nps_fmt,
                        start_time.elapsed().as_secs_f32()
                    );
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();

                    last_nodes = current_nodes;
                    last_time = now;
                }
            }
        });

        let mut mcts = MCTS::new(
            board,
            player,
            Some(self.tt.clone()),
            Some(self.config.clone()),
            Some(self.stop_flag.clone()),
            Some(self.nodes_searched.clone()),
            self.config.rollout_depth,
        )
        .with_concurrency(self.num_threads);

        let (_win_rate, best_move) = mcts.run(board, self.config.iterations);

        search_active.store(false, Ordering::Relaxed);
        println!();

        best_move
    }
}
