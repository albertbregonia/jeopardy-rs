pub mod actor_lobby;
pub use actor_lobby::*;

use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::player::{Player, ReadPlayerCollection, WritePlayerCollection};

// just a type alias to stop having to type this everywhere
pub type Responder<T> = oneshot::Sender<T>;
pub type Reply<T> = oneshot::Receiver<T>;

// Game is 'static (no non-static borrows) and Send to safely move ownership (to the actor lobby)
pub trait Game: Send + 'static {
    // `Collection` may be confusing:
    // - underlying Lobby manages the player lifetimes
    // - game simply reads and updates them
    // therefore, handle event should have knowledge of the current players but should not be able to delete them (read only)
    // therefore, PlayerCollectionType has R + W traits but only exposes R in handle_event

    // lowk Collection shouldn't be here and should be abstracted at the lobby level
    // bc the game doesn't NEED to know how the players are collected, they just need a ref to players
    // but writing <G:Game, P: ReadPlayers<Game::Player> + WritePlayers<Game::Player>> everywhere in lobby is not as clean
    type Collection: ReadPlayerCollection<Self::Player> + WritePlayerCollection<Self::Player>;

    type Player: Player;
    type Event: Send;
    type EventResponse: Send;

    fn handle_event(
        &mut self,
        // may seem contradictory, &mut allows the players to be mutable but not the collection
        players: &mut dyn ReadPlayerCollection<Self::Player>,
        event: Self::Event,
    ) -> Self::EventResponse;

    // helper function so i don't have to type this everywhere
    fn handle_reply<T>(responder: Responder<T>, value: T) {
        let _ = responder.send(value);
    }
}

#[derive(Debug, Error)]
pub enum LobbyError {
    #[error("Player ID: '{0}' conflicts with another in the lobby")]
    PlayerIDConflict(String),
    #[error("Player ID: '{0}' was not found in the lobby")]
    PlayerIDNotFound(String),
    #[error("Lobby closed due to actor thread shutdown")]
    ActorShutdown,
}

// named specifically as there are many SendError/RecvError variants per channel type

// can't send to ActorLobby bc it's shutdown
impl<T> From<mpsc::error::SendError<T>> for LobbyError {
    fn from(_e: mpsc::error::SendError<T>) -> Self {
        LobbyError::ActorShutdown
    }
}

// can't wait for response from ActorLobby bc it's shutdown
impl From<oneshot::error::RecvError> for LobbyError {
    fn from(_e: oneshot::error::RecvError) -> Self {
        LobbyError::ActorShutdown
    }
}
