use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub minimax: MinimaxConfig,
    pub mcts: Option<MctsConfig>,
    pub compute: ComputeConfig,
    pub api: ApiConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MinimaxConfig {
    pub depth: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MctsConfig {
    pub depth: usize,
    pub iterations: usize,
    pub iter_per_thread: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ComputeConfig {
    pub minutes: f64,
    pub concurrency: usize,
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
        config
    }

    fn merge_env(&mut self) {
        if let Ok(val) = std::env::var("HYPERCHESS_MINIMAX_DEPTH") {
            if let Ok(parsed) = val.parse() {
                self.minimax.depth = parsed;
            }
        }
        if let Ok(val) = std::env::var("HYPERCHESS_MCTS_DEPTH") {
            if let Ok(parsed) = val.parse() {
                if let Some(mcts) = &mut self.mcts {
                    mcts.depth = parsed;
                }
            }
        }
        if let Ok(val) = std::env::var("HYPERCHESS_MCTS_ITERATIONS") {
            if let Ok(parsed) = val.parse() {
                if let Some(mcts) = &mut self.mcts {
                    mcts.iterations = parsed;
                }
            }
        }
        if let Ok(val) = std::env::var("HYPERCHESS_MCTS_ITER_PER_THREAD") {
            if let Ok(parsed) = val.parse() {
                if let Some(mcts) = &mut self.mcts {
                    mcts.iter_per_thread = parsed;
                }
            }
        }
        if let Ok(val) = std::env::var("HYPERCHESS_COMPUTE_MINUTES") {
            if let Ok(parsed) = val.parse() {
                self.compute.minutes = parsed;
            }
        }
        if let Ok(val) = std::env::var("HYPERCHESS_COMPUTE_CONCURRENCY") {
            if let Ok(parsed) = val.parse() {
                self.compute.concurrency = parsed;
            }
        }
        if let Ok(val) = std::env::var("HYPERCHESS_API_PORT") {
            if let Ok(parsed) = val.parse() {
                self.api.port = parsed;
            }
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            minimax: MinimaxConfig { depth: 4 },
            mcts: Some(MctsConfig {
                depth: 50,
                iterations: 50,
                iter_per_thread: 5.0,
            }),
            compute: ComputeConfig {
                minutes: 2.0,
                concurrency: 2,
            },
            api: ApiConfig { port: 3123 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // Helper to safely set and unset env vars for tests
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
        // Ensure no env vars interfere
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

        // Set env vars
        let _g1 = EnvVarGuard::new("HYPERCHESS_MINIMAX_DEPTH", "99");
        let _g2 = EnvVarGuard::new("HYPERCHESS_MCTS_DEPTH", "101");
        let _g3 = EnvVarGuard::new("HYPERCHESS_COMPUTE_CONCURRENCY", "42");
        let _g4 = EnvVarGuard::new("HYPERCHESS_API_PORT", "8888");

        config.merge_env();

        assert_eq!(config.minimax.depth, 99);
        assert_eq!(config.mcts.unwrap().depth, 101);
        assert_eq!(config.compute.concurrency, 42);
        assert_eq!(config.api.port, 8888);
    }

    #[test]
    fn test_invalid_env_vars_ignored() {
        let mut config = AppConfig::default();
        let _g1 = EnvVarGuard::new("HYPERCHESS_MINIMAX_DEPTH", "not_a_number");

        config.merge_env();

        assert_eq!(config.minimax.depth, 4); // Should remain default
    }
}
