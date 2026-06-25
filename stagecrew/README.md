# stagecrew

`stagecrew` or *Stage Crew* is a Rust library built on `tokio` for ephemeral, in-memory, thread-safe, game lobbies.
- Each lobby follows the actor model in which each `Lobby` struct contains a `mpsc::Sender` handle to a dedicated task that handles players and game state.
- Simply create a struct for game state and implement the trait `Game` to hook into the functionality.
- Also includes traits for managing collections of lobbies (`Manager`) and small wrappers to contain metadata about each lobby (`ManagerEntry`)
- I built this because I wanted a lightweight, reusable library for my multiplayer web games and I named it stage crew because this library provides the setup for the *actors*.

# Example
```rust
// NOTE: largely taken from the unit tests in `actor_lobby.rs`
// NOTE: PlayerMap is library defined but not required
// it simply needs a struct that impl's the traits: ReadPlayers + WritePlayers

use stagecrew::{lobby::{Game, Lobby}, player::{Player, ReadPlayers, player_map::PlayerMap}};

// define player type for Game trait
pub struct TestPlayer(pub String);
impl Player for TestPlayer {
    fn id(&self) -> &str {
        &self.0
    }
}

// define concrete Game variant for Game trait
#[derive(Default)]
pub struct TestGame;

// define input/output message types for Game trait
pub enum TestEvent {
    Generic(String),
}
pub enum TestEventResponse {
    Generic(String),
}

// impl the Game trait for our struct
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
            TestEvent::Generic(msg) => TestEventResponse::Generic(format!("echo! {msg}")),
        }
    }
}

#[tokio::main]
async fn main() {
    let buffer_size = 1; // underlying we use an mpsc::channel so define a buffer size for game messages
    let lobby = Lobby::new(TestGame::default(), PlayerMap::new(), buffer_size);

    let TestEventResponse::Generic(msg) = lobby // send our own custom request and receive our response
        .send_game_event_and_wait(TestEvent::Generic("123".to_string()))
        .await
        .unwrap(); // unwrap bc the call may fail if the lobby.shutdown() was called

    println!("Received valid response from lobby! {msg}");

    // shutdown cleanly, however dropping the lobby will also shut down the actor
    let handle = lobby.shutdown().await.unwrap();

    // wait until the lobby is actually shutdown
    handle.await.unwrap();
}
```