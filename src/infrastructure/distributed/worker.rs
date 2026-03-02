use std::time::Duration;

use tonic::{Request, Response, Status};

use super::proto::search_worker_server::SearchWorker;
use super::proto::{PingRequest, PingResponse, SearchRequest, SearchResponse};
use crate::config::AppConfig;
use crate::domain::board::Board;
use crate::domain::models::{Move, Player};
use crate::infrastructure::ai::MinimaxBot;

pub struct SearchWorkerService {
    pub config: AppConfig,
}

#[tonic::async_trait]
impl SearchWorker for SearchWorkerService {
    async fn search_moves(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let req = request.into_inner();

        let board: Board = bincode::deserialize(&req.board_data)
            .map_err(|e| Status::invalid_argument(format!("Invalid board data: {}", e)))?;

        let moves: Vec<Move> = bincode::deserialize(&req.moves_data)
            .map_err(|e| Status::invalid_argument(format!("Invalid moves data: {}", e)))?;

        let player = match req.player.as_str() {
            "white" => Player::White,
            "black" => Player::Black,
            _ => {
                return Err(Status::invalid_argument(
                    "Player must be 'white' or 'black'",
                ));
            }
        };

        let depth = req.depth as usize;
        let time_limit = Duration::from_millis(req.time_limit_ms);
        let memory_mb = req.memory_mb as usize;
        let num_threads = req.num_threads as usize;

        eprintln!(
            "[worker] Received search request: {} moves, depth={}, threads={}, time={}ms",
            moves.len(),
            depth,
            num_threads,
            req.time_limit_ms
        );

        let result = tokio::task::spawn_blocking(move || {
            let mut bot = MinimaxBot::new_from_params(depth, time_limit, memory_mb, num_threads);
            bot.search_subset(&board, player, moves)
        })
        .await
        .map_err(|e| Status::internal(format!("Search task failed: {}", e)))?;

        let (best_move, score, nodes, completed) = result;

        eprintln!(
            "[worker] Search complete: score={}, nodes={}, completed={}",
            score, nodes, completed
        );

        Ok(Response::new(SearchResponse {
            best_move: bincode::serialize(&best_move)
                .map_err(|e| Status::internal(format!("Failed to serialize move: {}", e)))?,
            score,
            nodes_searched: nodes,
            completed,
        }))
    }

    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        Ok(Response::new(PingResponse {
            pod_name: std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string()),
            available_threads: self.config.compute.concurrency as u32,
        }))
    }
}

/// Start the gRPC worker server.
pub async fn start_grpc_worker(config: AppConfig) {
    let addr = format!("0.0.0.0:{}", config.distributed.grpc_port)
        .parse()
        .expect("Invalid gRPC address");

    let service = SearchWorkerService {
        config: config.clone(),
    };

    eprintln!("[worker] Starting gRPC worker on {}", addr);

    tonic::transport::Server::builder()
        .add_service(super::proto::search_worker_server::SearchWorkerServer::new(
            service,
        ))
        .serve(addr)
        .await
        .expect("gRPC worker server failed");
}
