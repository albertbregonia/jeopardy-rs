use std::{env, time::Duration};

use anyhow::Context;
use serde::Deserialize;
use tokio::fs;

pub const JSON_CONFIG_PATH_KEY: &str = "CONFIG_PATH";
pub const JSON_CONFIG_DEFAULT_PATH: &str = "./config.json";

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub max_timeout: Duration,
    pub max_retry: usize,
    pub max_username_length: usize,
    pub player_channel_buffer_size: usize,
    pub lobby_cleanup_grace_period: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_timeout: Duration::from_mins(5),
            max_retry: 3,
            max_username_length: 32,
            player_channel_buffer_size: 128,
            lobby_cleanup_grace_period: Duration::from_mins(5),
        }
    }
}

impl ServerConfig {
    pub async fn from_json_path(path: &str) -> anyhow::Result<Self> {
        let config = fs::read(path)
            .await
            .context("Failed to read JSON config file at given path")?;
        serde_json::from_slice::<ServerConfig>(&config)
            .context("Failed to deserialize file as JSON config")
    }

    pub async fn from_env() -> anyhow::Result<Self> {
        let path = env::var(JSON_CONFIG_PATH_KEY).unwrap_or(JSON_CONFIG_DEFAULT_PATH.to_string());
        Self::from_json_path(&path).await
    }

    #[cfg(test)]
    // absurdly small, unreasonble values, created for testing
    fn test_config() -> Self {
        Self {
            max_timeout: Duration::from_secs(1),
            max_retry: 3,
            max_username_length: 32,
            player_channel_buffer_size: 1,
            lobby_cleanup_grace_period: Duration::from_secs(1),
        }
    }
}
