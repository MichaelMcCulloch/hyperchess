use hyperchess::config::AppConfig;

#[tokio::main]
async fn main() {
    let mut config = AppConfig::load();

    // Force gateway mode
    config.distributed.enabled = true;
    config.distributed.mode = "gateway".to_string();

    // Apply env overrides that may have been set after initial load
    if let Ok(val) = std::env::var("HYPERCHESS_REDIS_URL") {
        config.distributed.redis_url = val;
    }
    if let Ok(val) = std::env::var("HYPERCHESS_WORKER_DNS") {
        config.distributed.worker_dns = val;
    }
    if let Ok(val) = std::env::var("HYPERCHESS_GRPC_PORT")
        && let Ok(parsed) = val.parse()
    {
        config.distributed.grpc_port = parsed;
    }

    println!("Starting HyperChess Gateway...");
    hyperchess::api::start_server_with_config(config).await;
}
