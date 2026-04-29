use serde::Serialize;
use std::{collections::HashMap, fmt::Debug};
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
    #[error("Invalid username: {0}. Must be valid ASCII and of length 0-{1}")]
    InvalidUsername(String, usize),
}

#[derive(Debug, Error)]
pub enum InternalError {}

#[derive(Clone)]
pub struct Lobby<T: Serialize + Debug> {
    name: String,
    password: String,
    players: HashMap<String, Player<T>>,
}

impl<T> Lobby<T>
where
    T: Serialize + Debug,
{
    pub fn new(name: String, password: String) -> Self {
        Self {
            name,
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

    pub fn get_mut_player(&mut self, name: &str) -> Result<&mut Player<T>, LobbyError> {
        self.players
            .get_mut(name)
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

#[cfg(test)]
#[allow(non_snake_case)]
mod lobby_tests {
    use std::fmt::Debug;

    use tokio::sync::mpsc;

    use crate::web::game::player::Player;

    use super::*;

    const TEST_LOBBY_NAME: &str = "test_lobby";
    const TEST_LOBBY_PASSWORD: &str = "test_password";
    const TEST_PLAYER_NAME: &str = "test_player";
    const TEST_PLAYER_INPUT: &str = "test_input";
    const TEST_PLAYER_POINTS: i32 = 100;
    const TEST_PLAYER_WAGER: i32 = 100;

    fn create_test_player<T: Serialize + Debug>(name: &str) -> Player<T> {
        let (sender, _receiver) = mpsc::channel(1);
        let mut player = Player::new(name.to_string(), sender);
        player.set_input(TEST_PLAYER_INPUT.to_string()).unwrap();
        player.set_points(TEST_PLAYER_POINTS).unwrap();
        player.set_wager(TEST_PLAYER_WAGER).unwrap();
        player
    }

    #[test]
    fn GIVEN_empty_lobby_WHEN_add_player_THEN_ok() {
        // GIVEN
        let mut lobby =
            Lobby::<u8>::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        // WHEN
        lobby
            .add_player(create_test_player(TEST_PLAYER_NAME))
            .unwrap();
        // THEN
        // NOTE: there is no get_player positive test bc it would be identical to this test.
        let player = lobby.get_player(TEST_PLAYER_NAME).unwrap();
        assert_eq!(player.get_name(), TEST_PLAYER_NAME);
        assert_eq!(player.get_input(), TEST_PLAYER_INPUT);
        assert_eq!(player.get_points(), TEST_PLAYER_POINTS);
        assert_eq!(player.get_wager(), TEST_PLAYER_WAGER);
    }

    #[test]
    fn GIVEN_lobby_with_conflicting_player_name_WHEN_add_player_THEN_err() {
        // GIVEN
        let mut lobby =
            Lobby::<u8>::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        lobby
            .add_player(create_test_player(TEST_PLAYER_NAME))
            .unwrap();
        // WHEN
        let result = lobby.add_player(create_test_player(TEST_PLAYER_NAME));
        // THEN
        assert!(matches!(
            result,
            Err(LobbyError::User(UserError::UsernameTaken(name))) if name == TEST_PLAYER_NAME
        ));
    }

    #[test]
    fn GIVEN_empty_lobby_WHEN_get_player_THEN_err() {
        // GIVEN
        let lobby = Lobby::<u8>::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        // WHEN
        let result = lobby.get_player(TEST_PLAYER_NAME);
        // THEN
        assert!(matches!(
            result,
            Err(LobbyError::User(UserError::UserNotFound(name))) if name == TEST_PLAYER_NAME
        ));
    }

    #[test]
    fn GIVEN_empty_lobby_WHEN_get_mut_player_THEN_err() {
        // GIVEN
        let mut lobby =
            Lobby::<u8>::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        // WHEN
        let result = lobby.get_mut_player(TEST_PLAYER_NAME);
        // THEN
        assert!(matches!(
            result,
            Err(LobbyError::User(UserError::UserNotFound(name))) if name == TEST_PLAYER_NAME
        ));
    }

    #[test]
    fn GIVEN_lobby_WHEN_get_mut_player_THEN_ok() {
        // GIVEN
        let mut lobby =
            Lobby::<u8>::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        lobby
            .add_player(create_test_player(TEST_PLAYER_NAME))
            .unwrap();
        // WHEN
        let player = lobby.get_mut_player(TEST_PLAYER_NAME).unwrap();
        // THEN
        assert_eq!(player.get_name(), TEST_PLAYER_NAME);
        player.set_input(String::new()).unwrap(); // this line will not compile if not mut
    }

    #[test]
    fn GIVEN_empty_lobby_WHEN_remove_player_THEN_err() {
        // GIVEN
        let mut lobby =
            Lobby::<u8>::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        // WHEN
        let result = lobby.remove_player(TEST_PLAYER_NAME);
        // THEN
        assert!(matches!(
            result,
            Err(LobbyError::User(UserError::UserNotFound(name))) if name == TEST_PLAYER_NAME
        ));
    }

    #[test]
    fn GIVEN_lobby_WHEN_remove_player_THEN_ok() {
        // GIVEN
        let mut lobby =
            Lobby::<u8>::new(TEST_LOBBY_NAME.to_string(), TEST_LOBBY_PASSWORD.to_string());
        lobby
            .add_player(create_test_player(TEST_PLAYER_NAME))
            .unwrap();
        // WHEN
        let player = lobby.remove_player(TEST_PLAYER_NAME).unwrap();
        // THEN
        assert_eq!(player.get_name(), TEST_PLAYER_NAME);
        assert_eq!(player.get_input(), TEST_PLAYER_INPUT);
        assert_eq!(player.get_points(), TEST_PLAYER_POINTS);
        assert_eq!(player.get_wager(), TEST_PLAYER_WAGER);
    }
}
