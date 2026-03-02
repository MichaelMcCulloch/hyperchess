use deadpool_redis::{Config, Connection, Pool, Runtime};
use redis::AsyncCommands;

use crate::domain::game::Game;
use crate::domain::models::Player;

/// Serializable session data stored in Redis.
/// Bot instances are not stored — they're reconstructed from BotConfig.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RedisSession {
    pub game: Game,
    pub white_bot_config: Option<BotConfig>,
    pub black_bot_config: Option<BotConfig>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct BotConfig {
    pub dimension: usize,
    pub side: usize,
}

impl RedisSession {
    pub fn is_bot_turn(&self) -> bool {
        let player = self.game.current_turn();
        match player {
            Player::White => self.white_bot_config.is_some(),
            Player::Black => self.black_bot_config.is_some(),
        }
    }
}

pub struct RedisSessionStore {
    pool: Pool,
}

const SESSION_TTL_SECS: i64 = 3600; // 1 hour

impl RedisSessionStore {
    pub async fn new(redis_url: &str) -> Self {
        let cfg = Config::from_url(redis_url);
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1))
            .expect("Failed to create Redis pool");

        // Verify connection
        let mut conn = pool.get().await.expect("Failed to connect to Redis");
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .expect("Redis PING failed");

        eprintln!("[redis] Connected to {}", redis_url);

        Self { pool }
    }

    async fn conn(&self) -> Connection {
        self.pool
            .get()
            .await
            .expect("Failed to get Redis connection")
    }

    pub async fn save_session(&self, uuid: &str, session: &RedisSession) -> Result<(), String> {
        let data = bincode::serialize(session).map_err(|e| format!("Serialize failed: {}", e))?;
        let key = format!("session:{}", uuid);

        let mut conn = self.conn().await;
        conn.set_ex::<_, _, ()>(&key, data, SESSION_TTL_SECS as u64)
            .await
            .map_err(|e| format!("Redis SET failed: {}", e))?;

        Ok(())
    }

    pub async fn get_session(&self, uuid: &str) -> Result<Option<RedisSession>, String> {
        let key = format!("session:{}", uuid);
        let mut conn = self.conn().await;

        let data: Option<Vec<u8>> = conn
            .get(&key)
            .await
            .map_err(|e| format!("Redis GET failed: {}", e))?;

        match data {
            Some(bytes) => {
                // Refresh TTL on read
                let _: () = conn.expire(&key, SESSION_TTL_SECS).await.unwrap_or(());

                let session: RedisSession = bincode::deserialize(&bytes)
                    .map_err(|e| format!("Deserialize failed: {}", e))?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Acquire a distributed lock for a game session.
    /// Returns true if lock was acquired.
    pub async fn acquire_lock(&self, uuid: &str, holder: &str) -> Result<bool, String> {
        let lock_key = format!("session:{}:lock", uuid);
        let mut conn = self.conn().await;

        // SET NX EX 30 — acquire only if not held, with 30s TTL
        let result: bool = redis::cmd("SET")
            .arg(&lock_key)
            .arg(holder)
            .arg("NX")
            .arg("EX")
            .arg(30)
            .query_async(&mut conn)
            .await
            .unwrap_or(false);

        Ok(result)
    }

    /// Release a distributed lock for a game session.
    pub async fn release_lock(&self, uuid: &str, holder: &str) -> Result<(), String> {
        let lock_key = format!("session:{}:lock", uuid);
        let mut conn = self.conn().await;

        // Only release if we hold it
        let current: Option<String> = conn.get(&lock_key).await.unwrap_or(None);

        if current.as_deref() == Some(holder) {
            let _: () = conn.del(&lock_key).await.unwrap_or(());
        }

        Ok(())
    }
}
