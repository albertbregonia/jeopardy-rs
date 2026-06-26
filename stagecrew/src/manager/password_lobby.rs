use super::ManagerEntry;
use crate::lobby::{Game, Lobby};

pub struct PasswordProtectedLobby<G: Game> {
    id: String,
    password: String,
    lobby: Lobby<G>,
}

impl<G: Game> PasswordProtectedLobby<G> {
    // getters only - immutable after creation
    pub fn new(id: String, password: String, lobby: Lobby<G>) -> Self {
        Self {
            id,
            password,
            lobby,
        }
    }

    pub fn is_correct_password(&self, password: &str) -> bool {
        self.password == password
    }
}

impl<G: Game> ManagerEntry for PasswordProtectedLobby<G> {
    type Game = G;
    fn lobby(&self) -> &Lobby<G> {
        &self.lobby
    }

    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
pub mod password_protected_lobby_test_constructs {
    use crate::{
        lobby::{Lobby, lobby_test_constructs::TestGame},
        manager::PasswordProtectedLobby,
        player::player_map::PlayerMap,
    };

    pub fn new_test_password_protected_lobby(
        id: String,
        password: String,
    ) -> PasswordProtectedLobby<TestGame> {
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 1);
        PasswordProtectedLobby::new(id.clone(), password, lobby)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod password_lobby_tests {
    use crate::manager::{
        ManagerEntry, password_protected_lobby_test_constructs::new_test_password_protected_lobby,
    };

    #[tokio::test]
    async fn GIVEN_pw_lobby_WHEN_check_password_THEN_ok() {
        let password = "12345".to_string(); // very secure, NSA core
        let lobby = new_test_password_protected_lobby("1".to_string(), password.clone());
        assert!(lobby.is_correct_password(&password));
    }

    #[tokio::test]
    async fn GIVEN_incorrect_pw_for_pw_lobby_WHEN_check_password_THEN_ok() {
        let lobby = new_test_password_protected_lobby("1".to_string(), "12345".to_string());
        assert_eq!(lobby.is_correct_password("please"), false);
    }

    #[tokio::test]
    async fn GIVEN_pw_lobby_WHEN_get_lobby_THEN_ok() {
        // pw lobby is just a simple wrapper
        // ensure we can get access to the underlying lobby
        // lowk a "test for testing"
        // but we're testing the concrete type matches the trait expectation
        let lobby = new_test_password_protected_lobby("1".to_string(), "".to_string());
        assert_eq!(lobby.lobby().is_shutdown(), false);
    }
}
