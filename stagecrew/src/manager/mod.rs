use thiserror::Error;

use crate::lobby::{Game, actor_lobby::Lobby};

mod map_manager;
pub use map_manager::*;

mod password_lobby;
pub use password_lobby::*;

#[cfg(feature = "test-util")]
mod test_manager;
#[cfg(feature = "test-util")]
pub use test_manager::*;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("Requested entry '{0}' not found ")]
    EntryNotFound(String),
    // unused, saved to allow for custom get/add/remove rules based on ManagerEntry
    #[error(transparent)]
    Dependency(Box<dyn std::error::Error + Send + Sync>),
    #[error("Entry ID: '{0}' already exists")]
    EntryIDConflict(String),
}

// simple trait to decouple variants of entries in Manager
// some may be password-protected (ie. `PasswordProtectedLobby`), some may have other metadata
pub trait ManagerEntry {
    type Game: Game;
    /// `id` primarily serves as a decoupling
    /// the corresponding `Manager` may not have keys
    fn id(&self) -> &str;
    fn lobby(&self) -> &Lobby<Self::Game>;
}

pub trait Manager {
    type Entry: ManagerEntry;
    fn has(&self, id: &str) -> Result<bool, ManagerError>;
    fn get(&self, id: &str) -> Result<&Self::Entry, ManagerError>;
    fn add(&mut self, id: &str, entry: Self::Entry) -> Result<(), ManagerError>;
    fn remove(&mut self, id: &str) -> Result<Self::Entry, ManagerError>;
    fn len(&self) -> Result<usize, ManagerError>;
}
