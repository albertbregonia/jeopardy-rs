use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;

use super::player::Player;

#[derive(Debug, Error)]
pub enum LobbyError {
    #[error("{0}")]
    User(#[from] UserError),
    #[error("{0}")]
    Internal(#[from] InternalError),
}

#[derive(Debug, Error)]
pub enum UserError {
    #[error("A user by the name '{0}' already exists in this lobby.")]
    UsernameTaken(String),
    #[error("Username '{0}' not found in the lobby.")]
    UserNotFound(String),
}

#[derive(Debug, Error)]
pub enum InternalError {}

pub struct Lobby<T: Serialize> {
    name: String,
    password: String,
    players: HashMap<String, Player<T>>,
}

impl<T> Lobby<T>
where
    T: Serialize,
{
    pub fn new(name: String, password: String) -> Self {
        Self {
            name: sanitize_name(&name),
            password,
            players: HashMap::new(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn is_correct_password(&self, password: &str) -> bool {
        // NOTE: this should normally be a hash for security,
        // but since this is NOT prod, plaintext is fine.
        self.password == password
    }

    pub fn get_player(&self, name: &str) -> Result<&Player<T>, LobbyError> {
        self.players
            .get(name)
            .ok_or(LobbyError::User(UserError::UserNotFound(name.to_string())))
    }

    pub fn add_player(&mut self, p: Player<T>) -> Result<(), LobbyError> {
        let name = p.get_name();
        if self.players.contains_key(name) {
            return Err(LobbyError::User(UserError::UsernameTaken(name.to_string())));
        }
        self.players.insert(name.to_string(), p);
        Ok(())
    }

    pub fn remove_player(&mut self, name: &str) -> Result<Player<T>, LobbyError> {
        self.players
            .remove(name)
            .ok_or(LobbyError::User(UserError::UserNotFound(name.to_string())))
    }
}

// lobby name must be all lowercase alphanumeric (including underscore)
// remove all other characters otherwise.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn is_valid_lobby_name(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() && c.is_ascii_lowercase() || c == '_')
}
