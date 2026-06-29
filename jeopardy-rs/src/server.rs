use std::{env, sync::Arc, time::Duration};

use anyhow::Context;
use serde::Deserialize;
use stagecrew::manager::{Manager, MapManager, PasswordProtectedLobby};
use tokio::{fs, sync::RwLock};

use crate::{
    game::Jeopardy,
    web::handlers::validators::{CredsValidator, nonzero_ascii::NonZeroAsciiValidator},
};

/// helper trait to allow for creating a default variant
/// of a struct to be used for testing
/// - keeps tests clean and idiomatic
#[cfg(test)]
pub trait TestDefault {
    fn test_default() -> Self;
}

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
}

#[cfg(test)]
impl TestDefault for ServerConfig {
    // absurdly small, unreasonble values, created for testing
    fn test_default() -> Self {
        Self {
            max_timeout: Duration::from_secs(1),
            max_retry: 3,
            max_username_length: 32,
            player_channel_buffer_size: 1,
            lobby_cleanup_grace_period: Duration::from_secs(1),
        }
    }
}

/// Top level struct to encapsulate server state
/// which includes the `ServerConfig`, `CredsValidator`
/// and the `Manager` for all the lobbies
pub struct JeopardyServer<M: Manager, C: CredsValidator> {
    // i have to have the RwLock here in the signature
    // bc i don't want access contention over manager vs config when unrelated
    manager: RwLock<M>,

    // the following are read-only
    config: ServerConfig,
    validator: C,
}

impl<M: Manager, C: CredsValidator> JeopardyServer<M, C> {
    pub fn new(manager: M, validator: C, config: ServerConfig) -> Self {
        Self {
            manager: RwLock::new(manager),
            validator,
            config,
        }
    }

    pub fn manager(&self) -> &RwLock<M> {
        &self.manager
    }

    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    pub fn validator(&self) -> &C {
        &self.validator
    }
}

/// top level alias for our chosen implementation of `JeopardyServer`
pub type JeopardyServerState =
    Arc<JeopardyServer<MapManager<PasswordProtectedLobby<Jeopardy>>, NonZeroAsciiValidator>>;

impl JeopardyServer<MapManager<PasswordProtectedLobby<Jeopardy>>, NonZeroAsciiValidator> {
    pub fn from_config(config: ServerConfig) -> Self {
        JeopardyServer::new(
            MapManager::new(),
            NonZeroAsciiValidator::new(config.max_username_length),
            config,
        )
    }
}

// generic aliases - really terrible but is the only way i don't have to write this everywhere
pub trait ManagerGeneric:
    Manager<Entry = PasswordProtectedLobby<Jeopardy>> + Send + Sync + 'static
{
}
impl<T> ManagerGeneric for T where
    T: Manager<Entry = PasswordProtectedLobby<Jeopardy>> + Send + Sync + 'static
{
}

pub trait CredsValidatorGeneric: CredsValidator + Send + Sync + 'static {}
impl<T> CredsValidatorGeneric for T where T: CredsValidator + Send + Sync + 'static {}
pub type GenericJeopardyServerState<ManagerGeneric, CredsValidatorGeneric> =
    Arc<JeopardyServer<ManagerGeneric, CredsValidatorGeneric>>;
