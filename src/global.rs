use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{
    PlayerRequest, PlayerResponse,
    web::game::{LobbyManager, LobbyMap},
};

// NOTE: this file is the top level for application level definitions
// current implementation of the jeopardy webserver uses:
// - LobbyMap as the LobbyManager
// - PlayerResponse as the output type that interfaces with the frontend

// type aliases so that if these signatures change, it will be propagated everywhere
pub type RequestType = PlayerRequest;
pub type ResponseType = PlayerResponse;
pub type JeopardyGlobalState = Arc<RwLock<GlobalState>>;
type Manager = dyn LobbyManager<ResponseType> + Send + Sync;

pub struct GlobalState {
    manager: Box<Manager>,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            manager: Box::new(LobbyMap::default()),
        }
    }

    pub fn get_manager(&self) -> &Box<Manager> {
        &self.manager
    }

    pub fn get_mut_manager(&mut self) -> &mut Box<Manager> {
        &mut self.manager
    }
}
