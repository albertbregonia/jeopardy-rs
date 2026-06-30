use serde::{Deserialize, Serialize};
use stagecrew::conn::{JsonConn, TextTransport};
use thiserror::Error;

use crate::{
    game::commands::player::{PlayerCommand, PlayerCommandResponse},
    server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric},
};

#[derive(Debug, Clone, Deserialize)]
pub struct LoginCredentials {
    lobby_id: String,
    lobby_password: String,
    username: String,
}

// variants of what can be sent by the player over the websocket
#[derive(Debug, Clone, Deserialize)]
pub enum PlayerRequest {
    Login(LoginCredentials),
    Command(PlayerCommand),
}

// response type for player
#[derive(Debug, Serialize)]
pub struct PlayerResponse {
    pub value: Option<PlayerCommandResponse>,
    pub error: Option<String>,
}

// shallow wrapper over JsonConn to interface the user with the game
// implicits:
// - all recoverable user errors are sent back to the user as a plain response
// - all internal server errors / irrecoverable user errors are returned from the function.
//   meaning, a function does not return an error unless it believes it cannot/should not continue
pub struct PlayerConn<T: TextTransport, M: ManagerGeneric, C: CredsValidatorGeneric> {
    state: GenericJeopardyServerState<M, C>,
    json_ws: JsonConn<T, PlayerRequest, PlayerResponse>,
    creds: Option<LoginCredentials>,
}

// top level error type
// at this level, we're essentially now defining what constitutes
// as an expected vs unexpected error. aka is this an issue because
// the user gave some wrong input or do we encounter an issue
// during normal operation and cannot do anything about it
#[derive(Debug, Error)]
pub enum PlayerHandlerError {
    #[error(transparent)]
    User(#[from] UserError),
    #[error(transparent)]
    Internal(#[from] InternalError),
}

impl From<PlayerHandlerError> for PlayerResponse {
    fn from(e: PlayerHandlerError) -> Self {
        Self {
            value: None,
            error: Some(e.to_string()),
        }
    }
}

#[derive(Debug, Error)]
pub enum UserError {}

#[derive(Debug, Error)]
pub enum InternalError {}
