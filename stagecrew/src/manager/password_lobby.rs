use super::ManagerEntry;
#[cfg(test)]
use crate::lobby::lobby_test_constructs::TestGame;
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

#[cfg(test)]
impl PasswordProtectedLobby<TestGame> {
    pub fn with_test_game(id: String, password: String) -> Self {
        use crate::player::player_map::PlayerMap;

        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 1);
        Self::new(id.clone(), password, lobby)
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
#[allow(non_snake_case)]
mod password_lobby_tests {
    use crate::manager::{ManagerEntry, PasswordProtectedLobby};

    #[tokio::test]
    async fn GIVEN_pw_lobby_WHEN_check_password_THEN_ok() {
        let password = "12345".to_string(); // very secure, NSA core
        let lobby = PasswordProtectedLobby::with_test_game("1".to_string(), password.clone());
        assert!(lobby.is_correct_password(&password));
    }

    #[tokio::test]
    async fn GIVEN_incorrect_pw_for_pw_lobby_WHEN_check_password_THEN_ok() {
        let lobby = PasswordProtectedLobby::with_test_game("1".to_string(), "12345".to_string());
        assert_eq!(false, lobby.is_correct_password("please")); // incorrect password test
    }

    #[tokio::test]
    async fn GIVEN_pw_lobby_WHEN_get_lobby_THEN_ok() {
        // pw lobby is just a simple wrapper
        // ensure we can get access to the underlying lobby
        // lowk a "test for testing"
        // but we're testing the concrete type matches the trait expectation
        let entry = PasswordProtectedLobby::with_test_game("1".to_string(), "".to_string());
        assert_eq!(false, entry.lobby().is_shutdown());
    }
}
