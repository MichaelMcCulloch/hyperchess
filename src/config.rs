use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct AppConfig {
    pub minimax: MinimaxConfig,
    pub compute: ComputeConfig,
    pub api: ApiConfig,
    #[serde(default)]
    pub distributed: DistributedConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DistributedConfig {
    pub enabled: bool,
    /// "gateway" or "worker"
    pub mode: String,
    pub redis_url: String,
    pub worker_dns: String,
    pub grpc_port: u16,
}

impl Default for DistributedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "standalone".to_string(),
            redis_url: "redis://127.0.0.1:6379".to_string(),
            worker_dns: String::new(),
            grpc_port: 50051,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MinimaxConfig {
    pub depth: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ComputeConfig {
    pub minutes: u64,
    pub concurrency: usize,
    pub memory: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiConfig {
    pub port: u16,
}

impl AppConfig {
    pub fn load() -> Self {
        let config_path = "Config.toml";
        let mut config = if Path::new(config_path).exists() {
            let contents = fs::read_to_string(config_path).expect("Failed to read Config.toml");
            toml::from_str(&contents).expect("Failed to parse Config.toml")
        } else {
            eprintln!("Config.toml not found, using defaults");
            Self::default()
        };

        config.merge_env();

        eprintln!("----------------------------------------");
        eprintln!("HyperChess Configuration:");
        eprintln!("  Minimax Depth: {}", config.minimax.depth);
        eprintln!(
            "  Compute: {:.1} min, {} threads, {} MB memory",
            config.compute.minutes, config.compute.concurrency, config.compute.memory
        );
        eprintln!("  API Port: {}", config.api.port);
        if config.distributed.enabled {
            eprintln!("  Distributed: {} mode", config.distributed.mode);
            eprintln!("  Redis: {}", config.distributed.redis_url);
            eprintln!("  Worker DNS: {}", config.distributed.worker_dns);
            eprintln!("  gRPC Port: {}", config.distributed.grpc_port);
        }
        eprintln!("----------------------------------------");

        config
    }

    fn merge_env(&mut self) {
        if let Ok(val) = std::env::var("HYPERCHESS_MINIMAX_DEPTH")
            && let Ok(parsed) = val.parse()
        {
            self.minimax.depth = parsed;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_COMPUTE_MEMORY")
            && let Ok(parsed) = val.parse()
        {
            self.compute.memory = parsed;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_COMPUTE_MINUTES")
            && let Ok(parsed) = val.parse()
        {
            self.compute.minutes = parsed;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_COMPUTE_CONCURRENCY")
            && let Ok(parsed) = val.parse()
        {
            self.compute.concurrency = parsed;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_API_PORT")
            && let Ok(parsed) = val.parse()
        {
            self.api.port = parsed;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_DISTRIBUTED_ENABLED") {
            self.distributed.enabled = val == "true" || val == "1";
        }
        if let Ok(val) = std::env::var("HYPERCHESS_MODE") {
            self.distributed.mode = val;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_REDIS_URL") {
            self.distributed.redis_url = val;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_WORKER_DNS") {
            self.distributed.worker_dns = val;
        }
        if let Ok(val) = std::env::var("HYPERCHESS_GRPC_PORT")
            && let Ok(parsed) = val.parse()
        {
            self.distributed.grpc_port = parsed;
        }
    }
}

impl Default for ComputeConfig {
    fn default() -> Self {
        Self {
            minutes: 2,
            concurrency: 2,
            memory: 1024,
        }
    }
}
impl Default for ApiConfig {
    fn default() -> Self {
        Self { port: 3123 }
    }
}
impl Default for MinimaxConfig {
    fn default() -> Self {
        Self { depth: 4 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    struct EnvVarGuard {
        key: String,
        original: Option<String>,
    }

    impl EnvVarGuard {
        fn new(key: &str, value: &str) -> Self {
            let original = env::var(key).ok();
            unsafe {
                env::set_var(key, value);
            }
            Self {
                key: key.to_string(),
                original,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.original {
                    Some(val) => env::set_var(&self.key, val),
                    None => env::remove_var(&self.key),
                }
            }
        }
    }

    #[test]
    fn test_default_config_loading() {
        let _guard = EnvVarGuard::new(
            "HYPERCHESS_MINIMAX_DEPTH",
            "invalid_to_ensure_clean_slate_or_removal",
        );
        unsafe {
            env::remove_var("HYPERCHESS_MINIMAX_DEPTH");
        }

        let config = AppConfig::default();
        assert_eq!(config.minimax.depth, 4);
    }

    #[test]
    fn test_merge_env_overrides() {
        let mut config = AppConfig::default();

        let _g1 = EnvVarGuard::new("HYPERCHESS_MINIMAX_DEPTH", "99");
        let _g3 = EnvVarGuard::new("HYPERCHESS_COMPUTE_CONCURRENCY", "42");
        let _g4 = EnvVarGuard::new("HYPERCHESS_API_PORT", "8888");

        config.merge_env();

        assert_eq!(config.minimax.depth, 99);
        assert_eq!(config.compute.concurrency, 42);
        assert_eq!(config.api.port, 8888);
    }

    #[test]
    fn test_invalid_env_vars_ignored() {
        let mut config = AppConfig::default();
        let _g1 = EnvVarGuard::new("HYPERCHESS_MINIMAX_DEPTH", "not_a_number");

        config.merge_env();

        assert_eq!(config.minimax.depth, 4);
    }

    #[test]
    fn test_load_prints_config() {
        let _config = AppConfig::load();
    }
}
