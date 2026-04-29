use super::lobby::Lobby;
use serde::Serialize;
use std::{collections::HashMap, fmt::Debug};
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
pub trait LobbyManager<T: Serialize + Debug> {
    /// returns a mutable reference to a lobby in the collection given the name
    fn get_mut(&mut self, name: &str) -> Result<&mut Lobby<T>, LobbyManagerError>;

    /// returns a reference to a lobby in the collection given the name
    fn get(&self, name: &str) -> Result<&Lobby<T>, LobbyManagerError>;

    /// adds a new lobby to the collection
    fn add(&mut self, lobby: Lobby<T>) -> Result<(), LobbyManagerError>;

    /// removes lobby from the collection and returns the owned instance
    fn remove(&mut self, name: &str) -> Result<Lobby<T>, LobbyManagerError>;
}

impl<T> LobbyManager<T> for LobbyMap<T>
where
    T: Serialize + Debug,
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

    fn add(&mut self, lobby: Lobby<T>) -> Result<(), LobbyManagerError> {
        let name = lobby.get_name().to_string();
        if self.lobbies.contains_key(&name) {
            return Err(LobbyManagerError::User(UserError::LobbyAlreadyExists(name)));
        }
        self.lobbies.insert(name.clone(), lobby);
        Ok(())
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

// lobby name must be all lowercase alphanumeric (including special chars)
// remove all other characters otherwise.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_graphic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn is_valid_lobby_name(name: &str, max_length: usize) -> bool {
    let n = name.len();
    let non_zero_length = n > 0;
    let under_length_limit = n <= max_length;
    let lowercase_visible_ascii = name
        .chars()
        .all(|c| (c.is_ascii_graphic() && c.is_ascii_lowercase()) || c.is_ascii_punctuation());
    non_zero_length && under_length_limit && lowercase_visible_ascii
}

pub struct LobbyMap<T: Serialize + Debug> {
    lobbies: HashMap<String, Lobby<T>>, // keys are lobby name, values are `Lobby` instances
}

impl<T> LobbyMap<T>
where
    T: Serialize + Debug,
{
    pub fn new() -> Self {
        Self {
            lobbies: HashMap::new(),
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod lobby_map_tests {
    use tokio::sync::mpsc;

    use crate::web::game::player::Player;

    use super::*;

    const TEST_LOBBY_NAME: &str = "test_lobby";
    const TEST_INVALID_LOBBY_NAME: &str = "TEST_LOBBY";
    const TEST_LOBBY_PASSWORD: &str = "test_password";
    const TEST_MAX_NAME_LENGTH: usize = 32;

    #[test]
    fn GIVEN_invalid_lobby_name_WHEN_sanitize_THEN_ok() {
        // GIVEN
        let name = TEST_INVALID_LOBBY_NAME;
        // WHEN/THEN
        assert_eq!(is_valid_lobby_name(name, TEST_MAX_NAME_LENGTH), false);
        // sanitize removes invalid chars and makes lowercase
        let sanitized = sanitize_name(name);
        assert_eq!(sanitized, TEST_LOBBY_NAME);
        assert!(is_valid_lobby_name(&sanitized, TEST_MAX_NAME_LENGTH));
    }

    #[test]
    fn GIVEN_valid_lobby_name_WHEN_sanitize_THEN_ok() {
        let name = TEST_LOBBY_NAME;
        assert!(is_valid_lobby_name(name, TEST_MAX_NAME_LENGTH));
        assert_eq!(sanitize_name(name), name); // sanitize does nothing here
    }

    #[test]
    fn GIVEN_empty_lobby_map_WHEN_add_THEN_ok() {
        // GIVEN
        let mut lobby_map = LobbyMap::<u8>::new();
        let new_lobby = Lobby::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        // WHEN
        lobby_map.add(new_lobby).unwrap();
        // THEN
        let lobby = lobby_map.get(TEST_LOBBY_NAME).unwrap();
        assert_eq!(lobby.get_name(), TEST_LOBBY_NAME);
        assert!(lobby.is_correct_password(TEST_LOBBY_PASSWORD));
    }

    #[test]
    fn GIVEN_lobby_map_with_conflicting_lobby_name_WHEN_add_THEN_err() {
        // GIVEN
        let mut lobby_map = LobbyMap::<u8>::new();
        let new_lobby = Lobby::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_NAME.to_string());
        // WHEN
        lobby_map.add(new_lobby.clone()).unwrap();
        let result = lobby_map.add(new_lobby);
        // THEN
        assert!(matches!(
            result,
            Err(LobbyManagerError::User(UserError::LobbyAlreadyExists(name))) if name == TEST_LOBBY_NAME
        ));
    }

    #[test]
    fn GIVEN_empty_lobby_map_WHEN_get_THEN_err() {
        // GIVEN
        let lobby_map = LobbyMap::<u8>::new();
        // WHEN
        let result = lobby_map.get(TEST_LOBBY_NAME);
        // THEN
        assert!(matches!(
            result,
            Err(LobbyManagerError::User(UserError::LobbyNotFound(name))) if name == TEST_LOBBY_NAME
        ));
    }

    #[test]
    fn GIVEN_lobby_map_WHEN_get_THEN_ok() {
        // GIVEN
        let mut lobby_map = LobbyMap::<u8>::new();
        let new_lobby = Lobby::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        lobby_map.add(new_lobby).unwrap();
        // WHEN
        let lobby = lobby_map.get(TEST_LOBBY_NAME).unwrap();
        // THEN
        assert_eq!(lobby.get_name(), TEST_LOBBY_NAME);
        assert!(lobby.is_correct_password(TEST_LOBBY_PASSWORD));
    }

    #[test]
    fn GIVEN_empty_lobby_map_WHEN_get_mut_THEN_err() {
        // GIVEN
        let mut lobby_map = LobbyMap::<u8>::new();
        // WHEN
        let result = lobby_map.get_mut(TEST_LOBBY_NAME);
        // THEN
        assert!(matches!(
            result,
            Err(LobbyManagerError::User(UserError::LobbyNotFound(name))) if name == TEST_LOBBY_NAME
        ));
    }

    #[test]
    fn GIVEN_lobby_map_WHEN_get_mut_THEN_ok() {
        // GIVEN
        let mut lobby_map = LobbyMap::<u8>::new();
        let new_lobby = Lobby::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        lobby_map.add(new_lobby).unwrap();
        // WHEN
        let lobby = lobby_map.get_mut(TEST_LOBBY_NAME).unwrap();
        // THEN
        assert_eq!(lobby.get_name(), TEST_LOBBY_NAME);
        assert!(lobby.is_correct_password(TEST_LOBBY_PASSWORD));

        // if this wasn't mutable the following wouldn't compile
        let (sender, _receiver) = mpsc::channel(1);
        let _ = lobby.add_player(Player::new("test".to_string(), sender));
    }

    #[test]
    fn GIVEN_empty_lobby_map_WHEN_remove_THEN_err() {
        // GIVEN
        let mut lobby_map = LobbyMap::<u8>::new();
        // WHEN
        let result = lobby_map.remove(TEST_LOBBY_NAME);
        // THEN
        assert!(matches!(
            result,
            Err(LobbyManagerError::User(UserError::LobbyNotFound(name))) if name == TEST_LOBBY_NAME
        ));
    }

    #[test]
    fn GIVEN_lobby_map_WHEN_remove_THEN_ok() {
        // GIVEN
        let mut lobby_map = LobbyMap::<u8>::new();
        let new_lobby = Lobby::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        lobby_map.add(new_lobby).unwrap();
        // WHEN
        let lobby = lobby_map.remove(TEST_LOBBY_NAME).unwrap();
        // THEN
        assert_eq!(lobby.get_name(), TEST_LOBBY_NAME);
        assert!(lobby.is_correct_password(TEST_LOBBY_PASSWORD));
    }
}
