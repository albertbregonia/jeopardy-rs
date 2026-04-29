use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum PlayerRequest {
    Login {
        username: String,
        lobby_name: String,
        password: String,
    },
    Buzzer,
    CreateLobby {
        lobby_name: String,
        password: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PlayerResponse {
    UserError { error_msg: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HostRequest {}
