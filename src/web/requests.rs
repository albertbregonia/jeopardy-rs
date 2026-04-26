use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum PlayerRequest {
    Login {
        username: String,
        lobby_name: String,
        password: String,
    },
    Buzzer,
}

#[derive(Serialize, Deserialize)]
pub enum PlayerResponse {}

#[derive(Serialize, Deserialize)]
pub enum HostRequest {}
