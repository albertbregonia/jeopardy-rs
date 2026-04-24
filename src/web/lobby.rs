use std::collections::HashMap;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LobbyManagerError {
    #[error("Lobby by the name '{0}' was not found in the database")]
    LobbyNotFound(String),
    #[error("Lobby by the name '{0}' already exists in the database")]
    LobbyAlreadyExists(String)
}

/// LobbyManager is the high level trait defining the collection of lobbies for the server
/// For now, it is mostly for decoupling because we are only going to implement this with a `LobbyMap`
/// but this is a nice abstraction for a real database with real API calls later on if we so choose.
pub trait LobbyManager {
    /// returns a reference to a lobby in the collection given the name
    fn get(&self, name: &str) -> Result<&Lobby, LobbyManagerError>;

    /// adds a new lobby to the collection returning the reference
    fn add(&mut self, lobby: Lobby) -> Result<&Lobby, LobbyManagerError>;

    /// removes lobby from the collection and returns the owned instance
    fn remove(&mut self, name: &str) -> Result<Lobby, LobbyManagerError>;
}

pub struct Lobby {
    name: String
}

pub struct LobbyMap {
    lobbies: HashMap<String, Lobby> // keys are lobby name, values are `Lobby` instances
}

impl LobbyManager for LobbyMap {
    fn get(&self, name: &str) -> Result<&Lobby, LobbyManagerError> {
        self.lobbies
            .get(name)
            .ok_or(LobbyManagerError::LobbyNotFound(name.to_string()))
    }

    fn add(&mut self, mut lobby: Lobby) -> Result<&Lobby, LobbyManagerError> {
        let name = LobbyMap::sanitize_lobby_name(lobby.name);
        if self.lobbies.contains_key(&name) {
            return Err(LobbyManagerError::LobbyAlreadyExists(name));
        }
        lobby.name = name.clone();
        self.lobbies
            .insert(name.clone(), lobby);
        self.get(&name)
    }

    fn remove(&mut self, name: &str) -> Result<Lobby, LobbyManagerError> {
        let lobby = self.lobbies
            .remove(name)
            .ok_or(LobbyManagerError::LobbyNotFound(name.to_string()))?;
        Ok(lobby)
    }
}

impl LobbyMap {
    pub fn new() -> Self {
        Self {
            lobbies: HashMap::new()
        }
    }

    pub fn sanitize_lobby_name(name: String) -> String {
        name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }
}