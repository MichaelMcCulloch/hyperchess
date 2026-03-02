use std::time::Duration;

use crate::config::AppConfig;
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::domain::rules::Rules;

use super::discovery::WorkerDiscovery;
use super::proto::SearchRequest;
use super::proto::search_worker_client::SearchWorkerClient;

/// Coordinates distributed search across remote gRPC workers.
/// The gateway itself does no search work — all computation is delegated to workers.
pub struct DistributedSearch {
    config: AppConfig,
    discovery: WorkerDiscovery,
}

impl DistributedSearch {
    pub fn new(config: &AppConfig) -> Self {
        let discovery = WorkerDiscovery::new(
            config.distributed.worker_dns.clone(),
            config.distributed.grpc_port,
        );
        Self {
            config: config.clone(),
            discovery,
        }
    }

    /// Execute a distributed search across available workers.
    /// All moves are partitioned across remote workers; the gateway does no local search.
    pub async fn search(&self, board: &Board, player: Player) -> Option<Move> {
        let root_moves_sv = Rules::generate_legal_moves(&mut board.clone(), player);
        if root_moves_sv.is_empty() {
            return None;
        }
        let root_moves: Vec<Move> = root_moves_sv.to_vec();

        let workers = self.discovery.discover_workers().await;

        if workers.is_empty() {
            eprintln!("[coordinator] No workers available, cannot search");
            return None;
        }

        eprintln!(
            "[coordinator] Distributing {} root moves across {} workers",
            root_moves.len(),
            workers.len()
        );

        let chunks = partition_moves(root_moves, workers.len());

        // Serialize board once
        let board_data = bincode::serialize(board).expect("Failed to serialize board");
        let player_str = match player {
            Player::White => "white",
            Player::Black => "black",
        };
        let time_limit_ms = self.config.compute.minutes * 60 * 1000;

        // Spawn remote searches
        let mut remote_handles = Vec::new();
        for (i, worker_addr) in workers.iter().enumerate() {
            let chunk = chunks[i].clone();
            if chunk.is_empty() {
                continue;
            }

            let bd = board_data.clone();
            let addr = worker_addr.clone();
            let depth = self.config.minimax.depth as u32;
            let memory = self.config.compute.memory as u32;
            let threads = self.config.compute.concurrency as u32;
            let player_s = player_str.to_string();

            remote_handles.push(tokio::spawn(async move {
                remote_search(
                    addr,
                    bd,
                    player_s,
                    chunk,
                    depth,
                    time_limit_ms,
                    memory,
                    threads,
                )
                .await
            }));
        }

        // Collect all results with timeout
        let mut results: Vec<(Move, i32)> = Vec::new();
        let deadline = Duration::from_secs(self.config.compute.minutes * 60 + 10);
        for handle in remote_handles {
            match tokio::time::timeout(deadline, handle).await {
                Ok(Ok(Some(result))) => results.push(result),
                Ok(Ok(None)) => eprintln!("[coordinator] Remote worker returned no result"),
                Ok(Err(e)) => eprintln!("[coordinator] Remote task failed: {}", e),
                Err(_) => eprintln!("[coordinator] Remote task timed out"),
            }
        }

        // Pick best move across all results
        results.into_iter().max_by_key(|r| r.1).map(|(mv, score)| {
            eprintln!("[coordinator] Best move score: {}", score);
            mv
        })
    }
}

/// Execute a search on a remote worker via gRPC.
async fn remote_search(
    addr: String,
    board_data: Vec<u8>,
    player: String,
    moves: Vec<Move>,
    depth: u32,
    time_limit_ms: u64,
    memory_mb: u32,
    num_threads: u32,
) -> Option<(Move, i32)> {
    let moves_data = bincode::serialize(&moves).expect("Failed to serialize moves");

    eprintln!(
        "[coordinator] Sending {} moves to worker {}",
        moves.len(),
        addr
    );

    let mut client = match SearchWorkerClient::connect(addr.clone()).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[coordinator] Failed to connect to {}: {}", addr, e);
            return None;
        }
    };

    let request = tonic::Request::new(SearchRequest {
        board_data,
        player,
        moves_data,
        depth,
        time_limit_ms,
        memory_mb,
        num_threads,
    });

    match client.search_moves(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            let best_move: Move = bincode::deserialize(&resp.best_move).ok()?;
            eprintln!(
                "[coordinator] Worker {} returned: score={}, nodes={}, completed={}",
                addr, resp.score, resp.nodes_searched, resp.completed
            );
            Some((best_move, resp.score))
        }
        Err(e) => {
            eprintln!("[coordinator] gRPC call to {} failed: {}", addr, e);
            None
        }
    }
}

/// Partition moves into N roughly equal chunks.
fn partition_moves(moves: Vec<Move>, n: usize) -> Vec<Vec<Move>> {
    let mut chunks: Vec<Vec<Move>> = (0..n).map(|_| Vec::new()).collect();
    for (i, mv) in moves.into_iter().enumerate() {
        chunks[i % n].push(mv);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::coordinate::Coordinate;

    fn make_move(from: usize, to: usize) -> Move {
        Move {
            from: Coordinate::new(vec![from as u8]),
            to: Coordinate::new(vec![to as u8]),
            promotion: None,
        }
    }

    #[test]
    fn test_partition_moves_even_split() {
        let moves: Vec<Move> = (0..6).map(|i| make_move(i, i + 1)).collect();
        let chunks = partition_moves(moves, 3);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 2);
        assert_eq!(chunks[1].len(), 2);
        assert_eq!(chunks[2].len(), 2);
    }

    #[test]
    fn test_partition_moves_uneven_split() {
        let moves: Vec<Move> = (0..7).map(|i| make_move(i, i + 1)).collect();
        let chunks = partition_moves(moves, 3);

        assert_eq!(chunks.len(), 3);
        // 7 moves across 3 partitions: 3, 2, 2
        assert_eq!(chunks[0].len(), 3);
        assert_eq!(chunks[1].len(), 2);
        assert_eq!(chunks[2].len(), 2);
    }

    #[test]
    fn test_partition_moves_single_partition() {
        let moves: Vec<Move> = (0..5).map(|i| make_move(i, i + 1)).collect();
        let chunks = partition_moves(moves, 1);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 5);
    }

    #[test]
    fn test_partition_moves_more_partitions_than_moves() {
        let moves: Vec<Move> = (0..2).map(|i| make_move(i, i + 1)).collect();
        let chunks = partition_moves(moves, 5);

        assert_eq!(chunks.len(), 5);
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_partition_moves_empty() {
        let moves: Vec<Move> = Vec::new();
        let chunks = partition_moves(moves, 3);

        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|c| c.is_empty()));
    }

    #[test]
    fn test_partition_moves_preserves_all_moves() {
        let moves: Vec<Move> = (0..20).map(|i| make_move(i, i + 1)).collect();
        let original_len = moves.len();
        let chunks = partition_moves(moves, 4);

        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, original_len);
    }
}
