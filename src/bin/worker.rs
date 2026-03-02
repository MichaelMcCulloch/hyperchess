use hyperchess::config::AppConfig;

#[tokio::main]
async fn main() {
    let config = AppConfig::load();

    println!("Starting HyperChess Worker...");
    hyperchess::infrastructure::distributed::worker::start_grpc_worker(config).await;
}
