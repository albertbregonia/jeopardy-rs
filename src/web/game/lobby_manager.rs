use super::lobby::Lobby;
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LobbyManagerError {
    #[error("{0}")]
    User(#[from] UserError),
    #[error("{0}")]
    Internal(#[from] InternalError),
}

#[derive(Debug, Error)]
pub enum UserError {
    #[error("Lobby by the name '{0}' was not found in the database")]
    LobbyNotFound(String),
    #[error("Lobby by the name '{0}' already exists in the database")]
    LobbyAlreadyExists(String),
}

#[derive(Debug, Error)]
pub enum InternalError {}

/// LobbyManager is the high level trait defining the collection of lobbies for the server
/// For now, it is mostly for decoupling because we are only going to implement this with a `LobbyMap`
/// but this is a nice abstraction for a real database with real API calls later on if we so choose.
pub trait LobbyManager<T: Serialize> {
    /// returns a mutable reference to a lobby in the collection given the name
    fn get_mut(&mut self, name: &str) -> Result<&mut Lobby<T>, LobbyManagerError>;

    /// returns a reference to a lobby in the collection given the name
    fn get(&self, name: &str) -> Result<&Lobby<T>, LobbyManagerError>;

    /// adds a new lobby to the collection returning the reference
    fn add(&mut self, lobby: Lobby<T>) -> Result<&Lobby<T>, LobbyManagerError>;

    /// removes lobby from the collection and returns the owned instance
    fn remove(&mut self, name: &str) -> Result<Lobby<T>, LobbyManagerError>;
}

impl<T> LobbyManager<T> for LobbyMap<T>
where
    T: Serialize,
{
    fn get_mut(&mut self, name: &str) -> Result<&mut Lobby<T>, LobbyManagerError> {
        self.lobbies
            .get_mut(name)
            .ok_or(LobbyManagerError::User(UserError::LobbyNotFound(
                name.to_string(),
            )))
    }

    fn get(&self, name: &str) -> Result<&Lobby<T>, LobbyManagerError> {
        self.lobbies
            .get(name)
            .ok_or(LobbyManagerError::User(UserError::LobbyNotFound(
                name.to_string(),
            )))
    }

    fn add(&mut self, lobby: Lobby<T>) -> Result<&Lobby<T>, LobbyManagerError> {
        let name = lobby.get_name().to_string();
        if self.lobbies.contains_key(&name) {
            return Err(LobbyManagerError::User(UserError::LobbyAlreadyExists(name)));
        }
        self.lobbies.insert(name.clone(), lobby);
        self.get(&name)
    }

    fn remove(&mut self, name: &str) -> Result<Lobby<T>, LobbyManagerError> {
        let lobby =
            self.lobbies
                .remove(name)
                .ok_or(LobbyManagerError::User(UserError::LobbyNotFound(
                    name.to_string(),
                )))?;
        Ok(lobby)
    }
}

pub struct LobbyMap<T: Serialize> {
    lobbies: HashMap<String, Lobby<T>>, // keys are lobby name, values are `Lobby` instances
}

impl<T> LobbyMap<T>
where
    T: Serialize,
{
    pub fn new() -> Self {
        Self {
            lobbies: HashMap::new(),
        }
    }
}
