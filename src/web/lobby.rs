use std::collections::HashMap;

pub struct Lobby {
    
}

pub struct LobbyMap {
    // NOTE: this is effectively our database, there is no long term persistence [for now?]
    lobbies: HashMap<String, Lobby> // keys are lobby name, values are `Lobby` instances
}

impl LobbyMap {
    pub fn new() -> Self {
        Self {
            lobbies: HashMap::new()
        }
    }
}