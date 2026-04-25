use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{
    PlayerResponse,
    web::game::{LobbyManager, LobbyMap},
};

// NOTE: this file is the top level for application level definitions
// current implementation of the jeopardy webserver uses:
// - LobbyMap as the LobbyManager
// - PlayerResponse as the output type that interfaces with the frontend
type ResponseType = PlayerResponse;

pub struct GlobalState<M: LobbyManager<ResponseType>> {
    manager: M,
}

// ngl I don't like this...
// in the way that, now, the type has to be specified
// wherever in use as opposed to being abstracted fully
// ie. if I use something else other than LobbyMap I have to fix the specifier
// nbd... but it is annoying
// and I can't use a Box<dyn> bc it violates the 'static constraint
impl GlobalState<LobbyMap<ResponseType>> {
    pub fn new() -> Self {
        Self {
            manager: LobbyMap::new(),
        }
    }
}

pub type JeopardyGlobalState = Arc<RwLock<GlobalState<LobbyMap<ResponseType>>>>;
