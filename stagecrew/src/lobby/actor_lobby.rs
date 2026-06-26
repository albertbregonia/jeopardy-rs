use super::{Game, LobbyError, Responder};
use crate::{
    lobby::Reply,
    player::{ReadPlayers, WritePlayers},
};
use tokio::{
    self,
    sync::{mpsc, oneshot},
};

// internal commands to interact with ActorLobby
// effectively the top level API signature for inputs and responses
enum Command<G: Game> {
    AddPlayer(Responder<Result<(), LobbyError>>, String, G::Player),
    RemovePlayer(Responder<Option<G::Player>>, String),
    PlayerCount(Responder<usize>),
    GameEvent(Responder<G::EventResponse>, G::Event),
    Shutdown(Responder<()>),
}

pub struct Lobby<G: Game> {
    // NOTE: due to the functionality of mpsc::Sender
    // when every instance of mpsc::Sender is dropped,
    // it will signal the mpsc::Receiver to drop
    // therefore in this case, if lobby_handle is dropped
    // and there are no clones of it,
    // the actor will be signaled to shutdown
    // after processing its last queued message
    // (no new messages will be queued)
    lobby_handle: mpsc::Sender<Command<G>>,
}

// public API struct for interacting with underlying ActorLobby
impl<G: Game> Lobby<G> {
    pub fn new(game: G, players: G::Collection, buffer_size: usize) -> Self {
        let (actor_lobby, lobby_handle) = ActorLobby::new(game, players, buffer_size);
        tokio::spawn(async {
            actor_lobby.run_game_loop().await;
        });
        Self { lobby_handle }
    }

    // helpers

    async fn send<F, T>(&self, event_builder: F) -> Result<Reply<T>, LobbyError>
    where
        F: FnOnce(Responder<T>) -> Command<G>,
    {
        let (responder, reply) = oneshot::channel();
        self.lobby_handle.send(event_builder(responder)).await?;
        Ok(reply)
    }

    async fn send_and_wait<F, T>(&self, event_builder: F) -> Result<T, LobbyError>
    where
        F: FnOnce(Responder<T>) -> Command<G>,
    {
        let reply = self.send(event_builder).await?;
        Ok(reply.await?)
    }

    // public api

    pub async fn send_game_event(
        &self,
        event: G::Event,
    ) -> Result<Reply<G::EventResponse>, LobbyError> {
        let reply = self
            .send(|responder| Command::GameEvent(responder, event))
            .await?;
        Ok(reply)
    }

    pub async fn send_game_event_and_wait(
        &self,
        event: G::Event,
    ) -> Result<G::EventResponse, LobbyError> {
        let reply = self.send_game_event(event).await?;
        Ok(reply.await?)
    }

    pub async fn add_player(&self, id: String, player: G::Player) -> Result<(), LobbyError> {
        self.send_and_wait(|responder| Command::AddPlayer(responder, id, player))
            .await?
    }

    pub async fn remove_player(&self, id: String) -> Result<Option<G::Player>, LobbyError> {
        self.send_and_wait(|responder| Command::RemovePlayer(responder, id))
            .await
    }

    pub async fn player_count(&self) -> Result<usize, LobbyError> {
        self.send_and_wait(|responder| Command::PlayerCount(responder))
            .await
    }

    pub async fn shutdown(&self) -> Result<Reply<()>, LobbyError> {
        let reply = self.send(|responder| Command::Shutdown(responder)).await?;
        Ok(reply)
    }

    pub fn is_shutdown(&self) -> bool {
        self.lobby_handle.is_closed()
    }
}

/// ActorLobby contains and manages its state (subsequently the game state)
/// by running its own thread and then ingesting/responding to commands
/// through the use of tokio channels such as `mpsc` and `oneshot`
struct ActorLobby<G: Game> {
    players: G::Collection,
    game: G,
    subscriber: mpsc::Receiver<Command<G>>,
}

impl<G: Game> ActorLobby<G> {
    pub fn new(
        game: G,
        players: G::Collection,
        buffer_size: usize,
    ) -> (Self, mpsc::Sender<Command<G>>) {
        let (publisher, subscriber) = mpsc::channel(buffer_size);
        let actor_lobby = Self {
            players,
            game,
            subscriber,
        };
        (actor_lobby, publisher)
    }

    pub async fn run_game_loop(mut self) {
        let mut shutdown_callback = None;
        while let Some(event) = self.subscriber.recv().await {
            match event {
                Command::AddPlayer(responder, id, player) => {
                    G::handle_reply(responder, self.add_player(id, player))
                }
                Command::RemovePlayer(responder, id) => {
                    G::handle_reply(responder, self.remove_player(id))
                }
                Command::GameEvent(responder, event) => {
                    G::handle_reply(responder, self.game.handle_event(&mut self.players, event));
                }
                Command::Shutdown(responder) => {
                    // this can close with more shutdown messages in the queue
                    // therefore, respond only to the first message
                    // others will simply error out with a recv error
                    // when their responder gets dropped by this function
                    if !self.subscriber.is_closed() {
                        self.subscriber.close();
                        shutdown_callback = Some(responder);
                    }
                }
                Command::PlayerCount(responder) => G::handle_reply(responder, self.players.len()),
            }
        }
        if let Some(responder) = shutdown_callback {
            let _ = responder.send(());
        }
    }

    fn add_player(&mut self, id: String, player: G::Player) -> Result<(), LobbyError> {
        // we do validation at this level bc it is integral to the type
        // any time a player's data could get overriden is bad for the Lobby
        // name validation should be at the caller level
        if self.players.contains(&id) {
            return Err(LobbyError::UserIDConflict(id));
        }
        self.players.add(id, player);
        Ok(())
    }

    fn remove_player(&mut self, id: String) -> Option<G::Player> {
        self.players.remove(&id)
    }
}

#[cfg(test)]
pub mod lobby_test_constructs {
    use crate::player::{Player, player_map::PlayerMap};

    use super::*;

    pub struct TestPlayer(pub String);
    impl Player for TestPlayer {
        fn id(&self) -> &str {
            &self.0
        }
    }
    #[derive(Default)]
    pub struct TestGame;

    pub enum TestEvent {
        HasPlayer(String),
    }
    pub enum TestEventResponse {
        HasPlayer(bool),
    }

    impl Game for TestGame {
        type Player = TestPlayer;
        type Collection = PlayerMap<Self::Player>;
        type Event = TestEvent;
        type EventResponse = TestEventResponse;

        fn handle_event(
            &mut self,
            players: &mut dyn ReadPlayers<Self::Player>,
            event: Self::Event,
        ) -> Self::EventResponse {
            match event {
                TestEvent::HasPlayer(id) => TestEventResponse::HasPlayer(players.contains(&id)),
            }
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod lobby_tests {
    use crate::lobby::actor_lobby::lobby_test_constructs::{
        TestEvent, TestEventResponse, TestGame, TestPlayer,
    };
    use crate::player::{Player, player_map::PlayerMap};

    use super::*;

    #[tokio::test]
    pub async fn GIVEN_lobby_WHEN_shutdown_THEN_ok() {
        // GIVEN
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 1);

        // WHEN
        let handle = lobby.shutdown().await.unwrap();

        // THEN
        assert_eq!(lobby.is_shutdown(), false); // lobby.shutdown() just sends the signal
        handle.await.unwrap(); // wait until actually shut down
        assert_eq!(lobby.is_shutdown(), true);
    }

    #[tokio::test]
    pub async fn GIVEN_already_shutdown_lobby_WHEN_shutdown_THEN_error() {
        // when 2 shutdowns are queued, both are successfully handled but
        // only the first one is acknowledged and given a valid handle to wait on for true shutdown.
        // any other recv handles with RecvError when their responder oneshot eventually gets dropped

        // it's better to rely on shutdown although it has been documented that
        // since Lobby{} holds the only publisher handle to the actor (and is private and cannot be cloned)
        // dropping Lobby and the publisher handle closes the subscriber, signaling the actor to shutdown

        // GIVEN
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 2);

        // WHEN
        let handle1 = lobby.shutdown().await.unwrap();
        let handle2 = lobby.shutdown().await.unwrap();

        // THEN
        handle1.await.unwrap();
        assert!(lobby.is_shutdown());
        assert!(matches!(
            handle2.await.map_err(|e| e.into()),
            Err(LobbyError::ActorShutdown)
        ));
    }

    #[tokio::test]
    pub async fn GIVEN_already_shutdown_lobby_WHEN_send_event_THEN_error() {
        // GIVEN
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 2);
        let handle1 = lobby.shutdown().await.unwrap();
        handle1.await.unwrap();
        assert!(lobby.is_shutdown());

        // WHEN - all of these should fail bc the actor is shut down
        // to be exhaustive, we should test that every command fails
        // but this suffices as they all share the same helper
        let shutdown_result = lobby.shutdown().await;
        let player_count_result = lobby.player_count().await;
        let remove_player_result = lobby.remove_player("".to_string()).await;

        // THEN
        assert!(matches!(shutdown_result, Err(LobbyError::ActorShutdown)));
        assert!(matches!(
            player_count_result,
            Err(LobbyError::ActorShutdown)
        ));
        assert!(matches!(
            remove_player_result,
            Err(LobbyError::ActorShutdown)
        ));
    }

    // moved to a helper because some operations require a player
    /// adds a player and performs validation given an empty lobby
    async fn add_player(id: String, lobby: &Lobby<TestGame>) {
        // GIVEN - lobby

        // WHEN
        lobby
            .add_player(id.clone(), TestPlayer(id.clone()))
            .await
            .unwrap();

        // THEN
        let count = lobby.player_count().await.unwrap();
        assert_eq!(count, 1); // player was added
        let result = lobby
            .send_game_event_and_wait(TestEvent::HasPlayer(id.clone()))
            .await
            .unwrap();
        assert!(matches!(
            result,
            TestEventResponse::HasPlayer(exists) if exists
        ));
    }

    #[tokio::test]
    pub async fn GIVEN_player_WHEN_add_player_THEN_ok() {
        // GIVEN
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 1);
        // WHEN / THEN
        add_player("1".to_string(), &lobby).await
    }

    #[tokio::test]
    pub async fn GIVEN_conflicting_player_id_WHEN_add_player_THEN_error() {
        // GIVEN
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 1);
        let id = "1".to_string();
        add_player(id.clone(), &lobby).await;

        // WHEN
        let result = lobby.add_player(id.clone(), TestPlayer(id.clone())).await;

        // THEN
        assert!(matches!(
            result,
            Err(LobbyError::UserIDConflict(conflict_id)) if conflict_id == id
        ));
        let count = lobby.player_count().await.unwrap();
        assert_eq!(count, 1); // no new player was added
    }

    #[tokio::test]
    pub async fn GIVEN_player_in_lobby_WHEN_remove_player_THEN_ok() {
        // GIVEN
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 1);
        let id = "1".to_string();
        add_player(id.clone(), &lobby).await;

        // WHEN
        let result = lobby.remove_player(id.clone()).await.unwrap();

        // THEN
        assert!(matches!(
            result,
            Some(test_player) if test_player.id() == id // removed player was returned
        ));
        let count = lobby.player_count().await.unwrap();
        assert_eq!(count, 0); // player count went down bc of the remove
        let result = lobby
            .send_game_event_and_wait(TestEvent::HasPlayer(id))
            .await
            .unwrap();
        assert!(matches!(
            result, // HasPlayer says the player doesn't exist
            TestEventResponse::HasPlayer(exists) if !exists
        ));
    }

    #[tokio::test]
    pub async fn GIVEN_player_not_in_lobby_WHEN_remove_player_THEN_ok() {
        // GIVEN
        let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), 1);
        // add dummy player, bc an empty lobby will always return empty even if logic is wrong
        add_player("2".to_string(), &lobby).await;

        // WHEN
        let result = lobby.remove_player("1".to_string()).await;
        assert!(matches!(result, Ok(None))); // operation passed but no player was found/removed

        // THEN
        let count = lobby.player_count().await.unwrap();
        assert_eq!(count, 1); // player count is still 1
    }
}
