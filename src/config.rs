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
        if Path::new(config_path).exists() {
            let contents = fs::read_to_string(config_path).expect("Failed to read Config.toml");
            toml::from_str(&contents).expect("Failed to parse Config.toml")
        } else {
            eprintln!("Config.toml not found, using defaults");
            Self::default()
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
